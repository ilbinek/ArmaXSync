//! Plan executor: concurrent downloads (full, resumed, segmented, zipped or
//! zsync delta), verification, atomic replacement and deletions.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use axs_model::paths::join_rel;
use axs_transport::{ChunkSink, RateLimiter, Transport, TransportError};
use axs_zsync::ZsyncMeta;

use crate::plan::{Op, SyncPlan};
use crate::{Result, SyncError};

#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub connections: usize,
    pub limiter: Option<Arc<RateLimiter>>,
    /// Range requests allowed (resume + zsync delta + segmented downloads).
    pub ranges: bool,
    /// Only `.pbo.zip` files exist for compressed PBOs.
    pub compressed_only: bool,
    /// Files at least this large are split into parallel range requests
    /// (0 disables; requires `ranges`).
    pub segment_threshold: u64,
    pub max_retries: u32,
    pub apply_deletes: bool,
    /// Use zsync delta only if at least this fraction of blocks is local.
    pub min_delta_ratio: f64,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            connections: 1,
            limiter: None,
            ranges: true,
            compressed_only: false,
            segment_threshold: 0,
            max_retries: 3,
            apply_deletes: true,
            min_delta_ratio: 0.15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Full,
    Resume,
    Segmented,
    Zip,
    Delta,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SyncEvent {
    Started {
        files: usize,
        bytes: u64,
        deletes: usize,
    },
    FileStarted {
        path: String,
        size: u64,
        method: Method,
    },
    Bytes {
        n: u64,
    },
    FileDone {
        path: String,
        transferred: u64,
        reused: u64,
    },
    FileFailed {
        path: String,
        error: String,
    },
    Deleted {
        path: String,
    },
    Finished {
        report: ExecReport,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExecReport {
    pub downloaded: usize,
    pub failed: Vec<(String, String)>,
    pub deleted: usize,
    pub bytes_transferred: u64,
    pub bytes_reused: u64,
    pub cancelled: bool,
}

impl ExecReport {
    pub fn ok(&self) -> bool {
        self.failed.is_empty() && !self.cancelled
    }
}

pub type EventFn = Arc<dyn Fn(SyncEvent) + Send + Sync>;

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut n = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    n.push(suffix);
    p.with_file_name(n)
}

/// Writes at a moving position of a file.
struct OffsetSink {
    file: std::fs::File,
    pos: u64,
}

impl ChunkSink for OffsetSink {
    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(self.pos))?;
        self.file.write_all(data)?;
        self.pos += data.len() as u64;
        Ok(())
    }
}

/// Execute a plan against `root`.
pub async fn execute(
    plan: &SyncPlan,
    t: &Transport,
    root: &Path,
    opts: &ExecOptions,
    events: EventFn,
    cancel: CancellationToken,
) -> ExecReport {
    let mut report = ExecReport::default();
    let fetches: Vec<&Op> = plan
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Fetch { .. }))
        .collect();
    events(SyncEvent::Started {
        files: plan.fetch_count,
        bytes: plan.fetch_bytes,
        deletes: plan.delete_count,
    });

    // Paths that must be created; deletes of conflicting entries run first.
    let created: HashSet<String> = plan
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Mkdir { path } | Op::Fetch { path, .. } => Some(path.to_lowercase()),
            _ => None,
        })
        .collect();
    let (pre_deletes, post_deletes): (Vec<&Op>, Vec<&Op>) = plan
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Delete { .. }))
        .partition(
            |o| matches!(o, Op::Delete { path, .. } if created.contains(&path.to_lowercase())),
        );
    for op in pre_deletes {
        delete_op(root, op, &events, &mut report);
    }
    for op in &plan.ops {
        if let Op::Mkdir { path } = op {
            if let Err(e) = std::fs::create_dir_all(join_rel(root, path)) {
                report.failed.push((path.clone(), e.to_string()));
            }
        }
    }

    let sem = Arc::new(Semaphore::new(opts.connections.max(1)));
    let transferred = Arc::new(AtomicU64::new(0));
    let mut set = tokio::task::JoinSet::new();
    for op in fetches {
        let Op::Fetch {
            path,
            size,
            sha1,
            blake3,
            zsync,
            zip_size,
            local_size,
        } = op.clone()
        else {
            unreachable!()
        };
        let sem = sem.clone();
        let t = t.clone();
        let root = root.to_path_buf();
        let opts = opts.clone();
        let events = events.clone();
        let cancel = cancel.clone();
        let transferred = transferred.clone();
        set.spawn(async move {
            let _permit = tokio::select! {
                p = sem.acquire_owned() => p.expect("semaphore"),
                _ = cancel.cancelled() => return (path, Err(SyncError::Cancelled)),
            };
            let job = FetchJob {
                path: path.clone(),
                size,
                sha1,
                blake3,
                zsync,
                zip_size,
                local_size,
            };
            let r = tokio::select! {
                r = fetch_with_retries(&t, &root, &job, &opts, &events, &transferred) => r,
                _ = cancel.cancelled() => Err(SyncError::Cancelled),
            };
            (path, r)
        });
    }
    while let Some(res) = set.join_next().await {
        match res {
            Ok((path, Ok((xfer, reused)))) => {
                report.downloaded += 1;
                report.bytes_reused += reused;
                events(SyncEvent::FileDone {
                    path,
                    transferred: xfer,
                    reused,
                });
            }
            Ok((_, Err(SyncError::Cancelled))) => report.cancelled = true,
            Ok((path, Err(e))) => {
                events(SyncEvent::FileFailed {
                    path: path.clone(),
                    error: e.to_string(),
                });
                report.failed.push((path, e.to_string()));
            }
            Err(e) => report.failed.push(("<task>".into(), e.to_string())),
        }
    }
    report.bytes_transferred = transferred.load(Ordering::Relaxed);
    if cancel.is_cancelled() {
        report.cancelled = true;
    }
    if report.ok() && opts.apply_deletes {
        for op in post_deletes {
            delete_op(root, op, &events, &mut report);
        }
    }
    events(SyncEvent::Finished {
        report: report.clone(),
    });
    report
}

fn delete_op(root: &Path, op: &Op, events: &EventFn, report: &mut ExecReport) {
    let Op::Delete { path, dir } = op else { return };
    let abs = join_rel(root, path);
    let r = if *dir {
        std::fs::remove_dir_all(&abs)
    } else {
        std::fs::remove_file(&abs)
    };
    match r {
        Ok(()) => {
            report.deleted += 1;
            events(SyncEvent::Deleted { path: path.clone() });
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => report.failed.push((path.clone(), format!("delete: {e}"))),
    }
}

#[derive(Debug, Clone)]
struct FetchJob {
    path: String,
    size: u64,
    sha1: String,
    blake3: Option<String>,
    zsync: bool,
    zip_size: Option<u64>,
    local_size: Option<u64>,
}

async fn fetch_with_retries(
    t: &Transport,
    root: &Path,
    job: &FetchJob,
    opts: &ExecOptions,
    events: &EventFn,
    transferred: &Arc<AtomicU64>,
) -> Result<(u64, u64)> {
    let mut attempt = 0;
    let mut force_full = false;
    loop {
        match fetch_once(t, root, job, opts, events, transferred, force_full).await {
            Ok(r) => return Ok(r),
            Err(SyncError::Verify { .. }) if !force_full => {
                tracing::warn!(
                    "{}: verification failed, retrying with a full download",
                    job.path
                );
                force_full = true;
            }
            Err(e) if attempt < opts.max_retries && e.is_retryable() => {
                attempt += 1;
                let wait = Duration::from_secs([2, 5, 10, 20][(attempt as usize - 1).min(3)]);
                tracing::warn!(
                    "{}: {e}; retry {attempt}/{} in {wait:?}",
                    job.path,
                    opts.max_retries
                );
                tokio::time::sleep(wait).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| SyncError::Io(std::io::Error::other(e)))?
        .map_err(SyncError::Io)
}

async fn fetch_once(
    t: &Transport,
    root: &Path,
    job: &FetchJob,
    opts: &ExecOptions,
    events: &EventFn,
    transferred: &Arc<AtomicU64>,
    force_full: bool,
) -> Result<(u64, u64)> {
    let dest = join_rel(root, &job.path);
    let part = with_suffix(&dest, ".axs-part");
    let part_meta = with_suffix(&dest, ".axs-part.sha1");
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let counter = transferred.clone();
    let ev = events.clone();
    let on_bytes = move |n: u64| {
        counter.fetch_add(n, Ordering::Relaxed);
        ev(SyncEvent::Bytes { n });
    };
    let limiter = opts.limiter.as_deref();

    if job.size == 0 {
        events(SyncEvent::FileStarted {
            path: job.path.clone(),
            size: 0,
            method: Method::Full,
        });
        std::fs::write(&part, b"")?;
        finish(&part, &dest, &part_meta, job).await?;
        return Ok((0, 0));
    }

    // A partial download from an earlier run for the same content?
    let resumable = !force_full
        && std::fs::read_to_string(&part_meta)
            .map(|s| s.trim() == job.sha1)
            .unwrap_or(false)
        && std::fs::metadata(&part)
            .map(|m| m.len() > 0 && m.len() < job.size)
            .unwrap_or(false)
        && (opts.ranges || !t.is_http());

    // After a failed attempt, prefer the plain file: some legacy builders
    // (ArmA3Sync on certain JDKs) publish damaged .pbo.zip files.
    let use_zip = job.zip_size.is_some()
        && job.path.to_lowercase().ends_with(".pbo")
        && (opts.compressed_only || (!resumable && !force_full));
    let try_delta = !force_full
        && !resumable
        && opts.ranges
        && job.zsync
        && job.local_size.unwrap_or(0) > 0
        && !opts.compressed_only;

    if try_delta {
        match delta(t, job, &dest, &part, opts, events, &on_bytes).await {
            Ok(Some((xfer, reused))) => {
                finish(&part, &dest, &part_meta, job).await?;
                return Ok((xfer, reused));
            }
            Ok(None) => {} // not worth it / stale sidecar: full download below
            Err(SyncError::Transport(TransportError::NotFound(_))) => {}
            Err(e) => return Err(e),
        }
    }

    std::fs::write(&part_meta, &job.sha1)?;
    if use_zip {
        events(SyncEvent::FileStarted {
            path: job.path.clone(),
            size: job.zip_size.unwrap_or(0),
            method: Method::Zip,
        });
        let zpart = with_suffix(&dest, ".zip.axs-part");
        let mut f = std::fs::File::create(&zpart)?;
        let got = t
            .fetch(
                &format!("{}.zip", job.path),
                0,
                None,
                &mut f,
                limiter,
                &on_bytes,
            )
            .await?;
        drop(f);
        let (zp, pp) = (zpart.clone(), part.clone());
        let unzipped = blocking(move || unzip_first(&zp, &pp)).await;
        let _ = std::fs::remove_file(&zpart);
        if let Err(e) = unzipped {
            // Treat a damaged archive like a checksum failure so the caller
            // retries with the uncompressed file.
            let _ = std::fs::remove_file(&part);
            return Err(SyncError::Verify {
                path: job.path.clone(),
                expected: job.sha1.clone(),
                actual: format!("unreadable .pbo.zip ({e})"),
            });
        }
        finish(&part, &dest, &part_meta, job).await?;
        return Ok((got.bytes, 0));
    }

    if !resumable
        && opts.ranges
        && opts.segment_threshold > 0
        && job.size >= opts.segment_threshold
        && t.is_http()
    {
        events(SyncEvent::FileStarted {
            path: job.path.clone(),
            size: job.size,
            method: Method::Segmented,
        });
        let f = std::fs::File::create(&part)?;
        f.set_len(job.size)?;
        drop(f);
        let segs = (job.size / (32 << 20)).clamp(2, 4);
        let seg_len = job.size.div_ceil(segs);
        let mut futs = vec![];
        for i in 0..segs {
            let start = i * seg_len;
            let end = ((i + 1) * seg_len).min(job.size);
            let file = std::fs::OpenOptions::new().write(true).open(&part)?;
            let on_bytes = &on_bytes;
            let path = job.path.clone();
            futs.push(async move {
                let mut sink = OffsetSink { file, pos: start };
                let got = t
                    .fetch(&path, start, Some(end), &mut sink, limiter, on_bytes)
                    .await?;
                if got.bytes != end - start {
                    return Err(SyncError::Incomplete(path));
                }
                Ok::<u64, SyncError>(got.bytes)
            });
        }
        let results = futures::future::join_all(futs).await;
        let mut total = 0;
        for r in results {
            total += r?;
        }
        finish(&part, &dest, &part_meta, job).await?;
        return Ok((total, 0));
    }

    let start = if resumable {
        std::fs::metadata(&part)?.len()
    } else {
        0
    };
    events(SyncEvent::FileStarted {
        path: job.path.clone(),
        size: job.size - start,
        method: if start > 0 {
            Method::Resume
        } else {
            Method::Full
        },
    });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(start > 0)
        .write(true)
        .truncate(start == 0)
        .open(&part)?;
    let got = t
        .fetch(&job.path, start, None, &mut f, limiter, &on_bytes)
        .await?;
    f.flush()?;
    drop(f);
    let have = std::fs::metadata(&part)?.len();
    if have < job.size {
        return Err(SyncError::Incomplete(job.path.clone()));
    }
    finish(&part, &dest, &part_meta, job).await?;
    Ok((got.bytes, start))
}

/// Verify the part file and move it into place.
async fn finish(part: &Path, dest: &Path, part_meta: &Path, job: &FetchJob) -> Result<()> {
    let p = part.to_path_buf();
    let h = blocking(move || axs_build::hash_file(&p, None)).await?;
    let sha_ok = job.sha1.is_empty() || h.sha1 == job.sha1;
    let b3_ok = job.blake3.as_deref().map(|b| b == h.blake3).unwrap_or(true);
    if !sha_ok || !b3_ok {
        let _ = std::fs::remove_file(part);
        let _ = std::fs::remove_file(part_meta);
        return Err(SyncError::Verify {
            path: job.path.clone(),
            expected: job.sha1.clone(),
            actual: h.sha1,
        });
    }
    axs_build::rename_replace(part, dest)?;
    let _ = std::fs::remove_file(part_meta);
    Ok(())
}

fn unzip_first(zip_path: &Path, out: &Path) -> std::io::Result<()> {
    let mut za =
        zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(std::io::Error::other)?;
    if za.is_empty() {
        return Err(std::io::Error::other("empty zip"));
    }
    let mut entry = za.by_index(0).map_err(std::io::Error::other)?;
    let mut f = std::fs::File::create(out)?;
    std::io::copy(&mut entry, &mut f)?;
    f.flush()?;
    Ok(())
}

/// zsync delta: Ok(None) when a full download is preferable.
async fn delta(
    t: &Transport,
    job: &FetchJob,
    dest: &Path,
    part: &Path,
    opts: &ExecOptions,
    events: &EventFn,
    on_bytes: &(dyn Fn(u64) + Sync),
) -> Result<Option<(u64, u64)>> {
    let zbytes = t.get_bytes(&format!("{}.zsync", job.path)).await?;
    let meta = match ZsyncMeta::parse(&zbytes) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("{}.zsync unreadable: {e}", job.path);
            return Ok(None);
        }
    };
    if meta.header.length != job.size
        || (!meta.header.sha1.is_empty() && meta.header.sha1 != job.sha1)
    {
        tracing::warn!("{}.zsync is stale; downloading the whole file", job.path);
        return Ok(None);
    }
    let seed = dest.to_path_buf();
    let meta = Arc::new(meta);
    let m2 = meta.clone();
    let map = blocking(move || axs_zsync::match_blocks(&m2, &seed)).await?;
    if map.ratio() < opts.min_delta_ratio {
        return Ok(None);
    }
    let ranges = axs_zsync::plan_ranges(&map, job.size, 2, 16 << 20);
    let fetch_bytes: u64 = ranges.iter().map(|r| r.len()).sum();
    events(SyncEvent::FileStarted {
        path: job.path.clone(),
        size: fetch_bytes,
        method: Method::Delta,
    });

    // Copy matched blocks from the seed into the part file.
    let (seed, out, bs, size) = (
        dest.to_path_buf(),
        part.to_path_buf(),
        map.block_size as u64,
        job.size,
    );
    let blocks = map.blocks.clone();
    let reused = blocking(move || -> std::io::Result<u64> {
        let mut src = std::fs::File::open(&seed)?;
        let seed_len = src.metadata()?.len();
        let mut dst = std::fs::File::create(&out)?;
        dst.set_len(size)?;
        let mut buf = vec![0u8; bs as usize];
        let mut reused = 0u64;
        for (i, b) in blocks.iter().enumerate() {
            let Some(off) = *b else { continue };
            let start = i as u64 * bs;
            let len = bs.min(size - start) as usize;
            buf[..len].fill(0);
            let avail = seed_len.saturating_sub(off).min(len as u64) as usize;
            src.seek(SeekFrom::Start(off))?;
            src.read_exact(&mut buf[..avail])?;
            dst.seek(SeekFrom::Start(start))?;
            dst.write_all(&buf[..len])?;
            reused += len as u64;
        }
        dst.flush()?;
        Ok(reused)
    })
    .await?;

    let mut xfer = 0;
    for r in ranges {
        let file = std::fs::OpenOptions::new().write(true).open(part)?;
        let mut sink = OffsetSink { file, pos: r.start };
        let got = t
            .fetch(
                &job.path,
                r.start,
                Some(r.end),
                &mut sink,
                opts.limiter.as_deref(),
                on_bytes,
            )
            .await?;
        if got.bytes != r.len() {
            return Err(SyncError::Incomplete(job.path.clone()));
        }
        xfer += got.bytes;
    }
    Ok(Some((xfer, reused)))
}
