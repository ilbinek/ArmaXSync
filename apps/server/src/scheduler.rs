//! Cron schedules (stored in SQLite) and periodic housekeeping.

use std::time::Duration;

use croner::Cron;

use crate::db::now;
use crate::jobs::{JobKind, JobManager};
use crate::state::Shared;

/// Next run time (ms) after `after_ms` for a cron expression (5 or 6 fields).
pub fn next_run(expr: &str, after_ms: i64) -> Option<i64> {
    let cron = Cron::new(expr.trim())
        .with_seconds_optional()
        .parse()
        .ok()?;
    let after = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(after_ms)?
        .with_timezone(&chrono::Local);
    cron.find_next_occurrence(&after, false)
        .ok()
        .map(|t| t.timestamp_millis())
}

pub fn spawn(state: Shared) {
    // Schedules.
    let s = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&s).await {
                tracing::warn!("scheduler: {e:#}");
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    });
    // Housekeeping every 30 minutes.
    let s = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1800)).await;
            let _ = JobManager::submit(&s, JobKind::Gc, None, None).await;
        }
    });
    // Live throughput samples (memory only).
    let s = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            s.stats.sample(
                s.stats
                    .live_bytes
                    .load(std::sync::atomic::Ordering::Relaxed),
            );
        }
    });
    // Stats flush.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = crate::stats::flush(&state).await {
                tracing::warn!("stats flush: {e:#}");
            }
        }
    });
}

async fn tick(state: &Shared) -> anyhow::Result<()> {
    let t = now();
    let due: Vec<(i64, i64, String, String)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT id, repo_id, kind, cron FROM schedules WHERE enabled=1 AND next_run IS NOT NULL AND next_run <= ?1")?;
            let r = st.query_map([t], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            r.collect()
        })
        .await?;
    for (id, repo_id, kind, cron) in due {
        // Skip if a job for this repository is already queued or running.
        let busy: i64 = state
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT COUNT(*) FROM jobs WHERE repo_id=?1 AND status IN ('queued','running')",
                    [repo_id],
                    |r| r.get(0),
                )
            })
            .await?;
        let next = next_run(&cron, t);
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE schedules SET last_run=?1, next_run=?2 WHERE id=?3",
                    rusqlite::params![t, next, id],
                )
            })
            .await?;
        if busy > 0 {
            continue;
        }
        let kind = if kind == "mirror" {
            JobKind::Mirror
        } else {
            JobKind::Build
        };
        tracing::info!(
            "schedule {id}: starting {} for repository {repo_id}",
            kind.name()
        );
        JobManager::submit(state, kind, Some(repo_id), None).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn cron() {
        let t = super::next_run("*/30 * * * *", 0).unwrap();
        assert!(t > 0 && t <= 30 * 60_000);
        assert!(super::next_run("not cron", 0).is_none());
    }
}
