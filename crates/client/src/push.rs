//! Push a local folder to an ArmaXSync server repository (delta upload).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use axs_build::{FileHashCache, HashCache};
use axs_zsync::ZsyncMeta;

use crate::{ClientError, Result};

pub const DELTA_MAGIC: &[u8; 6] = b"AXSD1\n";

#[derive(Debug, Clone)]
pub struct PushTarget {
    /// Server base URL, e.g. `https://mods.example.com`.
    pub server: String,
    pub repo: String,
    pub token: String,
}

#[derive(Debug, Clone, Default)]
pub struct PushOptions {
    /// Repository folders covered by the push (empty = everything).
    pub scope: Vec<String>,
    /// Delete server content (inside the scope) that is missing locally.
    pub delete_missing: bool,
    /// Publish after uploading (otherwise leave a draft changeset).
    pub commit: bool,
    /// Publish even if the repository changed since the push started.
    pub force: bool,
}

#[derive(Debug, Clone)]
pub enum PushEvent {
    Hashing {
        done: u64,
        total: u64,
    },
    Planned {
        unchanged: usize,
        reused: usize,
        delta: usize,
        full: usize,
        deleted: usize,
        upload_bytes: u64,
    },
    Uploading {
        path: String,
        bytes: u64,
    },
    Uploaded {
        bytes: u64,
    },
    Publishing {
        job: i64,
    },
    JobLog(String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PushReport {
    pub changeset: i64,
    pub unchanged: usize,
    pub reused: usize,
    pub delta_files: usize,
    pub full_files: usize,
    pub deleted: usize,
    pub bytes_sent: u64,
    pub bytes_total: u64,
    pub revision: Option<u64>,
    pub job: Option<i64>,
}

#[derive(Serialize)]
struct PushFile {
    path: String,
    size: u64,
    sha1: String,
    blake3: String,
}

#[derive(Deserialize, Default)]
struct PushPlan {
    unchanged: usize,
    reused: Vec<String>,
    delta: Vec<String>,
    full: Vec<String>,
    deleted: Vec<String>,
}

struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Api {
    fn url(&self, p: &str) -> String {
        format!("{}/api/v1{p}", self.base.trim_end_matches('/'))
    }
    async fn json<T: serde::de::DeserializeOwned>(&self, rb: reqwest::RequestBuilder) -> Result<T> {
        let r = rb
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ClientError::Invalid(format!("server: {e}")))?;
        let st = r.status();
        let text = r.text().await.unwrap_or_default();
        if !st.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
                .unwrap_or(text);
            return Err(ClientError::Invalid(format!("server answered {st}: {msg}")));
        }
        serde_json::from_str(&text)
            .map_err(|e| ClientError::Invalid(format!("bad server response: {e}")))
    }
}

fn enc_path(p: &str) -> String {
    p.split('/')
        .map(|s| {
            percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Build a delta of `new_file` against the server's old version (described by
/// its `.zsync`). Returns None when a full upload is cheaper.
pub fn make_delta(
    meta: &ZsyncMeta,
    new_file: &Path,
    blake3_hex: &str,
) -> std::io::Result<Option<Vec<u8>>> {
    let data = std::fs::read(new_file)?;
    let map = axs_zsync::match_blocks_in(meta, &data, || false);
    if map.ratio() < 0.1 {
        return Ok(None);
    }
    let bs = meta.header.block_size as u64;
    let old_len = meta.header.length;
    // New-file offset → old block index.
    let mut at: Vec<(u64, u64)> = map
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.map(|off| (off, i as u64)))
        .collect();
    at.sort();
    let mut out = Vec::with_capacity(data.len() / 4 + 64);
    out.extend_from_slice(DELTA_MAGIC);
    out.extend_from_slice(&(data.len() as u64).to_be_bytes());
    out.extend_from_slice(&(bs as u32).to_be_bytes());
    let b3 = blake3::Hash::from_hex(blake3_hex).map_err(std::io::Error::other)?;
    out.extend_from_slice(b3.as_bytes());
    let new_len = data.len() as u64;
    let mut pos = 0u64;
    let mut pending: Option<(u64, u32)> = None; // (first block, count)
    let flush_copy = |out: &mut Vec<u8>, p: &mut Option<(u64, u32)>| {
        if let Some((idx, n)) = p.take() {
            out.push(1);
            out.extend_from_slice(&idx.to_be_bytes());
            out.extend_from_slice(&n.to_be_bytes());
        }
    };
    let data_rec = |out: &mut Vec<u8>, bytes: &[u8]| {
        for chunk in bytes.chunks(1 << 20) {
            out.push(2);
            out.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            out.extend_from_slice(chunk);
        }
    };
    for (off, idx) in at {
        if off < pos {
            continue;
        }
        let blen = bs.min(old_len.saturating_sub(idx * bs));
        if blen == 0 || off + blen > new_len {
            continue;
        }
        // A copied block shorter than bs must end the file exactly.
        if blen < bs && off + blen != new_len {
            continue;
        }
        if off > pos {
            flush_copy(&mut out, &mut pending);
            data_rec(&mut out, &data[pos as usize..off as usize]);
        }
        pending = match pending {
            Some((first, n)) if first + n as u64 == idx && blen == bs => Some((first, n + 1)),
            other => {
                let mut p = other;
                flush_copy(&mut out, &mut p);
                Some((idx, 1))
            }
        };
        pos = off + blen;
        if blen < bs {
            flush_copy(&mut out, &mut pending);
        }
    }
    flush_copy(&mut out, &mut pending);
    if pos < new_len {
        data_rec(&mut out, &data[pos as usize..]);
    }
    out.push(0);
    if out.len() as u64 > new_len * 9 / 10 {
        return Ok(None);
    }
    Ok(Some(out))
}

/// Push `source` to the server repository.
pub async fn push(
    target: &PushTarget,
    source: &Path,
    opts: &PushOptions,
    cache_path: Option<PathBuf>,
    events: Arc<dyn Fn(PushEvent) + Send + Sync>,
) -> Result<PushReport> {
    let api = Api {
        http: reqwest::Client::builder()
            .user_agent(axs_transport::USER_AGENT)
            .build()
            .map_err(|e| ClientError::Invalid(e.to_string()))?,
        base: target.server.clone(),
        token: target.token.clone(),
    };
    let slug = &target.repo;
    let repo: serde_json::Value = api
        .json(api.http.get(api.url(&format!("/repos/{slug}"))))
        .await?;
    let base_rev = repo["revision"].as_u64().unwrap_or(0);

    // Scan + hash (sha1 and blake3).
    let src = source.to_path_buf();
    let scope = opts.scope.clone();
    let ev2 = events.clone();
    let (files, dirs) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<(PushFile, PathBuf)>, Vec<String>)> {
            let scan = axs_build::scan(&src, &[])?;
            let in_scope = |p: &str| {
                scope.is_empty()
                    || scope
                        .iter()
                        .any(|s| axs_model::paths::is_under(p, s, false))
            };
            let mut cache = match cache_path {
                Some(p) => FileHashCache::load(p),
                None => FileHashCache::in_memory(),
            };
            let total: u64 = scan
                .files
                .iter()
                .filter(|f| in_scope(&f.rel))
                .map(|f| f.size)
                .sum();
            let mut done = 0u64;
            let mut out = vec![];
            for f in scan.files.iter().filter(|f| in_scope(&f.rel)) {
                let key = f.abs.to_string_lossy().into_owned();
                let h = match cache.get(&key).filter(|c| {
                    c.size == f.size && c.mtime_ns == f.mtime_ns && !c.blake3.is_empty()
                }) {
                    Some(c) => (c.sha1, c.blake3),
                    None => {
                        let h = axs_build::hash_file(&f.abs, None)?;
                        cache.put(
                            &key,
                            axs_build::CachedHash {
                                size: f.size,
                                mtime_ns: f.mtime_ns,
                                sha1: h.sha1.clone(),
                                blake3: h.blake3.clone(),
                                zsync_sha1: None,
                                zip_sha1: None,
                            },
                        );
                        (h.sha1, h.blake3)
                    }
                };
                done += f.size;
                ev2(PushEvent::Hashing { done, total });
                out.push((
                    PushFile {
                        path: f.rel.clone(),
                        size: f.size,
                        sha1: h.0,
                        blake3: h.1,
                    },
                    f.abs.clone(),
                ));
            }
            let _ = cache.flush();
            let dirs = scan.dirs.iter().filter(|d| in_scope(d)).cloned().collect();
            Ok((out, dirs))
        })
        .await
        .map_err(|e| ClientError::Invalid(e.to_string()))??;

    let cs: serde_json::Value = api
        .json(
            api.http
                .post(api.url(&format!("/repos/{slug}/changesets")))
                .json(&serde_json::json!({"note": "push", "base_revision": base_rev})),
        )
        .await?;
    let cs = cs["id"]
        .as_i64()
        .ok_or_else(|| ClientError::Invalid("no changeset id".into()))?;
    let manifest = serde_json::json!({
        "files": files.iter().map(|(f, _)| f).collect::<Vec<_>>(),
        "scope": opts.scope,
        "delete_missing": opts.delete_missing,
        "dirs": dirs,
    });
    let plan: PushPlan = api
        .json(
            api.http
                .put(api.url(&format!("/changesets/{cs}/manifest")))
                .json(&manifest),
        )
        .await?;
    let local: HashMap<&str, (&PushFile, &PathBuf)> = files
        .iter()
        .map(|(f, p)| (f.path.as_str(), (f, p)))
        .collect();
    let upload_bytes: u64 = plan
        .delta
        .iter()
        .chain(plan.full.iter())
        .filter_map(|p| local.get(p.as_str()))
        .map(|(f, _)| f.size)
        .sum();
    events(PushEvent::Planned {
        unchanged: plan.unchanged,
        reused: plan.reused.len(),
        delta: plan.delta.len(),
        full: plan.full.len(),
        deleted: plan.deleted.len(),
        upload_bytes,
    });
    let mut report = PushReport {
        changeset: cs,
        unchanged: plan.unchanged,
        reused: plan.reused.len(),
        deleted: plan.deleted.len(),
        bytes_total: upload_bytes,
        ..Default::default()
    };

    let mut full = plan.full.clone();
    for p in &plan.delta {
        let (f, abs) = local[p.as_str()];
        let z = api
            .http
            .get(api.url(&format!("/repos/{slug}/zsync/{}", enc_path(p))))
            .bearer_auth(&api.token)
            .send()
            .await;
        let meta = match z {
            Ok(r) if r.status().is_success() => {
                r.bytes().await.ok().and_then(|b| ZsyncMeta::parse(&b).ok())
            }
            _ => None,
        };
        let Some(meta) = meta else {
            full.push(p.clone());
            continue;
        };
        let (abs2, b3) = (abs.clone(), f.blake3.clone());
        let delta = tokio::task::spawn_blocking(move || make_delta(&meta, &abs2, &b3))
            .await
            .map_err(|e| ClientError::Invalid(e.to_string()))??;
        let Some(delta) = delta else {
            full.push(p.clone());
            continue;
        };
        events(PushEvent::Uploading {
            path: p.clone(),
            bytes: delta.len() as u64,
        });
        let n = delta.len() as u64;
        let r: std::result::Result<serde_json::Value, _> = api
            .json(
                api.http
                    .put(api.url(&format!("/changesets/{cs}/delta/{}", enc_path(p))))
                    .body(delta),
            )
            .await;
        match r {
            Ok(_) => {
                report.delta_files += 1;
                report.bytes_sent += n;
                events(PushEvent::Uploaded { bytes: f.size });
            }
            Err(e) => {
                tracing::warn!("{p}: delta rejected ({e}); uploading in full");
                full.push(p.clone());
            }
        }
    }
    for p in &full {
        let (f, abs) = local[p.as_str()];
        events(PushEvent::Uploading {
            path: p.clone(),
            bytes: f.size,
        });
        let file = tokio::fs::File::open(abs).await?;
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
        let _: serde_json::Value = api
            .json(
                api.http
                    .put(api.url(&format!("/changesets/{cs}/files/{}", enc_path(p))))
                    .header("content-length", f.size)
                    .body(body),
            )
            .await?;
        report.full_files += 1;
        report.bytes_sent += f.size;
        events(PushEvent::Uploaded { bytes: f.size });
    }

    if !opts.commit {
        return Ok(report);
    }
    let r: serde_json::Value = api
        .json(
            api.http
                .post(api.url(&format!("/changesets/{cs}/commit")))
                .json(&serde_json::json!({"expected_revision": base_rev, "force": opts.force})),
        )
        .await?;
    let job = r["job"].as_i64().unwrap_or(0);
    report.job = Some(job);
    events(PushEvent::Publishing { job });
    let mut seen_log = 0usize;
    loop {
        let j: serde_json::Value = api
            .json(api.http.get(api.url(&format!("/jobs/{job}"))))
            .await?;
        if let Some(log) = j["log"].as_str() {
            for line in log.lines().skip(seen_log) {
                events(PushEvent::JobLog(line.to_owned()));
            }
            seen_log = log.lines().count();
        }
        match j["status"].as_str().unwrap_or("") {
            "queued" | "running" => tokio::time::sleep(std::time::Duration::from_millis(400)).await,
            "succeeded" => {
                report.revision = j["summary"]["revision"].as_u64();
                return Ok(report);
            }
            other => {
                return Err(ClientError::Invalid(format!(
                    "publish {other}: {}",
                    j["error"].as_str().unwrap_or("")
                )))
            }
        }
    }
}

/// Read an entire file (helper for tests).
pub fn read_all(p: &Path) -> std::io::Result<Vec<u8>> {
    let mut v = vec![];
    std::fs::File::open(p)?.read_to_end(&mut v)?;
    Ok(v)
}
