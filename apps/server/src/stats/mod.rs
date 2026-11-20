//! Download statistics and Prometheus metrics.

use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::api::ApiResult;
use crate::auth::Principal;
use crate::state::Shared;

pub mod api;
pub mod collect;
pub mod hll;
#[cfg(debug_assertions)]
pub mod seed;

use collect::{day_of, month_of, Acc, Counters, Salt, COLS};
use hll::Hll;

/// This month's player-hash salt (created on first use; older ones deleted).
pub fn load_salt(db: &crate::db::Db) -> anyhow::Result<Salt> {
    let month = month_of(crate::db::now());
    db.with(|c| salt_for(c, &month))
}

fn salt_for(c: &mut rusqlite::Connection, month: &str) -> rusqlite::Result<Salt> {
    use rand::RngCore;
    use rusqlite::OptionalExtension;
    let existing: Option<Vec<u8>> = c
        .query_row("SELECT salt FROM stats_salt WHERE month=?1", [month], |r| {
            r.get(0)
        })
        .optional()?;
    let key: [u8; 32] = match existing.and_then(|v| v.try_into().ok()) {
        Some(k) => k,
        None => {
            let mut k = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut k);
            c.execute(
                "INSERT OR REPLACE INTO stats_salt(month, salt) VALUES (?1, ?2)",
                rusqlite::params![month, k.to_vec()],
            )?;
            k
        }
    };
    // Old salts go away so hashes cannot be linked across months.
    c.execute("DELETE FROM stats_salt WHERE month <> ?1", [month])?;
    Ok(Salt {
        month: month.to_owned(),
        key,
    })
}

fn upsert_sql(table: &str, key: &str) -> String {
    let cols = COLS.join(", ");
    let marks: Vec<String> = (0..COLS.len()).map(|i| format!("?{}", i + 3)).collect();
    let set: Vec<String> = COLS
        .iter()
        .map(|c| format!("{c}={c}+excluded.{c}"))
        .collect();
    format!(
        "INSERT INTO {table}({key}, repo_id, {cols}) VALUES (?1, ?2, {}) ON CONFLICT({key}, repo_id) DO UPDATE SET {}",
        marks.join(", "),
        set.join(", ")
    )
}

fn counter_params(
    key: rusqlite::types::Value,
    repo: i64,
    v: &Counters,
) -> Vec<rusqlite::types::Value> {
    let mut p = vec![key, repo.into()];
    p.extend(
        v.0.iter()
            .map(|x| rusqlite::types::Value::Integer(*x as i64)),
    );
    p
}

fn write(c: &mut rusqlite::Connection, acc: &Acc) -> rusqlite::Result<()> {
    use rusqlite::OptionalExtension;
    let tx = c.transaction()?;
    {
        let hourly = upsert_sql("stats_hourly", "hour");
        let daily = upsert_sql("stats_daily", "day");
        let mut days: std::collections::HashMap<(String, i64), Counters> = Default::default();
        for ((hour, repo), v) in &acc.hours {
            if v.is_zero() {
                continue;
            }
            tx.execute(
                &hourly,
                rusqlite::params_from_iter(counter_params((*hour).into(), *repo, v)),
            )?;
            days.entry((day_of(hour * 3_600_000), *repo))
                .or_default()
                .add(v);
        }
        for ((day, repo), v) in &days {
            tx.execute(
                &daily,
                rusqlite::params_from_iter(counter_params(day.clone().into(), *repo, v)),
            )?;
        }
        for ((day, repo, addon), (b, d)) in &acc.addons {
            tx.execute(
                "INSERT INTO stats_addons(day, repo_id, addon, bytes, downloads) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(day, repo_id, addon) DO UPDATE SET bytes=bytes+?4, downloads=downloads+?5",
                rusqlite::params![day, repo, addon, *b as i64, *d as i64],
            )?;
        }
        for ((day, family, version), n) in &acc.versions {
            tx.execute(
                "INSERT INTO stats_versions(day, family, version, requests) VALUES (?1,?2,?3,?4)
                 ON CONFLICT(day, family, version) DO UPDATE SET requests=requests+?4",
                rusqlite::params![day, family, version, *n as i64],
            )?;
        }
        for ((day, repo), sketch) in &acc.players {
            let old: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT players FROM stats_daily WHERE day=?1 AND repo_id=?2",
                    rusqlite::params![day, repo],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let mut merged = old.as_deref().and_then(Hll::from_bytes).unwrap_or_default();
            merged.merge(sketch);
            tx.execute(
                "INSERT INTO stats_daily(day, repo_id, players, clients) VALUES (?1,?2,?3,?4)
                 ON CONFLICT(day, repo_id) DO UPDATE SET players=excluded.players, clients=excluded.clients",
                rusqlite::params![day, repo, merged.to_bytes(), merged.count() as i64],
            )?;
        }
    }
    tx.commit()
}

/// Move collected statistics into the database (every 60 s and at shutdown).
pub async fn flush(state: &Shared) -> anyhow::Result<()> {
    let st = state.stats.clone();
    // New month: new salt, the old one is deleted.
    let month = month_of(crate::db::now());
    if st.salt.load().month != month {
        let salt = state.db.call(move |c| salt_for(c, &month)).await?;
        st.salt.store(std::sync::Arc::new(salt));
    }
    let acc = st.take();
    if acc.hours.is_empty()
        && acc.addons.is_empty()
        && acc.versions.is_empty()
        && acc.players.is_empty()
    {
        return Ok(());
    }
    let acc = std::sync::Arc::new(acc);
    let a2 = acc.clone();
    let r = state.db.call(move |c| write(c, &a2)).await;
    if let Err(e) = r {
        // Keep the numbers for the next attempt.
        if let Ok(acc) = std::sync::Arc::try_unwrap(acc) {
            st.restore(acc);
        }
        return Err(e);
    }
    Ok(())
}

/// Drop statistics older than the configured retention.
pub async fn prune(state: &Shared) -> anyhow::Result<usize> {
    let cfg = state.cfg.stats.clone();
    let now = crate::db::now();
    let day_cut = day_of(now - cfg.retention_days.max(1) as i64 * 86_400_000);
    let hour_cut = collect::hour_of(now - cfg.hourly_retention_days.max(1) as i64 * 86_400_000);
    state
        .db
        .call(move |c| {
            let mut n = c.execute("DELETE FROM stats_hourly WHERE hour < ?1", [hour_cut])?;
            n += c.execute("DELETE FROM stats_daily WHERE day < ?1", [&day_cut])?;
            n += c.execute("DELETE FROM stats_addons WHERE day < ?1", [&day_cut])?;
            n += c.execute("DELETE FROM stats_versions WHERE day < ?1", [&day_cut])?;
            Ok(n)
        })
        .await
}

/// Remove a deleted repository's statistics.
pub async fn forget_repo(state: &Shared, repo_id: i64) -> anyhow::Result<()> {
    state
        .db
        .call(move |c| {
            for t in ["stats_hourly", "stats_daily", "stats_addons"] {
                c.execute(&format!("DELETE FROM {t} WHERE repo_id=?1"), [repo_id])?;
            }
            Ok(())
        })
        .await
}

/// Record a published revision (from a build, publish or mirror summary).
pub async fn note_revision(
    state: &Shared,
    repo_id: i64,
    job_id: i64,
    kind: &str,
    summary: &serde_json::Value,
) {
    let Some(rev) = summary.get("revision").and_then(|v| v.as_i64()) else {
        return;
    };
    let n = |k: &str| summary.get(k).and_then(|v| v.as_i64());
    let len = |k: &str| {
        summary
            .get(k)
            .and_then(|v| v.as_array())
            .map(|a| a.len() as i64)
    };
    let row = (
        rev,
        kind.to_owned(),
        n("files"),
        n("total_size"),
        len("new_addons"),
        len("updated_addons"),
        len("deleted_addons"),
        n("duration_ms"),
    );
    let r = state
        .db
        .call(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO revision_history(repo_id, revision, at, kind, files, total_size, new_addons, updated_addons, deleted_addons, duration_ms, job_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                rusqlite::params![repo_id, row.0, crate::db::now(), row.1, row.2, row.3, row.4, row.5, row.6, row.7, job_id],
            )
        })
        .await;
    if let Err(e) = r {
        tracing::warn!("revision history: {e:#}");
    }
}

#[derive(Serialize)]
pub struct DayStat {
    pub day: String,
    pub repo: String,
    pub bytes: i64,
    pub requests: i64,
    pub files: i64,
}

pub async fn stats(
    State(state): State<Shared>,
    p: Principal,
) -> ApiResult<Json<serde_json::Value>> {
    flush(&state).await?;
    let admin = p.is_admin();
    let maintained = p.maintained.clone();
    let rows: Vec<(DayStat, i64)> = state
        .db
        .call(|c| {
            let mut st = c.prepare(
                "SELECT s.day, r.slug, s.bytes, s.requests, s.files, r.id FROM stats_daily s JOIN repos r ON r.id=s.repo_id WHERE s.day >= date('now','-60 day') ORDER BY s.day",
            )?;
            let r = st.query_map([], |r| Ok((DayStat { day: r.get(0)?, repo: r.get(1)?, bytes: r.get(2)?, requests: r.get(3)?, files: r.get(4)? }, r.get(5)?)))?;
            r.collect()
        })
        .await?;
    let days: Vec<DayStat> = rows
        .into_iter()
        .filter(|(_, id)| admin || maintained.contains(id))
        .map(|(d, _)| d)
        .collect();
    let totals: Vec<serde_json::Value> = state
        .repo_list()
        .iter()
        .filter(|rt| admin || maintained.contains(&rt.record().id))
        .map(|rt| {
            serde_json::json!({
                "repo": rt.record().slug,
                "bytes": rt.bytes_served.load(Ordering::Relaxed),
                "requests": rt.requests.load(Ordering::Relaxed),
                "files": rt.files_served.load(Ordering::Relaxed),
            })
        })
        .collect();
    Ok(Json(
        serde_json::json!({"since_start": totals, "daily": days}),
    ))
}

/// Prometheus text exposition.
pub async fn metrics(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if !state.cfg.metrics {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(tok) = &state.cfg.metrics_token {
        let ok = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| crate::crypto::ct_eq(t.trim(), tok))
            .unwrap_or(false);
        if !ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    let mut out = String::new();
    let mut metric = |name: &str, help: &str, kind: &str, values: Vec<(String, f64)>| {
        out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} {kind}\n"));
        for (labels, v) in values {
            out.push_str(&format!("{name}{labels} {v}\n"));
        }
    };
    let repos = state.repo_list();
    let lbl = |s: &str| format!("{{repo=\"{}\"}}", s.replace('"', ""));
    metric(
        "armaxsync_bytes_served_total",
        "Bytes sent to clients",
        "counter",
        repos
            .iter()
            .map(|r| {
                (
                    lbl(&r.record().slug),
                    r.bytes_served.load(Ordering::Relaxed) as f64,
                )
            })
            .collect(),
    );
    metric(
        "armaxsync_requests_total",
        "Repository HTTP requests",
        "counter",
        repos
            .iter()
            .map(|r| {
                (
                    lbl(&r.record().slug),
                    r.requests.load(Ordering::Relaxed) as f64,
                )
            })
            .collect(),
    );
    metric(
        "armaxsync_files_served_total",
        "Files downloaded from the start",
        "counter",
        repos
            .iter()
            .map(|r| {
                (
                    lbl(&r.record().slug),
                    r.files_served.load(Ordering::Relaxed) as f64,
                )
            })
            .collect(),
    );
    metric(
        "armaxsync_repository_revision",
        "Published revision",
        "gauge",
        repos
            .iter()
            .map(|r| (lbl(&r.record().slug), r.published.load().revision as f64))
            .collect(),
    );
    metric(
        "armaxsync_repository_size_bytes",
        "Published content size",
        "gauge",
        repos
            .iter()
            .map(|r| (lbl(&r.record().slug), r.published.load().total_size as f64))
            .collect(),
    );
    metric(
        "armaxsync_repository_files",
        "Published file count",
        "gauge",
        repos
            .iter()
            .map(|r| (lbl(&r.record().slug), r.published.load().file_count as f64))
            .collect(),
    );
    let running: i64 = state
        .db
        .call(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM jobs WHERE status='running'",
                [],
                |r| r.get(0),
            )
        })
        .await
        .unwrap_or(0);
    metric(
        "armaxsync_jobs_running",
        "Running jobs",
        "gauge",
        vec![(String::new(), running as f64)],
    );
    let st = &state.stats;
    metric(
        "armaxsync_active_downloads",
        "Content transfers in progress",
        "gauge",
        vec![(
            String::new(),
            st.active_downloads.load(Ordering::Relaxed).max(0) as f64,
        )],
    );
    metric(
        "armaxsync_downloads_aborted_total",
        "Content transfers the client dropped",
        "counter",
        vec![(
            String::new(),
            st.aborted_total.load(Ordering::Relaxed) as f64,
        )],
    );
    metric(
        "armaxsync_client_requests_total",
        "Repository requests by client family",
        "counter",
        ["armaxsync", "arma3sync", "browser", "other"]
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    format!("{{client=\"{f}\"}}"),
                    st.family_requests[i].load(Ordering::Relaxed) as f64,
                )
            })
            .collect(),
    );
    if state.cfg.stats.players {
        metric(
            "armaxsync_players_active",
            "Players seen in the last 5 minutes",
            "gauge",
            vec![(String::new(), st.active_players() as f64)],
        );
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], out).into_response()
}
