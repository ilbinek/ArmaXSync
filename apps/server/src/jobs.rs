//! Background jobs (build, publish, mirror, gc) with per-repository
//! exclusivity, a global worker limit, live progress and persisted logs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::db::now;
use crate::state::Shared;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Rebuild metadata from the repository folder.
    Build,
    /// Apply a changeset (uploads, file manager, push) and publish.
    Publish { changeset: i64 },
    /// Pull from the configured remote repository and publish.
    Mirror,
    /// Housekeeping (trash, stale uploads/staging).
    Gc,
    /// Download, verify and stage a server update.
    SelfUpdate { version: String },
}

impl JobKind {
    pub fn name(&self) -> &'static str {
        match self {
            JobKind::Build => "build",
            JobKind::Publish { .. } => "publish",
            JobKind::Mirror => "mirror",
            JobKind::Gc => "gc",
            JobKind::SelfUpdate { .. } => "self_update",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Log {
        line: String,
    },
    Progress {
        phase: String,
        done: u64,
        total: u64,
    },
    Status {
        status: String,
    },
}

/// Context handed to a running job.
#[derive(Clone)]
pub struct JobCtx {
    pub id: i64,
    pub cancel: CancellationToken,
    tx: broadcast::Sender<JobEvent>,
    log: Arc<Mutex<String>>,
    last_progress: Arc<Mutex<i64>>,
}

impl JobCtx {
    pub fn log(&self, line: impl Into<String>) {
        let line = line.into();
        tracing::info!(job = self.id, "{line}");
        {
            let mut l = self.log.lock().unwrap();
            if l.len() < 1 << 20 {
                l.push_str(&line);
                l.push('\n');
            }
        }
        let _ = self.tx.send(JobEvent::Log { line });
    }

    /// Throttled progress (at most ~5/s, always the final value).
    pub fn progress(&self, phase: &str, done: u64, total: u64) {
        let t = now();
        {
            let mut lp = self.last_progress.lock().unwrap();
            if done < total && t - *lp < 200 {
                return;
            }
            *lp = t;
        }
        let _ = self.tx.send(JobEvent::Progress {
            phase: phase.to_owned(),
            done,
            total,
        });
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

#[derive(Default)]
struct Inner {
    repo_locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    running: Mutex<HashMap<i64, JobCtx>>,
}

#[derive(Clone)]
pub struct JobManager {
    inner: Arc<Inner>,
    sem: Arc<Semaphore>,
}

impl JobManager {
    pub fn new(workers: usize) -> Self {
        Self {
            inner: Arc::new(Inner::default()),
            sem: Arc::new(Semaphore::new(workers.max(1))),
        }
    }

    pub fn repo_lock(&self, repo_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.inner
            .repo_locks
            .lock()
            .unwrap()
            .entry(repo_id)
            .or_default()
            .clone()
    }

    /// Subscribe to a running job's events (None if not running).
    pub fn subscribe(&self, id: i64) -> Option<(broadcast::Receiver<JobEvent>, String)> {
        let r = self.inner.running.lock().unwrap();
        r.get(&id)
            .map(|c| (c.tx.subscribe(), c.log.lock().unwrap().clone()))
    }

    pub fn cancel(&self, id: i64) -> bool {
        match self.inner.running.lock().unwrap().get(&id) {
            Some(c) => {
                c.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Mark jobs left over from a previous process as interrupted.
    pub fn recover(state: &Shared) -> anyhow::Result<()> {
        state.db.with(|c| {
            c.execute(
                "UPDATE jobs SET status='interrupted', finished_at=?1 WHERE status IN ('queued','running')",
                [now()],
            )
        })?;
        Ok(())
    }

    /// Queue a job; returns its id immediately.
    pub async fn submit(
        state: &Shared,
        kind: JobKind,
        repo_id: Option<i64>,
        user_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        let params = serde_json::to_string(&kind)?;
        let k = kind.name().to_owned();
        let id = state
            .db
            .call(move |c| {
                c.execute(
                    "INSERT INTO jobs(repo_id, kind, status, created_at, user_id, params) VALUES (?1,?2,'queued',?3,?4,?5)",
                    rusqlite::params![repo_id, k, now(), user_id, params],
                )?;
                Ok(c.last_insert_rowid())
            })
            .await?;
        let (tx, _) = broadcast::channel(256);
        let ctx = JobCtx {
            id,
            cancel: CancellationToken::new(),
            tx,
            log: Arc::new(Mutex::new(String::new())),
            last_progress: Arc::new(Mutex::new(0)),
        };
        let jm = state.jobs.clone();
        jm.inner.running.lock().unwrap().insert(id, ctx.clone());
        let state = state.clone();
        tokio::spawn(async move {
            // Wait for the repository lock and a worker slot, but give up as
            // soon as the job is cancelled while still queued.
            let repo_lock = repo_id.map(|r| jm.repo_lock(r));
            let acquire = async {
                let guard = match repo_lock {
                    Some(l) => Some(l.lock_owned().await),
                    None => None,
                };
                let permit = jm.sem.clone().acquire_owned().await;
                (guard, permit)
            };
            let (_repo_guard, _permit) = tokio::select! {
                held = acquire => held,
                _ = ctx.cancel.cancelled() => {
                    ctx.log("cancelled while queued");
                    let log = ctx.log.lock().unwrap().clone();
                    let _ = state
                        .db
                        .call(move |c| {
                            c.execute(
                                "UPDATE jobs SET status='cancelled', finished_at=?1, error='cancelled', log=?2 WHERE id=?3",
                                rusqlite::params![now(), log, id],
                            )
                        })
                        .await;
                    let _ = ctx.tx.send(JobEvent::Status { status: "cancelled".into() });
                    jm.inner.running.lock().unwrap().remove(&id);
                    return;
                }
            };
            let _ = state
                .db
                .call(move |c| {
                    c.execute(
                        "UPDATE jobs SET status='running', started_at=?1 WHERE id=?2",
                        rusqlite::params![now(), id],
                    )
                })
                .await;
            let _ = ctx.tx.send(JobEvent::Status {
                status: "running".into(),
            });
            let result = if ctx.is_cancelled() {
                Err(anyhow::anyhow!("cancelled"))
            } else {
                run(&state, &kind, repo_id, &ctx).await
            };
            let (status, summary, error) = match result {
                Ok(summary) => ("succeeded", Some(summary), None),
                Err(_) if ctx.is_cancelled() => ("cancelled", None, Some("cancelled".to_owned())),
                Err(e) => {
                    ctx.log(format!("error: {e:#}"));
                    ("failed", None, Some(format!("{e:#}")))
                }
            };
            if let (Some(s), Some(repo)) = (&summary, repo_id) {
                if matches!(
                    kind,
                    JobKind::Build | JobKind::Publish { .. } | JobKind::Mirror
                ) {
                    crate::stats::note_revision(&state, repo, id, kind.name(), s).await;
                }
            }
            let log = ctx.log.lock().unwrap().clone();
            let summary_s = summary.as_ref().map(|s| s.to_string());
            let _ = state
                .db
                .call(move |c| {
                    c.execute(
                        "UPDATE jobs SET status=?1, finished_at=?2, summary=?3, error=?4, log=?5 WHERE id=?6",
                        rusqlite::params![status, now(), summary_s, error, log, id],
                    )
                })
                .await;
            let _ = ctx.tx.send(JobEvent::Status {
                status: status.into(),
            });
            jm.inner.running.lock().unwrap().remove(&id);
        });
        Ok(id)
    }
}

async fn run(
    state: &Shared,
    kind: &JobKind,
    repo_id: Option<i64>,
    ctx: &JobCtx,
) -> anyhow::Result<serde_json::Value> {
    let rt = match repo_id {
        Some(id) => Some(
            state
                .repo_list()
                .into_iter()
                .find(|r| r.record().id == id)
                .ok_or_else(|| anyhow::anyhow!("repository no longer exists"))?,
        ),
        None => None,
    };
    match kind {
        JobKind::Build => crate::publish::build_job(state, &rt.unwrap(), ctx).await,
        JobKind::Publish { changeset } => {
            crate::publish::publish_changeset(state, &rt.unwrap(), *changeset, ctx).await
        }
        JobKind::Mirror => crate::mirror::mirror_job(state, &rt.unwrap(), ctx).await,
        JobKind::Gc => crate::publish::gc_job(state, ctx).await,
        JobKind::SelfUpdate { version } => crate::updates::install_job(state, version, ctx).await,
    }
}
