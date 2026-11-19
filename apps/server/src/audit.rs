//! Audit log of administrative actions.

use std::net::IpAddr;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::auth::Principal;
use crate::db::now;
use crate::state::Shared;

pub async fn record(
    state: &Shared,
    who: Option<&Principal>,
    action: &str,
    target: Option<&str>,
    details: Option<serde_json::Value>,
    ip: Option<IpAddr>,
) {
    let (uid, uname) = (who.map(|p| p.user_id), who.map(|p| p.username.clone()));
    let (action, target) = (action.to_owned(), target.map(str::to_owned));
    let details = details.map(|d| d.to_string());
    let ip = ip.or(who.and_then(|p| p.ip)).map(|i| i.to_string());
    let r = state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO audit_log(at, user_id, username, action, target, details, ip) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![now(), uid, uname, action, target, details, ip],
            )
        })
        .await;
    if let Err(e) = r {
        tracing::warn!("audit log write failed: {e}");
    }
}

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub before: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub target: Option<String>,
    /// Exact username.
    #[serde(default)]
    pub user: Option<String>,
    /// Action prefix, e.g. `repo.` or `login`.
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: i64,
    pub username: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip: Option<String>,
}

pub async fn list(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    p.require_admin()?;
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let before = q.before.unwrap_or(i64::MAX);
    let target = q.target.clone().filter(|s| !s.is_empty());
    let user = q.user.clone().filter(|s| !s.is_empty());
    let action = q.action.clone().filter(|s| !s.is_empty());
    let rows = state
        .db
        .call(move |c| {
            let mut st = c.prepare(
                "SELECT id, at, username, action, target, details, ip FROM audit_log WHERE id < ?1 AND (?2 IS NULL OR target = ?2) AND (?4 IS NULL OR username = ?4) AND (?5 IS NULL OR action LIKE ?5 || '%') ORDER BY id DESC LIMIT ?3",
            )?;
            let r = st.query_map(rusqlite::params![before, target, limit, user, action], |r| {
                let d: Option<String> = r.get(5)?;
                Ok(AuditEntry {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    username: r.get(2)?,
                    action: r.get(3)?,
                    target: r.get(4)?,
                    details: d.and_then(|s| serde_json::from_str(&s).ok()),
                    ip: r.get(6)?,
                })
            })?;
            r.collect()
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}
