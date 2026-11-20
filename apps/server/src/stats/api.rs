//! Statistics queries for the admin UI (`/api/v1/stats/*`).
//!
//! Admins see every repository; other users the ones they maintain. All
//! times are UTC milliseconds; the UI converts to the viewer's time zone
//! (the heatmap takes the offset as `tz`).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::collect::{col, day_of, hour_of, Counters, COLS, N};
use super::hll::Hll;
use crate::api::{ApiError, ApiResult};
use crate::auth::Principal;
use crate::db::now;
use crate::state::Shared;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/stats/summary", get(summary))
        .route("/stats/traffic", get(traffic))
        .route("/stats/players", get(players))
        .route("/stats/clients", get(clients))
        .route("/stats/addons", get(addons))
        .route("/stats/heatmap", get(heatmap))
        .route("/stats/repos", get(repos))
        .route("/stats/live", get(live))
}

const DAY: i64 = 86_400_000;

#[derive(Deserialize, Default)]
pub struct Q {
    from: Option<i64>,
    to: Option<i64>,
    repo: Option<String>,
    bucket: Option<String>,
    /// Viewer's offset from UTC in minutes (east positive), for the heatmap.
    tz: Option<i32>,
    limit: Option<usize>,
}

/// Repositories the caller may see, filtered by `?repo=`.
struct Scope {
    ids: Vec<i64>,
    slugs: HashMap<i64, String>,
    /// Whole server (admin, no repository filter): use the server-wide
    /// player sketches (repo 0) so multi-repository players count once.
    server: bool,
    from: i64,
    to: i64,
}

impl Scope {
    fn in_list(&self) -> String {
        if self.ids.is_empty() {
            "-1".into()
        } else {
            self.ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        }
    }
    fn days(&self) -> (String, String) {
        (day_of(self.from), day_of(self.to - 1))
    }
}

fn scope(state: &Shared, p: &Principal, q: &Q) -> ApiResult<Scope> {
    let to = q.to.unwrap_or_else(now).min(now() + DAY);
    let from = q.from.unwrap_or(to - 30 * DAY).min(to - 1);
    let mut slugs = HashMap::new();
    for rt in state.repo_list() {
        let r = rt.record();
        if p.is_admin() || p.can_manage(r.id, &r.slug) {
            slugs.insert(r.id, r.slug.clone());
        }
    }
    let ids: Vec<i64> = match q.repo.as_deref().filter(|s| !s.is_empty()) {
        Some(slug) => {
            let id = slugs
                .iter()
                .find(|(_, s)| s.as_str() == slug)
                .map(|(i, _)| *i)
                .ok_or_else(ApiError::not_found)?;
            vec![id]
        }
        None => slugs.keys().copied().collect(),
    };
    Ok(Scope {
        server: p.is_admin() && q.repo.as_deref().unwrap_or("").is_empty(),
        ids,
        slugs,
        from,
        to,
    })
}

fn counters_from_row(r: &rusqlite::Row, offset: usize) -> rusqlite::Result<Counters> {
    let mut c = Counters::default();
    for i in 0..N {
        c.0[i] = r.get::<_, Option<i64>>(offset + i)?.unwrap_or(0).max(0) as u64;
    }
    Ok(c)
}

fn sum_cols() -> String {
    COLS.iter()
        .map(|c| format!("SUM({c})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn counters_json(c: &Counters) -> Value {
    let mut m = serde_json::Map::new();
    for (i, name) in COLS.iter().enumerate() {
        m.insert((*name).into(), c.0[i].into());
    }
    let served = c.0[col::bytes] + c.0[col::bytes_meta] + c.0[col::bytes_sidecar];
    m.insert("served".into(), served.into());
    let done = c.0[col::completed] + c.0[col::aborted];
    let speed = if c.0[col::xfer_ms] > 0 {
        (c.0[col::bytes] + c.0[col::bytes_sidecar]) as f64 / (c.0[col::xfer_ms] as f64 / 1000.0)
    } else {
        0.0
    };
    m.insert("avg_speed".into(), json!(speed));
    m.insert(
        "aborted_rate".into(),
        json!(if done > 0 {
            c.0[col::aborted] as f64 / done as f64
        } else {
            0.0
        }),
    );
    Value::Object(m)
}

/// Totals for [from, to): hourly rows inside hourly retention, daily otherwise.
async fn totals(state: &Shared, s: &Scope, from: i64, to: i64) -> ApiResult<Counters> {
    let ids = s.in_list();
    let hourly_from = now() - state.cfg.stats.hourly_retention_days as i64 * DAY;
    let (sql, a, b): (String, Value, Value) = if from >= hourly_from {
        (
            format!(
                "SELECT {} FROM stats_hourly WHERE repo_id IN ({ids}) AND hour >= ?1 AND hour < ?2",
                sum_cols()
            ),
            json!(hour_of(from)),
            json!(hour_of(to - 1) + 1),
        )
    } else {
        (
            format!(
                "SELECT {} FROM stats_daily WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2",
                sum_cols()
            ),
            json!(day_of(from)),
            json!(day_of(to - 1)),
        )
    };
    let c = state
        .db
        .call(move |c| {
            let to_sql = |v: &Value| -> rusqlite::types::Value {
                match v {
                    Value::Number(n) => n.as_i64().unwrap_or(0).into(),
                    Value::String(s) => s.clone().into(),
                    _ => rusqlite::types::Value::Null,
                }
            };
            c.query_row(&sql, [to_sql(&a), to_sql(&b)], |r| counters_from_row(r, 0))
        })
        .await?;
    Ok(c)
}

/// Daily player sketches for the scope: day → merged sketch.
async fn sketches(
    state: &Shared,
    s: &Scope,
    from_day: String,
    to_day: String,
) -> ApiResult<BTreeMap<String, Hll>> {
    let ids = if s.server {
        "0".to_owned()
    } else {
        s.in_list()
    };
    let rows: Vec<(String, Vec<u8>)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT day, players FROM stats_daily WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2 AND players IS NOT NULL"
            ))?;
            let r = st.query_map([from_day, to_day], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
        .await?;
    let mut out: BTreeMap<String, Hll> = BTreeMap::new();
    for (day, b) in rows {
        if let Some(h) = Hll::from_bytes(&b) {
            out.entry(day).or_default().merge(&h);
        }
    }
    Ok(out)
}

fn union<'a>(it: impl Iterator<Item = &'a Hll>) -> Hll {
    let mut u = Hll::default();
    for h in it {
        u.merge(h);
    }
    u
}

fn day_list(from: i64, to: i64) -> Vec<String> {
    let mut d = Vec::new();
    let mut t = from - from.rem_euclid(DAY);
    while t < to {
        d.push(day_of(t));
        t += DAY;
    }
    d
}

async fn fresh(state: &Shared) {
    if let Err(e) = super::flush(state).await {
        tracing::warn!("stats flush: {e:#}");
    }
}

// ------------------------------------------------------------- summary ----

async fn summary(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    let len = s.to - s.from;
    let cur = totals(&state, &s, s.from, s.to).await?;
    let prev = totals(&state, &s, s.from - len, s.from).await?;
    let (fd, td) = s.days();
    let sk = sketches(&state, &s, day_of(s.from - len), td.clone()).await?;
    let players_cur = union(sk.range(fd.clone()..=td.clone()).map(|(_, h)| h)).count();
    let players_prev = union(sk.range(day_of(s.from - len)..fd.clone()).map(|(_, h)| h)).count();
    // Daily series for sparklines.
    let ids = s.in_list();
    let (f2, t2) = (fd.clone(), td.clone());
    let rows: Vec<(String, i64, i64, i64)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT day, SUM(bytes + bytes_meta + bytes_sidecar), SUM(files), SUM(checks) FROM stats_daily
                 WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2 GROUP BY day"
            ))?;
            let r = st.query_map([f2, t2], |r| {
                Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0), r.get::<_, Option<i64>>(2)?.unwrap_or(0), r.get::<_, Option<i64>>(3)?.unwrap_or(0)))
            })?;
            r.collect()
        })
        .await?;
    let by_day: HashMap<String, (i64, i64, i64)> = rows
        .into_iter()
        .map(|(d, a, b, c)| (d, (a, b, c)))
        .collect();
    let daily: Vec<Value> = day_list(s.from, s.to)
        .into_iter()
        .map(|d| {
            let (served, downloads, checks) = by_day.get(&d).copied().unwrap_or_default();
            json!({"day": d, "served": served, "downloads": downloads, "checks": checks, "players": sk.get(&d).map(|h| h.count()).unwrap_or(0)})
        })
        .collect();
    let mut c = counters_json(&cur);
    c["players"] = players_cur.into();
    let mut pv = counters_json(&prev);
    pv["players"] = players_prev.into();
    Ok(Json(json!({
        "from": s.from, "to": s.to,
        "players_enabled": state.cfg.stats.players,
        "current": c, "previous": pv, "daily": daily,
    })))
}

// ------------------------------------------------------------- traffic ----

async fn traffic(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    let hourly_from = now() - state.cfg.stats.hourly_retention_days as i64 * DAY;
    let hourly = match q.bucket.as_deref() {
        Some("hour") => true,
        Some("day") => false,
        _ => s.to - s.from <= 3 * DAY,
    } && s.from >= hourly_from - DAY;
    let ids = s.in_list();
    let (from, to) = (s.from, s.to);
    let rows: Vec<(i64, i64, Counters)> = state
        .db
        .call(move |c| {
            let cols = COLS.join(", ");
            let (sql, a, b): (String, rusqlite::types::Value, rusqlite::types::Value) = if hourly {
                (
                    format!("SELECT hour * 3600000, repo_id, {cols} FROM stats_hourly WHERE repo_id IN ({ids}) AND hour >= ?1 AND hour < ?2 ORDER BY hour"),
                    hour_of(from).into(),
                    (hour_of(to - 1) + 1).into(),
                )
            } else {
                (
                    format!("SELECT CAST(strftime('%s', day) AS INTEGER) * 1000, repo_id, {cols} FROM stats_daily WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2 ORDER BY day"),
                    day_of(from).into(),
                    day_of(to - 1).into(),
                )
            };
            let mut st = c.prepare(&sql)?;
            let r = st.query_map([a, b], |r| Ok((r.get(0)?, r.get(1)?, counters_from_row(r, 2)?)))?;
            r.collect()
        })
        .await?;
    let step = if hourly { 3_600_000 } else { DAY };
    let mut series: BTreeMap<i64, Counters> = BTreeMap::new();
    let mut by_repo: BTreeMap<(i64, i64), u64> = BTreeMap::new();
    // Every bucket in range, so charts show gaps as zero.
    let mut t = s.from - s.from.rem_euclid(step);
    while t < s.to {
        series.insert(t, Counters::default());
        t += step;
    }
    for (t, repo, c) in &rows {
        series.entry(*t).or_default().add(c);
        *by_repo.entry((*t, *repo)).or_default() +=
            c.0[col::bytes] + c.0[col::bytes_sidecar] + c.0[col::bytes_meta];
    }
    let total = series.values().fold(Counters::default(), |mut a, c| {
        a.add(c);
        a
    });
    Ok(Json(json!({
        "bucket": if hourly { "hour" } else { "day" },
        "series": series.iter().map(|(t, c)| { let mut v = counters_json(c); v["t"] = (*t).into(); v }).collect::<Vec<_>>(),
        "by_repo": by_repo.iter().map(|((t, r), b)| json!({"t": t, "repo": s.slugs.get(r), "served": b})).collect::<Vec<_>>(),
        "total": counters_json(&total),
    })))
}

// ------------------------------------------------------------- players ----

async fn players(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    if !state.cfg.stats.players {
        return Ok(Json(json!({"enabled": false})));
    }
    let (fd, td) = s.days();
    // 30 days before the range for rolling weekly/monthly windows.
    let sk = sketches(&state, &s, day_of(s.from - 30 * DAY), td.clone()).await?;
    let days = day_list(s.from, s.to);
    let mut out = Vec::with_capacity(days.len());
    let mut mtd = Hll::default();
    let mut month = String::new();
    // Month-to-date union before the range starts (new vs returning).
    if let Some(first) = days.first() {
        month = first[..7].to_owned();
        for h in sk
            .range(format!("{month}-01")..first.clone())
            .map(|(_, h)| h)
        {
            mtd.merge(h);
        }
    }
    for (i, d) in days.iter().enumerate() {
        if d[..7] != month {
            month = d[..7].to_owned();
            mtd = Hll::default();
        }
        let today = sk.get(d).cloned().unwrap_or_default();
        let before = mtd.count();
        mtd.merge(&today);
        let new = mtd.count().saturating_sub(before);
        let dau = today.count();
        let t = s.from - s.from.rem_euclid(DAY) + i as i64 * DAY;
        let wau = union(sk.range(day_of(t - 6 * DAY)..=d.clone()).map(|(_, h)| h)).count();
        let mau = union(sk.range(day_of(t - 29 * DAY)..=d.clone()).map(|(_, h)| h)).count();
        out.push(json!({"day": d, "players": dau, "new": new.min(dau), "returning": dau.saturating_sub(new), "weekly": wau, "monthly": mau}));
    }
    // Per repository, for the whole range.
    let ids = s.in_list();
    let (f2, t2) = (fd.clone(), td.clone());
    let rows: Vec<(i64, Vec<u8>)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT repo_id, players FROM stats_daily WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2 AND players IS NOT NULL"
            ))?;
            let r = st.query_map([f2, t2], |r| Ok((r.get(0)?, r.get(1)?)))?;
            r.collect()
        })
        .await?;
    let mut per: HashMap<i64, Hll> = HashMap::new();
    for (r, b) in rows {
        if let Some(h) = Hll::from_bytes(&b) {
            per.entry(r).or_default().merge(&h);
        }
    }
    let mut per_repo: Vec<Value> = per
        .iter()
        .map(|(r, h)| json!({"repo": s.slugs.get(r), "players": h.count()}))
        .collect();
    per_repo.sort_by_key(|v| std::cmp::Reverse(v["players"].as_u64().unwrap_or(0)));
    let total = union(sk.range(fd..=td).map(|(_, h)| h)).count();
    Ok(Json(
        json!({"enabled": true, "total": total, "daily": out, "per_repo": per_repo}),
    ))
}

// ------------------------------------------------------------- clients ----

async fn clients(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    let (fd, td) = s.days();
    let ids = s.in_list();
    let (f2, t2) = (fd.clone(), td.clone());
    let daily: Vec<Value> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT day, SUM(c_axs), SUM(c_a3s), SUM(c_browser), SUM(c_other) FROM stats_daily
                 WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2 GROUP BY day ORDER BY day"
            ))?;
            let r = st.query_map([f2, t2], |r| {
                Ok(json!({"day": r.get::<_, String>(0)?, "armaxsync": r.get::<_, Option<i64>>(1)?, "arma3sync": r.get::<_, Option<i64>>(2)?, "browser": r.get::<_, Option<i64>>(3)?, "other": r.get::<_, Option<i64>>(4)?}))
            })?;
            r.collect()
        })
        .await?;
    // Client versions are server-wide (not per repository).
    let versions: Vec<Value> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(
                "SELECT family, version, SUM(requests), MAX(day) FROM stats_versions WHERE day >= ?1 AND day <= ?2
                 GROUP BY family, version ORDER BY SUM(requests) DESC LIMIT 50",
            )?;
            let r = st.query_map([fd, td], |r| {
                Ok(json!({"family": r.get::<_, String>(0)?, "version": r.get::<_, String>(1)?, "requests": r.get::<_, i64>(2)?, "last_seen": r.get::<_, String>(3)?}))
            })?;
            r.collect()
        })
        .await?;
    Ok(Json(
        json!({"daily": daily, "versions": versions, "current": crate::updates::VERSION}),
    ))
}

// -------------------------------------------------------------- addons ----

async fn addons(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    let (fd, td) = s.days();
    let ids = s.in_list();
    let limit = q.limit.unwrap_or(50).clamp(1, 500) as i64;
    let (f2, t2) = (fd.clone(), td.clone());
    let top: Vec<(i64, String, i64, i64)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT repo_id, addon, SUM(bytes), SUM(downloads) FROM stats_addons
                 WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2
                 GROUP BY repo_id, addon ORDER BY SUM(bytes) DESC LIMIT ?3"
            ))?;
            let r = st.query_map(rusqlite::params![f2, t2, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
            r.collect()
        })
        .await?;
    // Daily trend for the top 10.
    let ids = s.in_list();
    let (f3, t3) = (fd.clone(), td.clone());
    let trend_rows: Vec<(String, i64, String, i64)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT day, repo_id, addon, bytes FROM stats_addons WHERE repo_id IN ({ids}) AND day >= ?1 AND day <= ?2"
            ))?;
            let r = st.query_map([f3, t3], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            r.collect()
        })
        .await?;
    let days = day_list(s.from, s.to);
    let idx: HashMap<&String, usize> = days.iter().enumerate().map(|(i, d)| (d, i)).collect();
    let mut trends: HashMap<(i64, String), Vec<i64>> = HashMap::new();
    for (repo, a, ..) in top.iter().take(10) {
        trends.insert((*repo, a.clone()), vec![0; days.len()]);
    }
    for (d, repo, a, b) in trend_rows {
        if let (Some(t), Some(i)) = (trends.get_mut(&(repo, a)), idx.get(&d)) {
            t[*i] += b;
        }
    }
    Ok(Json(json!({
        "days": days,
        "addons": top.iter().map(|(r, a, b, d)| json!({
            "repo": s.slugs.get(r), "addon": a, "bytes": b, "downloads": d,
            "trend": trends.get(&(*r, a.clone())),
        })).collect::<Vec<_>>(),
    })))
}

// ------------------------------------------------------------- heatmap ----

async fn heatmap(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    fresh(&state).await;
    let s = scope(&state, &p, &q)?;
    let tz = q.tz.unwrap_or(0).clamp(-14 * 60, 14 * 60) as i64;
    let ids = s.in_list();
    let (from, to) = (hour_of(s.from), hour_of(s.to - 1) + 1);
    let rows: Vec<(i64, i64, i64, i64)> = state
        .db
        .call(move |c| {
            let mut st = c.prepare(&format!(
                "SELECT hour, SUM(bytes + bytes_meta + bytes_sidecar), SUM(requests), SUM(files) FROM stats_hourly
                 WHERE repo_id IN ({ids}) AND hour >= ?1 AND hour < ?2 GROUP BY hour"
            ))?;
            let r = st.query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            r.collect()
        })
        .await?;
    // 7 × 24 grid, Monday first, in the viewer's time zone.
    let mut grid = vec![[0i64; 3]; 7 * 24];
    for (hour, bytes, req, files) in rows {
        let local = hour * 3600 + tz * 60;
        let days_since_epoch = local.div_euclid(86_400);
        let weekday = (days_since_epoch + 3).rem_euclid(7); // 1970-01-01 was a Thursday
        let h = local.rem_euclid(86_400) / 3600;
        let cell = &mut grid[(weekday * 24 + h) as usize];
        cell[0] += bytes;
        cell[1] += req;
        cell[2] += files;
    }
    let hourly_limit = now() - state.cfg.stats.hourly_retention_days as i64 * DAY;
    Ok(Json(json!({
        "cells": grid.iter().enumerate().map(|(i, c)| json!({"weekday": i / 24, "hour": i % 24, "served": c[0], "requests": c[1], "downloads": c[2]})).collect::<Vec<_>>(),
        "from": s.from.max(hourly_limit),
    })))
}

// ----------------------------------------------------- repositories/jobs ----

async fn repos(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<Q>,
) -> ApiResult<Json<Value>> {
    let s = scope(&state, &p, &q)?;
    let ids = s.in_list();
    let (from, to) = (s.from, s.to);
    let admin = p.is_admin() && s.server;
    let (revisions, jobs, failures) = state
        .db
        .call(move |c| {
            // Revisions in range plus the last one before it (chart baseline).
            let mut st = c.prepare(&format!(
                "SELECT repo_id, revision, at, kind, files, total_size, new_addons, updated_addons, deleted_addons, duration_ms FROM revision_history
                 WHERE repo_id IN ({ids}) AND (at >= ?1 OR at = (SELECT MAX(at) FROM revision_history r2 WHERE r2.repo_id = revision_history.repo_id AND r2.at < ?1)) AND at < ?2
                 ORDER BY at"
            ))?;
            let revisions: Vec<(i64, Value)> = st
                .query_map([from, to], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        json!({"revision": r.get::<_, i64>(1)?, "at": r.get::<_, i64>(2)?, "kind": r.get::<_, String>(3)?, "files": r.get::<_, Option<i64>>(4)?,
                               "total_size": r.get::<_, Option<i64>>(5)?, "new": r.get::<_, Option<i64>>(6)?, "updated": r.get::<_, Option<i64>>(7)?,
                               "deleted": r.get::<_, Option<i64>>(8)?, "duration_ms": r.get::<_, Option<i64>>(9)?}),
                    ))
                })?
                .collect::<rusqlite::Result<_>>()?;
            let repo_filter = if admin { format!("(repo_id IN ({ids}) OR repo_id IS NULL)") } else { format!("repo_id IN ({ids})") };
            let mut st = c.prepare(&format!(
                "SELECT kind, status, started_at, finished_at, created_at FROM jobs WHERE {repo_filter} AND created_at >= ?1 AND created_at < ?2"
            ))?;
            let jobs: Vec<(String, String, Option<i64>, Option<i64>, i64)> = st
                .query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut st = c.prepare(&format!(
                "SELECT j.id, j.kind, r.slug, j.error, j.finished_at FROM jobs j LEFT JOIN repos r ON r.id = j.repo_id
                 WHERE {} AND j.status IN ('failed','interrupted') AND j.created_at >= ?1 AND j.created_at < ?2 ORDER BY j.id DESC LIMIT 10",
                repo_filter.replace("repo_id", "j.repo_id")
            ))?;
            let failures: Vec<Value> = st
                .query_map([from, to], |r| {
                    Ok(json!({"id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "repo": r.get::<_, Option<String>>(2)?, "error": r.get::<_, Option<String>>(3)?, "at": r.get::<_, Option<i64>>(4)?}))
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok((revisions, jobs, failures))
        })
        .await?;

    // Job statistics per kind.
    #[derive(Default, Serialize)]
    struct K {
        total: u64,
        succeeded: u64,
        failed: u64,
        cancelled: u64,
        #[serde(skip)]
        durations: Vec<i64>,
        #[serde(skip)]
        waits: Vec<i64>,
        avg_ms: i64,
        p95_ms: i64,
        avg_wait_ms: i64,
    }
    let mut kinds: BTreeMap<String, K> = BTreeMap::new();
    let mut weekly: BTreeMap<i64, u64> = BTreeMap::new();
    for (kind, status, started, finished, created) in &jobs {
        let k = kinds.entry(kind.clone()).or_default();
        k.total += 1;
        match status.as_str() {
            "succeeded" => k.succeeded += 1,
            "failed" | "interrupted" => k.failed += 1,
            "cancelled" => k.cancelled += 1,
            _ => {}
        }
        if let (Some(s0), Some(f)) = (started, finished) {
            k.durations.push(f - s0);
        }
        if let Some(s0) = started {
            k.waits.push(s0 - created);
        }
        if status == "succeeded" && matches!(kind.as_str(), "publish" | "build" | "mirror") {
            // Monday-based week start.
            let days = created.div_euclid(DAY);
            let monday = days - (days + 3).rem_euclid(7);
            *weekly.entry(monday * DAY).or_default() += 1;
        }
    }
    for k in kinds.values_mut() {
        if !k.durations.is_empty() {
            k.durations.sort_unstable();
            k.avg_ms = k.durations.iter().sum::<i64>() / k.durations.len() as i64;
            k.p95_ms = k.durations
                [((k.durations.len() as f64 * 0.95) as usize).min(k.durations.len() - 1)];
        }
        if !k.waits.is_empty() {
            k.avg_wait_ms = k.waits.iter().sum::<i64>() / k.waits.len() as i64;
        }
    }
    let mut per_repo: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (r, v) in revisions {
        if let Some(slug) = s.slugs.get(&r) {
            per_repo.entry(slug.clone()).or_default().push(v);
        }
    }
    let current: Vec<Value> = state
        .repo_list()
        .iter()
        .filter(|rt| s.ids.contains(&rt.record().id))
        .map(|rt| {
            let p = rt.published.load();
            json!({"repo": rt.record().slug, "name": rt.record().name, "revision": p.revision, "files": p.file_count, "total_size": p.total_size, "build_date": p.build_date})
        })
        .collect();
    Ok(Json(json!({
        "revisions": per_repo,
        "current": current,
        "jobs": kinds,
        "publishes_weekly": weekly.iter().map(|(t, n)| json!({"t": t, "count": n})).collect::<Vec<_>>(),
        "failures": failures,
    })))
}

// ---------------------------------------------------------------- live ----

async fn live(State(state): State<Shared>, _p: Principal) -> Json<Value> {
    let st = &state.stats;
    let series = st.live_series();
    let current = series.last().map(|(_, v)| *v).unwrap_or(0.0);
    let max = if state.cfg.max_downloads == 0 {
        None
    } else {
        Some(state.cfg.max_downloads)
    };
    Json(json!({
        "series": series.iter().map(|(t, v)| json!([t, v])).collect::<Vec<_>>(),
        "throughput": current,
        "downloads": st.active_downloads.load(Ordering::Relaxed).max(0),
        "players": if state.cfg.stats.players { Some(st.active_players()) } else { None },
        "max_downloads": max,
        "bandwidth_limit": state.cfg.bandwidth_limit,
    }))
}
