//! Repository build engine.
//!
//! Produces, for a folder of mods:
//! * `.zsync` sidecars (HTTP repositories) and optional `.pbo.zip` sidecars;
//! * the legacy `.a3s/{sync,serverinfo,changelogs,events,autoconfig}` files;
//! * the modern `.axs/manifest.<rev>.json.zst` + `.axs/index.json`.
//!
//! Unlike the legacy builder, metadata is never deleted up-front: sidecars are
//! produced first and metadata files are replaced atomically at the end, with
//! `serverinfo` (what clients poll) written last.

mod hash;
mod meta;
mod scan;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rayon::prelude::*;

use axs_legacy::{FavoriteServer, Protocol, ServerInfo, SyncLeaf};
use axs_model::{EventDef, GameId, Manifest};

pub use hash::{hash_file, hex, CachedHash, FileHashCache, FileHashes, HashCache};
pub use meta::{
    addon_names, diff_changelog, read_previous, remove_stale_sidecars, write_metadata,
    PreviousBuild, RenderedMetadata,
};
pub use scan::{is_excluded_name, scan, Scan, ScannedFile};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),
    #[error("build cancelled")]
    Cancelled,
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, BuildError>;

/// Settings for one build.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub root: PathBuf,
    pub repo_name: String,
    pub game: GameId,
    /// Download protocol published in the autoconfig (base URL + credentials).
    /// HTTP(S) protocols also enable `.zsync` generation.
    pub protocol: Option<Protocol>,
    /// Embed `protocol`'s login/password in the public autoconfig.
    pub embed_credentials: bool,
    /// Force zsync generation on/off (default: on for HTTP protocols).
    pub zsync: Option<bool>,
    /// Create `.pbo.zip` sidecars.
    pub compress_pbo: bool,
    pub compressed_pbo_only: bool,
    pub connections: u32,
    pub partial_transfer: bool,
    /// Relative paths of files to leave out.
    pub excluded_files: Vec<String>,
    /// Relative folder paths clients should not clean up (`hiddenFolderPaths`).
    pub hidden_paths: Vec<String>,
    pub favorite_servers: Vec<FavoriteServer>,
    /// Events to publish; `None` carries the previous build's events forward.
    pub events: Option<Vec<EventDef>>,
    pub emit_legacy: bool,
    pub emit_modern: bool,
    /// Worker threads for hashing (0 = all cores).
    pub threads: usize,
}

impl BuildOptions {
    pub fn new(root: impl Into<PathBuf>, repo_name: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            repo_name: repo_name.into(),
            game: GameId::Arma3,
            protocol: None,
            embed_credentials: true,
            zsync: None,
            compress_pbo: false,
            compressed_pbo_only: false,
            connections: 1,
            partial_transfer: true,
            excluded_files: vec![],
            hidden_paths: vec![],
            favorite_servers: vec![],
            events: None,
            emit_legacy: true,
            emit_modern: true,
            threads: 0,
        }
    }

    fn zsync_enabled(&self) -> bool {
        self.zsync.unwrap_or_else(|| {
            matches!(
                self.protocol.as_ref().map(|p| p.class),
                Some(axs_legacy::ProtocolClass::Http) | None
            )
        })
    }
}

/// Progress notifications.
#[derive(Debug, Clone)]
pub enum BuildEvent {
    Scanned {
        files: usize,
        bytes: u64,
    },
    Hashing {
        done: usize,
        total: usize,
        bytes_done: u64,
        bytes_total: u64,
    },
    Compressing {
        done: usize,
        total: usize,
    },
    WritingMetadata,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct BuildReport {
    pub revision: u32,
    pub files: usize,
    pub total_size: u64,
    pub hashed_files: usize,
    pub hashed_bytes: u64,
    pub zsync_written: usize,
    pub zips_written: usize,
    pub sidecars_removed: usize,
    pub content_updated: bool,
    pub new_addons: Vec<String>,
    pub updated_addons: Vec<String>,
    pub deleted_addons: Vec<String>,
    pub duration_ms: u64,
}

/// Everything computed for a build, before anything is published.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub report: BuildReport,
    pub metadata: RenderedMetadata,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Write `data` to `path` via a temporary file and rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    rename_replace(&tmp, path)
}

pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{}.axs-tmp", std::process::id()));
    path.with_file_name(name)
}

/// Rename over an existing file, retrying briefly on Windows sharing errors.
pub fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut delay = 20;
    for attempt in 0.. {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e)
                if attempt < 8
                    && cfg!(windows)
                    && e.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay *= 2;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Run a complete build and publish the metadata.
pub fn build(
    opts: &BuildOptions,
    cache: &mut dyn HashCache,
    progress: &(dyn Fn(BuildEvent) + Sync),
    cancel: &AtomicBool,
) -> Result<BuildReport> {
    let prepared = prepare(opts, cache, progress, cancel)?;
    progress(BuildEvent::WritingMetadata);
    write_metadata(&opts.root, &prepared.metadata)
        .map_err(|e| BuildError::Io(opts.root.clone(), e))?;
    cache
        .flush()
        .map_err(|e| BuildError::Io(opts.root.clone(), e))?;
    Ok(prepared.report)
}

/// Scan, hash, produce sidecars and render metadata (nothing published yet).
pub fn prepare(
    opts: &BuildOptions,
    cache: &mut dyn HashCache,
    progress: &(dyn Fn(BuildEvent) + Sync),
    cancel: &AtomicBool,
) -> Result<Prepared> {
    let scan = scan::scan(&opts.root, &opts.excluded_files)?;
    prepare_scan(opts, scan, true, cache, progress, cancel)
}

/// Like [`prepare`], for an already assembled (possibly virtual) file list:
/// files may live outside `opts.root` (e.g. in a staging folder); their
/// sidecars are written next to them. `cleanup_sidecars` removes stale
/// sidecars below `opts.root`.
pub fn prepare_scan(
    opts: &BuildOptions,
    scan: Scan,
    cleanup_sidecars: bool,
    cache: &mut dyn HashCache,
    progress: &(dyn Fn(BuildEvent) + Sync),
    cancel: &AtomicBool,
) -> Result<Prepared> {
    let started = Instant::now();
    let root = &opts.root;
    if !opts.game.supports_legacy() && opts.emit_legacy {
        return Err(BuildError::Invalid(format!(
            "{} repositories cannot publish the legacy format",
            opts.game
        )));
    }
    let total_bytes: u64 = scan.files.iter().map(|f| f.size).sum();
    progress(BuildEvent::Scanned {
        files: scan.files.len(),
        bytes: total_bytes,
    });

    let zsync_on = opts.zsync_enabled();
    let block_size = axs_zsync::LEGACY_BLOCK_SIZE;

    // ---- decide work per file ------------------------------------------
    struct Work<'a> {
        file: &'a ScannedFile,
        cached: Option<CachedHash>,
        need_hash: bool,
        need_zsync: bool,
    }
    let works: Vec<Work> = scan
        .files
        .iter()
        .map(|f| {
            let cached = cache
                .get(&f.rel)
                .filter(|c| c.size == f.size && c.mtime_ns == f.mtime_ns);
            let need_hash = cached.is_none();
            let need_zsync = zsync_on && {
                let zpath = sidecar(&f.abs, ".zsync");
                let exists = std::fs::metadata(&zpath)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false);
                need_hash
                    || !exists
                    || cached.as_ref().and_then(|c| c.zsync_sha1.as_deref())
                        != cached.as_ref().map(|c| c.sha1.as_str())
            };
            Work {
                file: f,
                cached,
                need_hash,
                need_zsync,
            }
        })
        .collect();

    let hash_total = works.iter().filter(|w| w.need_hash || w.need_zsync).count();
    let hash_bytes_total: u64 = works
        .iter()
        .filter(|w| w.need_hash || w.need_zsync)
        .map(|w| w.file.size)
        .sum();
    let done = AtomicUsize::new(0);
    let bytes_done = AtomicU64::new(0);
    let zsync_written = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads)
        .build()
        .map_err(|e| BuildError::Invalid(e.to_string()))?;

    let results: Vec<std::result::Result<(usize, CachedHash), BuildError>> = pool.install(|| {
        works
            .par_iter()
            .enumerate()
            .map(|(i, w)| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(BuildError::Cancelled);
                }
                let f = w.file;
                let mut entry = match (&w.cached, w.need_hash || w.need_zsync) {
                    (Some(c), false) => return Ok((i, c.clone())),
                    (c, _) => c.clone(),
                };
                let hashes = hash_file(&f.abs, w.need_zsync.then_some(block_size))
                    .map_err(|e| BuildError::Io(f.abs.clone(), e))?;
                let mut e = entry.take().unwrap_or(CachedHash {
                    size: f.size,
                    mtime_ns: f.mtime_ns,
                    sha1: String::new(),
                    blake3: String::new(),
                    zsync_sha1: None,
                    zip_sha1: None,
                });
                e.size = f.size;
                e.mtime_ns = f.mtime_ns;
                e.sha1 = hashes.sha1.clone();
                e.blake3 = hashes.blake3.clone();
                if let Some(sums) = hashes.block_sums {
                    let url = opts
                        .protocol
                        .as_ref()
                        .map(|p| axs_legacy::urls::file_url(p, &f.rel))
                        .unwrap_or_default();
                    let mtime_ms = (f.mtime_ns / 1_000_000) as i64;
                    let name = axs_model::paths::file_name(&f.rel);
                    let meta = axs_zsync::ZsyncMeta::new(
                        name, mtime_ms, f.size, &url, &e.sha1, block_size, &sums,
                    );
                    let zpath = sidecar(&f.abs, ".zsync");
                    write_atomic(&zpath, &meta.to_bytes())
                        .map_err(|err| BuildError::Io(zpath.clone(), err))?;
                    e.zsync_sha1 = Some(e.sha1.clone());
                    zsync_written.fetch_add(1, Ordering::Relaxed);
                }
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                let b = bytes_done.fetch_add(f.size, Ordering::Relaxed) + f.size;
                progress(BuildEvent::Hashing {
                    done: d,
                    total: hash_total,
                    bytes_done: b,
                    bytes_total: hash_bytes_total,
                });
                Ok((i, e))
            })
            .collect()
    });

    let mut hashes: Vec<Option<CachedHash>> = vec![None; works.len()];
    let mut hashed_files = 0;
    let mut hashed_bytes = 0;
    for r in results {
        let (i, e) = r?;
        if works[i].need_hash {
            hashed_files += 1;
            hashed_bytes += works[i].file.size;
        }
        hashes[i] = Some(e);
    }
    let mut hashes: Vec<CachedHash> = hashes
        .into_iter()
        .map(|h| h.expect("every file hashed"))
        .collect();

    // ---- .pbo.zip sidecars ---------------------------------------------
    let mut zips_written = 0;
    let mut zip_sizes: HashMap<usize, u64> = HashMap::new();
    if opts.compress_pbo {
        let todo: Vec<usize> = (0..works.len())
            .filter(|&i| is_pbo(&works[i].file.rel))
            .collect();
        let total = todo.len();
        let cdone = AtomicUsize::new(0);
        let made: Vec<std::result::Result<(usize, u64, bool), BuildError>> = pool.install(|| {
            todo.par_iter()
                .map(|&i| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(BuildError::Cancelled);
                    }
                    let f = works[i].file;
                    let zpath = sidecar(&f.abs, ".zip");
                    let fresh = hashes[i].zip_sha1.as_deref() == Some(hashes[i].sha1.as_str());
                    let existing = std::fs::metadata(&zpath)
                        .ok()
                        .map(|m| m.len())
                        .filter(|&l| l > 0);
                    let r = match (fresh, existing) {
                        (true, Some(len)) => (i, len, false),
                        _ => {
                            let len = meta::zip_single(&f.abs, &zpath)
                                .map_err(|e| BuildError::Io(zpath.clone(), e))?;
                            (i, len, true)
                        }
                    };
                    let d = cdone.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(BuildEvent::Compressing { done: d, total });
                    Ok(r)
                })
                .collect()
        });
        for r in made {
            let (i, len, written) = r?;
            if written {
                zips_written += 1;
            }
            hashes[i].zip_sha1 = Some(hashes[i].sha1.clone());
            zip_sizes.insert(i, len);
        }
    }

    // ---- stale sidecars --------------------------------------------------
    let live: HashSet<&str> = scan.files.iter().map(|f| f.rel.as_str()).collect();
    let sidecars_removed = if cleanup_sidecars {
        meta::remove_stale_sidecars(root, &live, zsync_on, opts.compress_pbo, &scan.dirs)
    } else {
        0
    };

    // ---- cache maintenance ----------------------------------------------
    for (w, h) in works.iter().zip(&hashes) {
        let mut h = h.clone();
        if !zsync_on {
            h.zsync_sha1 = None;
        }
        if !opts.compress_pbo {
            h.zip_sha1 = None;
        }
        cache.put(&w.file.rel, h);
    }
    cache.retain(&|k| live.contains(k));

    // ---- sync tree --------------------------------------------------------
    let index: HashMap<&str, usize> = scan
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.rel.as_str(), i))
        .collect();
    let tree = scan::build_tree(&scan, |f| {
        let i = index[f.rel.as_str()];
        SyncLeaf {
            name: axs_model::paths::file_name(&f.rel).to_owned(),
            sha1: Some(hashes[i].sha1.clone()),
            size: f.size as i64,
            compressed_size: zip_sizes.get(&i).copied().unwrap_or(0) as i64,
            compressed: zip_sizes.contains_key(&i),
            updated: false,
            deleted: false,
        }
    });

    // ---- metadata -----------------------------------------------------------
    let previous = read_previous(root);
    let files: Vec<axs_model::FileEntry> = scan
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| axs_model::FileEntry {
            path: f.rel.clone(),
            size: f.size,
            sha1: hashes[i].sha1.clone(),
            blake3: hashes[i].blake3.clone(),
            zsync: zsync_on,
            zip_size: zip_sizes.get(&i).copied(),
        })
        .collect();
    let rendered = meta::render(opts, &previous, tree, files, &scan, total_bytes, now_ms());
    let report = BuildReport {
        revision: rendered.revision,
        files: scan.files.len(),
        total_size: total_bytes,
        hashed_files,
        hashed_bytes,
        zsync_written: zsync_written.load(Ordering::Relaxed),
        zips_written,
        sidecars_removed,
        content_updated: rendered.content_updated,
        new_addons: rendered.changelog.new_addons.clone(),
        updated_addons: rendered.changelog.updated_addons.clone(),
        deleted_addons: rendered.changelog.deleted_addons.clone(),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    Ok(Prepared {
        report,
        metadata: rendered,
    })
}

/// `<file><ext>` next to a file (e.g. `.zsync`, `.zip`).
pub fn sidecar(abs: &Path, ext: &str) -> PathBuf {
    let mut name = abs
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(ext);
    abs.with_file_name(name)
}

pub fn is_pbo(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".pbo")
}

/// Convenience used by tests and simple callers.
pub fn server_info_of(m: &RenderedMetadata) -> &ServerInfo {
    &m.server_info
}

/// Modern manifest of a rendered build.
pub fn manifest_of(m: &RenderedMetadata) -> &Manifest {
    &m.manifest
}
