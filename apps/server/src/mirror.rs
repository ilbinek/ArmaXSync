//! Mirroring a remote (legacy or ArmaXSync) repository.
//!
//! The live folder is cloned into staging with hard links (cheap, and gives
//! the delta engine its seeds), the remote is synced into the clone, and the
//! differences become a changeset published as our own revision.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use axs_build::is_excluded_name;
use axs_model::paths::join_rel;
use axs_sync::{CancellationToken, EventFn, ExecOptions, PlanOptions, SyncEvent};
use axs_transport::{Endpoint, Transport};

use crate::db::now;
use crate::jobs::{JobCtx, JobKind};
use crate::publish::{publish_changeset, staging_dir, CsOp};
use crate::state::{RepoRuntime, Shared};

#[derive(Debug, Clone, Copy)]
struct Meta {
    size: u64,
    mtime: i128,
}

/// Content files (no metadata/sidecars) below `root`, by relative path.
fn list_content(root: &Path) -> (BTreeMap<String, Meta>, BTreeSet<String>) {
    let mut files = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    fn rec(dir: &Path, rel: &str, files: &mut BTreeMap<String, Meta>, dirs: &mut BTreeSet<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(md) = e.metadata() else { continue };
            let is_dir = md.is_dir();
            if is_excluded_name(&name, is_dir) {
                continue;
            }
            let r = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if is_dir {
                dirs.insert(r.clone());
                rec(&e.path(), &r, files, dirs);
            } else {
                let mtime = md
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i128)
                    .unwrap_or(0);
                files.insert(
                    r,
                    Meta {
                        size: md.len(),
                        mtime,
                    },
                );
            }
        }
    }
    rec(root, "", &mut files, &mut dirs);
    (files, dirs)
}

/// Hard-link every content file of `live` into `clone`. Returns false when
/// hard links are not possible (different filesystem / unsupported).
fn hardlink_clone(live: &Path, clone: &Path) -> std::io::Result<bool> {
    let (files, dirs) = list_content(live);
    std::fs::create_dir_all(clone)?;
    for d in &dirs {
        std::fs::create_dir_all(join_rel(clone, d))?;
    }
    for f in files.keys() {
        if let Err(e) = std::fs::hard_link(join_rel(live, f), join_rel(clone, f)) {
            tracing::warn!("hard link failed ({e}); falling back to full staging copy");
            return Ok(false);
        }
    }
    Ok(true)
}

/// Adapts the planner's absolute-path cache keys to the repository build
/// cache (relative keys). Hard-linked clones share size and mtime with live
/// files, so unchanged content is never rehashed.
struct CloneCache {
    prefix: String,
    inner: axs_build::FileHashCache,
}

impl CloneCache {
    fn rel(&self, key: &str) -> Option<String> {
        key.strip_prefix(&self.prefix)
            .map(|r| r.trim_start_matches(['/', '\\']).replace('\\', "/"))
    }
}

impl axs_build::HashCache for CloneCache {
    fn get(&self, key: &str) -> Option<axs_build::CachedHash> {
        self.rel(key).and_then(|r| self.inner.get(&r))
    }
    fn put(&mut self, key: &str, v: axs_build::CachedHash) {
        if let Some(r) = self.rel(key) {
            // Keep sidecar bookkeeping of an existing entry.
            let mut v = v;
            if let Some(old) = self.inner.get(&r) {
                if old.sha1 == v.sha1 {
                    v.zsync_sha1 = old.zsync_sha1;
                    v.zip_sha1 = old.zip_sha1;
                }
            }
            self.inner.put(&r, v);
        }
    }
    fn retain(&mut self, _keep: &dyn Fn(&str) -> bool) -> usize {
        0
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub async fn mirror_job(
    state: &Shared,
    rt: &Arc<RepoRuntime>,
    ctx: &JobCtx,
) -> anyhow::Result<serde_json::Value> {
    let rec = rt.record();
    let m = rec
        .settings
        .mirror
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no mirror source configured"))?;
    let password = m
        .password_sealed
        .as_deref()
        .and_then(|s| state.secrets.open(s));
    let mut url = m.url.trim().to_owned();
    let mut login = m.login.clone();
    let mut password = password;
    if url.to_lowercase().ends_with("/autoconfig") {
        let (ac, _) = axs_sync::fetch_autoconfig(&url).await?;
        let p = ac
            .protocole
            .ok_or_else(|| anyhow::anyhow!("remote autoconfig has no protocol"))?;
        url = axs_legacy::urls::base_url(&p);
        if login.is_none() {
            login = p.login.filter(|l| !l.eq_ignore_ascii_case("anonymous"));
            password = p.password;
        }
    }
    ctx.log(format!("mirroring {url}"));
    let ep = Endpoint::parse(&url, login.as_deref(), password.as_deref())?;
    let t = Transport::new(ep)?;
    let snap = axs_sync::fetch_snapshot(&t, true).await?;
    ctx.log(format!(
        "remote revision {} ({:?}, {} files)",
        snap.manifest.revision,
        snap.source,
        snap.manifest.files.len()
    ));

    // Changeset + staging clone.
    let repo_id = rec.id;
    let cs: i64 = state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO changesets(repo_id, status, note, created_at, updated_at) VALUES (?1,'draft','mirror',?2,?2)",
                rusqlite::params![repo_id, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    let stage = staging_dir(state, &rec.slug, cs);
    let clone = stage.join("files");
    let (live, c2) = (rt.root.clone(), clone.clone());
    let linked = tokio::task::spawn_blocking(move || hardlink_clone(&live, &c2)).await??;
    if !linked {
        // Copy fallback (slow, but keeps publishing atomic).
        let (live, c2) = (rt.root.clone(), clone.clone());
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let (files, dirs) = list_content(&live);
            for d in &dirs {
                std::fs::create_dir_all(join_rel(&c2, d))?;
            }
            for f in files.keys() {
                std::fs::copy(join_rel(&live, f), join_rel(&c2, f))?;
            }
            Ok(())
        })
        .await??;
    }

    // Sync remote → clone.
    let selection = (!m.include.is_empty()).then(|| m.include.clone());
    let popts = PlanOptions {
        exact_match: selection.is_none(),
        selection,
        keep_extras: vec![],
        case_insensitive: false,
    };
    let (man, c3, p2) = (snap.manifest.clone(), clone.clone(), popts.clone());
    let cache_path = state.cache_path(&rec.slug);
    let local = tokio::task::spawn_blocking(move || {
        let mut cache = CloneCache {
            prefix: c3.to_string_lossy().into_owned(),
            inner: axs_build::FileHashCache::load(cache_path),
        };
        let r = axs_sync::snapshot_local(&c3, &man, &p2, &mut cache);
        let _ = axs_build::HashCache::flush(&mut cache);
        r
    })
    .await??;
    let plan = axs_sync::plan(&snap.manifest, &local, &popts);
    ctx.log(format!(
        "{} file(s) to download ({} bytes), {} to delete",
        plan.fetch_count, plan.fetch_bytes, plan.delete_count
    ));
    let total = plan.fetch_bytes;
    let done = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (ctx2, done2) = (ctx.clone(), done.clone());
    let events: EventFn = Arc::new(move |e: SyncEvent| match e {
        SyncEvent::Bytes { n } => {
            let d = done2.fetch_add(n, std::sync::atomic::Ordering::Relaxed) + n;
            ctx2.progress("download", d.min(total), total);
        }
        SyncEvent::FileFailed { path, error } => ctx2.log(format!("failed: {path}: {error}")),
        _ => {}
    });
    let cancel = CancellationToken::new();
    let c4 = cancel.clone();
    let jc = ctx.cancel.clone();
    tokio::spawn(async move {
        jc.cancelled().await;
        c4.cancel();
    });
    let eo = ExecOptions {
        connections: snap.manifest.connections.max(1) as usize,
        ranges: snap.ranges_supported,
        compressed_only: snap.manifest.compressed_only,
        ..Default::default()
    };
    let report = axs_sync::execute(&plan, &t, &clone, &eo, events, cancel).await;
    if !report.ok() {
        let msg = report
            .failed
            .iter()
            .take(5)
            .map(|(p, e)| format!("{p}: {e}"))
            .collect::<Vec<_>>()
            .join("; ");
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE changesets SET status='discarded', updated_at=?1 WHERE id=?2",
                    rusqlite::params![now(), cs],
                )
            })
            .await?;
        anyhow::bail!(
            "mirror download failed ({} errors): {msg}",
            report.failed.len()
        );
    }

    // Diff clone vs live → changeset ops.
    let (live, c5) = (rt.root.clone(), clone.clone());
    let ops = tokio::task::spawn_blocking(move || {
        let (lf, ld) = list_content(&live);
        let (cf, cd) = list_content(&c5);
        let mut ops = vec![];
        for (p, m) in &lf {
            if !cf.contains_key(p) && !cd.iter().any(|d| d == p) {
                ops.push(CsOp::Delete { path: p.clone() });
            } else if let Some(c) = cf.get(p) {
                if c.size != m.size || c.mtime != m.mtime {
                    ops.push(CsOp::Put { path: p.clone(), size: c.size });
                }
            }
        }
        for (p, c) in &cf {
            if !lf.contains_key(p) {
                ops.push(CsOp::Put { path: p.clone(), size: c.size });
            }
        }
        for d in &ld {
            if !cd.contains(d) && !ops.iter().any(|o| matches!(o, CsOp::Delete { path } if axs_model::paths::is_under(d, path, false))) {
                ops.push(CsOp::Delete { path: d.clone() });
            }
        }
        for d in &cd {
            if !ld.contains(d) {
                ops.push(CsOp::Mkdir { path: d.clone() });
            }
        }
        ops
    })
    .await?;

    // Imported settings.
    let mut settings = rec.settings.clone();
    if m.import_events {
        settings.events = snap.manifest.events.clone();
    }
    if m.import_hidden {
        settings.hidden = snap.manifest.hidden_paths.clone();
    }
    if m.import_servers {
        settings.servers = snap.manifest.servers.clone();
    }
    let settings_changed = settings != rec.settings;
    if settings_changed {
        let js = serde_json::to_string(&settings)?;
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE repos SET settings=?1 WHERE id=?2",
                    rusqlite::params![js, repo_id],
                )
            })
            .await?;
        state.refresh_repo(&rec.slug).await?;
    }
    let first_publish = rt.published.load().revision == 0;
    if ops.is_empty() && !settings_changed && !first_publish {
        ctx.log("already up to date");
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE changesets SET status='discarded', updated_at=?1 WHERE id=?2",
                    rusqlite::params![now(), cs],
                )
            })
            .await?;
        let _ = std::fs::remove_dir_all(&stage);
        return Ok(
            serde_json::json!({"changed": false, "remote_revision": snap.manifest.revision}),
        );
    }
    let n = ops.len();
    let js = serde_json::to_string(&ops)?;
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET ops=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![js, now(), cs],
            )
        })
        .await?;
    ctx.log(format!("{n} change(s); publishing"));
    let mut out = publish_changeset(state, rt, cs, ctx).await?;
    out["remote_revision"] = serde_json::json!(snap.manifest.revision);
    out["changes"] = serde_json::json!(n);
    let _ = JobKind::Gc; // keep variant referenced for docs
    Ok(out)
}
