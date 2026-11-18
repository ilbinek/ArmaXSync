//! Download statistics and Prometheus metrics.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::api::ApiResult;
use crate::auth::Principal;
use crate::state::Shared;

/// Counter values already written to `stats_daily`, per repository.
static FLUSHED: std::sync::LazyLock<Mutex<HashMap<i64, (u64, u64, u64)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Add counter deltas since the last flush to today's row.
pub async fn flush(state: &Shared) -> anyhow::Result<()> {
    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut rows = vec![];
    {
        let mut f = FLUSHED.lock().unwrap();
        for rt in state.repo_list() {
            let id = rt.record().id;
            let cur = (
                rt.bytes_served.load(Ordering::Relaxed),
                rt.requests.load(Ordering::Relaxed),
                rt.files_served.load(Ordering::Relaxed),
            );
            let prev = f.get(&id).copied().unwrap_or((0, 0, 0));
            let d = (
                cur.0.saturating_sub(prev.0),
                cur.1.saturating_sub(prev.1),
                cur.2.saturating_sub(prev.2),
            );
            f.insert(id, cur);
            if d != (0, 0, 0) {
                rows.push((id, d));
            }
        }
    }
    if rows.is_empty() {
        return Ok(());
    }
    state
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            for (id, (b, r, f)) in rows {
                tx.execute(
                    "INSERT INTO stats_daily(day, repo_id, bytes, requests, files) VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(day, repo_id) DO UPDATE SET bytes=bytes+?3, requests=requests+?4, files=files+?5",
                    rusqlite::params![day, id, b as i64, r as i64, f as i64],
                )?;
            }
            tx.commit()
        })
        .await
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
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], out).into_response()
}
