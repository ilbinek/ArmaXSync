//! Building and publishing repository content.
//!
//! Changesets are staged outside the live folder. Publishing hashes the staged
//! files, generates their sidecars in staging and renders the new metadata
//! *before* touching the live folder; the commit itself only renames files and
//! swaps metadata under the repository's commit lock.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use axs_build::{rename_replace, sidecar, BuildEvent, FileHashCache, HashCache, Scan, ScannedFile};
use axs_model::paths::{is_under, join_rel};

use crate::db::now;
use crate::jobs::JobCtx;
use crate::state::{Published, RepoRuntime, Shared};

/// One change in a changeset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum CsOp {
    /// File staged at `<staging>/files/<path>`.
    Put { path: String, size: u64 },
    /// Remove a file or folder (recursively).
    Delete { path: String },
    /// Create an (empty) folder.
    Mkdir { path: String },
}

impl CsOp {
    pub fn path(&self) -> &str {
        match self {
            CsOp::Put { path, .. } | CsOp::Delete { path } | CsOp::Mkdir { path } => path,
        }
    }
}

pub fn staging_dir(state: &Shared, slug: &str, cs: i64) -> PathBuf {
    state.cfg.staging_dir().join(slug).join(cs.to_string())
}

pub fn staged_file(state: &Shared, slug: &str, cs: i64, rel: &str) -> PathBuf {
    join_rel(&staging_dir(state, slug, cs).join("files"), rel)
}

/// Bridge a job's cancellation token to the build engine's flag.
fn cancel_flag(ctx: &JobCtx) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let tok = ctx.cancel.clone();
    tokio::spawn(async move {
        tok.cancelled().await;
        f2.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    flag
}

fn progress_fn(ctx: &JobCtx) -> impl Fn(BuildEvent) + Sync + Send + 'static {
    let ctx = ctx.clone();
    move |ev| match ev {
        BuildEvent::Scanned { files, bytes } => ctx.log(format!("{files} files, {bytes} bytes")),
        BuildEvent::Hashing {
            bytes_done,
            bytes_total,
            ..
        } => ctx.progress("hashing", bytes_done, bytes_total),
        BuildEvent::Compressing { done, total } => {
            ctx.progress("compressing", done as u64, total as u64)
        }
        BuildEvent::WritingMetadata => ctx.log("publishing metadata"),
    }
}

/// Swap rendered metadata into place (files + in-memory state).
async fn commit_metadata(
    rt: &RepoRuntime,
    rendered: &axs_build::RenderedMetadata,
) -> anyhow::Result<()> {
    let _g = rt.commit_lock.write().await;
    axs_build::write_metadata(&rt.root, rendered)?;
    rt.published.store(Arc::new(Published::load(&rt.root)));
    Ok(())
}

fn report_json(r: &axs_build::BuildReport) -> serde_json::Value {
    serde_json::to_value(r).unwrap_or_default()
}

/// Rebuild metadata from the live folder (external repos, settings/events changes).
pub async fn build_job(
    state: &Shared,
    rt: &Arc<RepoRuntime>,
    ctx: &JobCtx,
) -> anyhow::Result<serde_json::Value> {
    let opts = state.build_options(rt);
    ctx.log(format!(
        "building {} from {}",
        opts.repo_name,
        opts.root.display()
    ));
    let cache_path = state.cache_path(&rt.record().slug);
    let flag = cancel_flag(ctx);
    let progress = progress_fn(ctx);
    let (prepared, mut cache) = tokio::task::spawn_blocking(move || {
        let mut cache = FileHashCache::load(cache_path);
        let p = axs_build::prepare(&opts, &mut cache, &progress, &flag);
        (p, cache)
    })
    .await?;
    let prepared = prepared?;
    commit_metadata(rt, &prepared.metadata).await?;
    cache.flush()?;
    ctx.log(format!(
        "revision {} published ({} files)",
        prepared.report.revision, prepared.report.files
    ));
    Ok(report_json(&prepared.report))
}

fn move_to_trash(trash: &Path, live: &Path) -> std::io::Result<()> {
    if !live.exists() {
        return Ok(());
    }
    if let Some(d) = trash.parent() {
        std::fs::create_dir_all(d)?;
    }
    match std::fs::rename(live, trash) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Different filesystem: delete instead.
            if live.is_dir() {
                std::fs::remove_dir_all(live)
            } else {
                std::fs::remove_file(live)
            }
        }
    }
}

/// Load a changeset's ops.
pub async fn load_ops(state: &Shared, cs: i64) -> anyhow::Result<(i64, String, Vec<CsOp>)> {
    let (repo_id, status, ops): (i64, String, String) = state
        .db
        .call(move |c| {
            c.query_row(
                "SELECT repo_id, status, ops FROM changesets WHERE id=?1",
                [cs],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .await?;
    Ok((repo_id, status, serde_json::from_str(&ops)?))
}

/// Build the post-changeset virtual tree.
fn virtual_scan(
    root: &Path,
    exclude: &[String],
    staging_files: &Path,
    ops: &[CsOp],
) -> anyhow::Result<Scan> {
    let live = axs_build::scan(root, exclude)?;
    let mut files: BTreeMap<String, ScannedFile> =
        live.files.into_iter().map(|f| (f.rel.clone(), f)).collect();
    let mut dirs: BTreeSet<String> = live.dirs.into_iter().collect();
    for op in ops {
        match op {
            CsOp::Delete { path } => {
                files.retain(|k, _| !is_under(k, path, false));
                dirs.retain(|d| !is_under(d, path, false));
            }
            CsOp::Mkdir { path } => {
                dirs.insert(path.clone());
            }
            CsOp::Put { path, .. } => {
                let abs = join_rel(staging_files, path);
                let md =
                    std::fs::metadata(&abs).map_err(|e| anyhow::anyhow!("staged {path}: {e}"))?;
                if dirs.contains(path) {
                    anyhow::bail!("{path} is a folder; delete it first");
                }
                files.insert(
                    path.clone(),
                    ScannedFile {
                        rel: path.clone(),
                        abs,
                        size: md.len(),
                        mtime_ns: axs_build_mtime(&md),
                    },
                );
            }
        }
    }
    Ok(Scan::from_entries(files.into_values().collect(), dirs))
}

fn axs_build_mtime(md: &std::fs::Metadata) -> i128 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128)
        .unwrap_or(0)
}

/// Apply a changeset and publish a new revision.
pub async fn publish_changeset(
    state: &Shared,
    rt: &Arc<RepoRuntime>,
    cs: i64,
    ctx: &JobCtx,
) -> anyhow::Result<serde_json::Value> {
    let (repo_id, status, ops) = load_ops(state, cs).await?;
    let rec = rt.record();
    anyhow::ensure!(repo_id == rec.id, "changeset belongs to another repository");
    anyhow::ensure!(
        status == "draft" || status == "publishing",
        "changeset is {status}"
    );
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET status='publishing', updated_at=?1 WHERE id=?2",
                rusqlite::params![now(), cs],
            )
        })
        .await?;
    ctx.log(format!(
        "publishing changeset #{cs} ({} operations)",
        ops.len()
    ));

    let stage = staging_dir(state, &rec.slug, cs);
    let staging_files = stage.join("files");
    let opts = state.build_options(rt);
    let cache_path = state.cache_path(&rec.slug);
    let flag = cancel_flag(ctx);
    let progress = progress_fn(ctx);
    let (root, exclude, ops2, sf) = (
        rt.root.clone(),
        opts.excluded_files.clone(),
        ops.clone(),
        staging_files.clone(),
    );
    let (prepared, cache) = tokio::task::spawn_blocking(move || {
        let mut cache = FileHashCache::load(cache_path);
        let r = virtual_scan(&root, &exclude, &sf, &ops2).and_then(|scan| {
            Ok(axs_build::prepare_scan(
                &opts, scan, false, &mut cache, &progress, &flag,
            )?)
        });
        (r, cache)
    })
    .await?;
    let prepared = match prepared {
        Ok(p) => p,
        Err(e) => {
            state
                .db
                .call(move |c| {
                    c.execute(
                        "UPDATE changesets SET status='draft', updated_at=?1 WHERE id=?2",
                        rusqlite::params![now(), cs],
                    )
                })
                .await?;
            return Err(e);
        }
    };
    std::fs::write(stage.join("journal.json"), serde_json::to_vec(&ops)?)?;

    // ---- commit (renames only) -------------------------------------------
    let revision = prepared.metadata.revision;
    let trash = state
        .cfg
        .trash_dir()
        .join(&rec.slug)
        .join(format!("{revision}-{cs}"));
    {
        let _g = rt.commit_lock.write().await;
        for op in &ops {
            let live = join_rel(&rt.root, op.path());
            match op {
                CsOp::Delete { path } => {
                    let t = join_rel(&trash, path);
                    move_to_trash(&t, &live)?;
                    for ext in [".zsync", ".zip"] {
                        let _ = move_to_trash(&sidecar(&t, ext), &sidecar(&live, ext));
                    }
                }
                CsOp::Mkdir { .. } => std::fs::create_dir_all(&live)?,
                CsOp::Put { path, .. } => {
                    let staged = join_rel(&staging_files, path);
                    if let Some(d) = live.parent() {
                        std::fs::create_dir_all(d)?;
                    }
                    if live.is_dir() {
                        anyhow::bail!("{path} is a folder in the live repository");
                    }
                    rename_or_copy(&staged, &live)?;
                    for ext in [".zsync", ".zip"] {
                        let s = sidecar(&staged, ext);
                        let l = sidecar(&live, ext);
                        if s.exists() {
                            rename_or_copy(&s, &l)?;
                        } else {
                            let _ = std::fs::remove_file(&l);
                        }
                    }
                }
            }
        }
        axs_build::write_metadata(&rt.root, &prepared.metadata)?;
        rt.published.store(Arc::new(Published::load(&rt.root)));
    }
    let mut cache = cache;
    cache.flush()?;
    // Sidecars of files that vanished (e.g. compression switched off).
    let live_set: std::collections::HashSet<&str> = prepared
        .metadata
        .manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    let removed = axs_build::remove_stale_sidecars(
        &rt.root,
        &live_set,
        true,
        rec.settings.compress,
        &prepared.metadata.manifest.dirs,
    );
    if removed > 0 {
        ctx.log(format!("removed {removed} stale sidecar(s)"));
    }
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET status='published', updated_at=?1 WHERE id=?2",
                rusqlite::params![now(), cs],
            )
        })
        .await?;
    let _ = std::fs::remove_dir_all(&stage);
    ctx.log(format!("revision {revision} published"));
    Ok(report_json(&prepared.report))
}

fn rename_or_copy(from: &Path, to: &Path) -> std::io::Result<()> {
    match rename_replace(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Staging on another filesystem: copy to a temp name, then rename.
            let tmp = sidecar(to, ".axs-tmp");
            std::fs::copy(from, &tmp)?;
            rename_replace(&tmp, to)?;
            let _ = std::fs::remove_file(from);
            Ok(())
        }
    }
}

/// Finish commits interrupted by a crash (journal present), then rebuild.
pub async fn recover_journals(state: &Shared) -> anyhow::Result<()> {
    for rt in state.repo_list() {
        let rec = rt.record();
        let dir = state.cfg.staging_dir().join(&rec.slug);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            if !e.path().join("journal.json").exists() {
                continue;
            }
            let Ok(cs) = e.file_name().to_string_lossy().parse::<i64>() else {
                continue;
            };
            tracing::warn!(
                "repository {}: finishing interrupted publish of changeset {cs}",
                rec.slug
            );
            let ops: Vec<CsOp> =
                serde_json::from_slice(&std::fs::read(e.path().join("journal.json"))?)?;
            let files = e.path().join("files");
            for op in &ops {
                if let CsOp::Put { path, .. } = op {
                    let s = join_rel(&files, path);
                    if s.exists() {
                        let l = join_rel(&rt.root, path);
                        if let Some(d) = l.parent() {
                            std::fs::create_dir_all(d)?;
                        }
                        rename_or_copy(&s, &l)?;
                    }
                } else if let CsOp::Delete { path } = op {
                    let l = join_rel(&rt.root, path);
                    let _ = if l.is_dir() {
                        std::fs::remove_dir_all(&l)
                    } else {
                        std::fs::remove_file(&l)
                    };
                }
            }
            let _ = std::fs::remove_dir_all(e.path());
            state
                .db
                .call(move |c| {
                    c.execute(
                        "UPDATE changesets SET status='published', updated_at=?1 WHERE id=?2",
                        rusqlite::params![now(), cs],
                    )
                })
                .await?;
            crate::jobs::JobManager::submit(state, crate::jobs::JobKind::Build, Some(rec.id), None)
                .await?;
        }
    }
    Ok(())
}

/// Remove old trash, abandoned uploads and discarded staging folders.
pub async fn gc_job(state: &Shared, ctx: &JobCtx) -> anyhow::Result<serde_json::Value> {
    let mut removed = 0u64;
    let old = |p: &Path, ms: i64| -> bool {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|t| {
                t.elapsed()
                    .map(|e| e.as_millis() as i64 > ms)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    };
    if let Ok(rd) = std::fs::read_dir(state.cfg.trash_dir()) {
        for repo in rd.flatten() {
            for e in std::fs::read_dir(repo.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                if old(&e.path(), 3_600_000) && std::fs::remove_dir_all(e.path()).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    let cutoff = now() - 7 * 86_400_000;
    let stale: Vec<String> = state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT id FROM uploads WHERE created_at < ?1")?;
            let r = st.query_map([cutoff], |r| r.get(0))?;
            let v: rusqlite::Result<Vec<String>> = r.collect();
            let v = v?;
            c.execute("DELETE FROM uploads WHERE created_at < ?1", [cutoff])?;
            Ok(v)
        })
        .await?;
    for id in stale {
        let _ = std::fs::remove_file(state.cfg.uploads_dir().join(&id));
        removed += 1;
    }
    let discarded: Vec<(i64, String)> = state
        .db
        .call(|c| {
            let mut st = c.prepare("SELECT c.id, r.slug FROM changesets c JOIN repos r ON r.id=c.repo_id WHERE c.status IN ('discarded','published')")?;
            let r = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
        .await?;
    for (id, slug) in discarded {
        let d = staging_dir(state, &slug, id);
        if d.exists() && std::fs::remove_dir_all(&d).is_ok() {
            removed += 1;
        }
    }
    ctx.log(format!("removed {removed} item(s)"));
    match crate::stats::prune(state).await {
        Ok(n) if n > 0 => ctx.log(format!("pruned {n} old statistics row(s)")),
        Ok(_) => {}
        Err(e) => ctx.log(format!("statistics pruning failed: {e:#}")),
    }
    Ok(serde_json::json!({"removed": removed}))
}
