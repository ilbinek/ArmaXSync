//! Users, sessions, API tokens and permission checks.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::crypto;
use crate::db::now;
use crate::state::Shared;

pub const SESSION_COOKIE: &str = "axs_session";
pub const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Admin,
    User,
}

impl Role {
    pub fn parse(s: &str) -> Role {
        match s {
            "owner" => Role::Owner,
            "admin" => Role::Admin,
            _ => Role::User,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }
}

/// The authenticated caller.
#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    pub user_id: i64,
    pub username: String,
    pub role: Role,
    /// API token scopes (`None` for interactive sessions).
    pub scopes: Option<Vec<String>>,
    #[serde(skip)]
    pub maintained: Vec<i64>,
    #[serde(skip)]
    pub ip: Option<IpAddr>,
}

impl Principal {
    pub fn is_admin(&self) -> bool {
        matches!(self.role, Role::Owner | Role::Admin)
            && self
                .scopes
                .as_ref()
                .map(|s| s.iter().any(|x| x == "admin"))
                .unwrap_or(true)
    }

    /// Can modify content/settings of a repository.
    pub fn can_manage(&self, repo_id: i64, slug: &str) -> bool {
        if let Some(scopes) = &self.scopes {
            let has = scopes.iter().any(|s| {
                s == "admin" || s == &format!("push:{slug}") || s == &format!("manage:{slug}")
            });
            return has
                && (matches!(self.role, Role::Owner | Role::Admin)
                    || self.maintained.contains(&repo_id));
        }
        matches!(self.role, Role::Owner | Role::Admin) || self.maintained.contains(&repo_id)
    }

    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }

    pub fn require_manage(&self, repo_id: i64, slug: &str) -> ApiResult<()> {
        if self.can_manage(repo_id, slug) {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
}

/// Real client address (honours X-Forwarded-For from trusted proxies).
pub fn client_ip(state: &Shared, peer: Option<SocketAddr>, headers: &HeaderMap) -> Option<IpAddr> {
    let peer_ip = peer.map(|p| p.ip());
    let trusted = peer_ip
        .map(|ip| {
            state
                .cfg
                .trusted_proxies
                .iter()
                .any(|t| t.parse::<IpAddr>().ok() == Some(ip))
        })
        .unwrap_or(false);
    if trusted {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff
                .split(',')
                .map(str::trim)
                .find_map(|s| s.parse::<IpAddr>().ok())
            {
                return Some(first);
            }
        }
    }
    peer_ip
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for v in headers.get_all(header::COOKIE) {
        let Ok(s) = v.to_str() else { continue };
        for part in s.split(';') {
            if let Some((k, val)) = part.trim().split_once('=') {
                if k == name {
                    return Some(val.to_owned());
                }
            }
        }
    }
    None
}

async fn maintained(state: &Shared, user_id: i64) -> Vec<i64> {
    state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT repo_id FROM repo_roles WHERE user_id=?1")?;
            let r = st.query_map([user_id], |r| r.get(0))?;
            r.collect()
        })
        .await
        .unwrap_or_default()
}

impl FromRequestParts<Shared> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Shared,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0);
        let ip = client_ip(state, peer, &parts.headers);
        // API token
        if let Some(tok) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
        {
            let h = crypto::sha256_hex(tok);
            let row: Option<(i64, String, String, String, Option<i64>, bool)> = state
                .db
                .call(move |c| {
                    c.query_row(
                        "SELECT u.id, u.username, u.role, t.scopes, t.expires_at, u.disabled FROM api_tokens t JOIN users u ON u.id=t.user_id WHERE t.token_hash=?1",
                        [&h],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, i64>(5)? != 0)),
                    )
                    .optional()
                })
                .await
                .map_err(ApiError::internal)?;
            let Some((uid, username, role, scopes, exp, disabled)) = row else {
                return Err(ApiError::unauthorized());
            };
            if disabled || exp.map(|e| e < now()).unwrap_or(false) {
                return Err(ApiError::unauthorized());
            }
            let h2 = crypto::sha256_hex(tok);
            let _ = state
                .db
                .call(move |c| {
                    c.execute(
                        "UPDATE api_tokens SET last_used=?1 WHERE token_hash=?2",
                        rusqlite::params![now(), h2],
                    )
                })
                .await;
            return Ok(Principal {
                user_id: uid,
                username,
                role: Role::parse(&role),
                scopes: Some(serde_json::from_str(&scopes).unwrap_or_default()),
                maintained: maintained(state, uid).await,
                ip,
            });
        }
        // Session cookie
        let Some(tok) = cookie_value(&parts.headers, SESSION_COOKIE) else {
            return Err(ApiError::unauthorized());
        };
        let h = crypto::sha256_hex(&tok);
        let row: Option<(i64, String, String, String, i64, bool)> = state
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT u.id, u.username, u.role, s.csrf, s.expires_at, u.disabled FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?1",
                    [&h],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, i64>(5)? != 0)),
                )
                .optional()
            })
            .await
            .map_err(ApiError::internal)?;
        let Some((uid, username, role, csrf, exp, disabled)) = row else {
            return Err(ApiError::unauthorized());
        };
        if disabled || exp < now() {
            return Err(ApiError::unauthorized());
        }
        if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !crypto::ct_eq(sent, &csrf) {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "missing or invalid CSRF token",
                ));
            }
        }
        Ok(Principal {
            user_id: uid,
            username,
            role: Role::parse(&role),
            scopes: None,
            maintained: maintained(state, uid).await,
            ip,
        })
    }
}

// ------------------------------------------------------------- endpoints ----

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct MeResp {
    pub user: Principal,
    pub csrf: Option<String>,
}

/// Brute-force protection: failed logins per IP per window.
pub struct LoginLimiter {
    map: Mutex<HashMap<IpAddr, (u32, i64)>>,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl LoginLimiter {
    fn blocked(&self, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return false };
        let m = self.map.lock().unwrap();
        matches!(m.get(&ip), Some((n, since)) if *n >= 10 && now() - since < 15 * 60_000)
    }
    fn fail(&self, ip: Option<IpAddr>) {
        let Some(ip) = ip else { return };
        let mut m = self.map.lock().unwrap();
        let e = m.entry(ip).or_insert((0, now()));
        if now() - e.1 > 15 * 60_000 {
            *e = (0, now());
        }
        e.0 += 1;
    }
    fn ok(&self, ip: Option<IpAddr>) {
        if let Some(ip) = ip {
            self.map.lock().unwrap().remove(&ip);
        }
    }
}

pub static LOGIN_LIMITER: std::sync::LazyLock<LoginLimiter> =
    std::sync::LazyLock::new(LoginLimiter::default);

pub async fn login(
    State(state): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> ApiResult<Response> {
    let ip = client_ip(&state, Some(peer), &headers);
    if LOGIN_LIMITER.blocked(ip) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too many failed logins, try again later",
        ));
    }
    let uname = req.username.clone();
    let row: Option<(i64, String, String, bool)> = state
        .db
        .call(move |c| {
            c.query_row(
                "SELECT id, password_hash, role, disabled FROM users WHERE username=?1",
                [&uname],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0)),
            )
            .optional()
        })
        .await
        .map_err(ApiError::internal)?;
    let pw = req.password.clone();
    let ok = match &row {
        Some((_, hash, _, disabled)) => {
            let hash = hash.clone();
            !disabled
                && tokio::task::spawn_blocking(move || crypto::verify_password(&pw, &hash))
                    .await
                    .unwrap_or(false)
        }
        None => {
            // Equalize timing for unknown users.
            let _ = tokio::task::spawn_blocking(move || crypto::hash_password(&pw)).await;
            false
        }
    };
    if !ok {
        LOGIN_LIMITER.fail(ip);
        crate::audit::record(&state, None, "login.failed", Some(&req.username), None, ip).await;
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid username or password",
        ));
    }
    LOGIN_LIMITER.ok(ip);
    let (uid, _, role, _) = row.unwrap();
    let token = crypto::random_token(32);
    let csrf = crypto::random_token(18);
    let (th, csrf2) = (crypto::sha256_hex(&token), csrf.clone());
    let exp = now() + state.cfg.session_hours as i64 * 3_600_000;
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>();
    let ipstr = ip.map(|i| i.to_string());
    state
        .db
        .call(move |c| {
            c.execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])?;
            c.execute(
                "INSERT INTO sessions(token_hash, user_id, csrf, created_at, expires_at, ip, user_agent) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![th, uid, csrf2, now(), exp, ipstr, ua],
            )
        })
        .await
        .map_err(ApiError::internal)?;
    let principal = Principal {
        user_id: uid,
        username: req.username.clone(),
        role: Role::parse(&role),
        scopes: None,
        maintained: vec![],
        ip,
    };
    crate::audit::record(&state, Some(&principal), "login", None, None, ip).await;
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        state.cfg.session_hours * 3600,
        if state.cfg.secure_cookies() {
            "; Secure"
        } else {
            ""
        }
    );
    let mut resp = Json(MeResp {
        user: principal,
        csrf: Some(csrf),
    })
    .into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(ApiError::internal)?,
    );
    Ok(resp)
}

pub async fn logout(State(state): State<Shared>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(tok) = cookie_value(&headers, SESSION_COOKIE) {
        let h = crypto::sha256_hex(&tok);
        state
            .db
            .call(move |c| c.execute("DELETE FROM sessions WHERE token_hash=?1", [h]))
            .await
            .map_err(ApiError::internal)?;
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("axs_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(resp)
}

pub async fn me(
    State(state): State<Shared>,
    headers: HeaderMap,
    p: Principal,
) -> ApiResult<Json<MeResp>> {
    let csrf = match cookie_value(&headers, SESSION_COOKIE) {
        Some(tok) if p.scopes.is_none() => {
            let h = crypto::sha256_hex(&tok);
            state
                .db
                .call(move |c| {
                    c.query_row("SELECT csrf FROM sessions WHERE token_hash=?1", [h], |r| {
                        r.get(0)
                    })
                    .optional()
                })
                .await
                .map_err(ApiError::internal)?
        }
        _ => None,
    };
    Ok(Json(MeResp { user: p, csrf }))
}

/// Create a user (CLI `init`/`user add` and the API).
pub async fn create_user(
    state: &Shared,
    username: &str,
    password: &str,
    role: Role,
) -> anyhow::Result<i64> {
    if username.trim().is_empty() || username.len() > 64 {
        anyhow::bail!("invalid username");
    }
    if password.len() < 8 {
        anyhow::bail!("password must be at least 8 characters");
    }
    let hash = {
        let pw = password.to_owned();
        tokio::task::spawn_blocking(move || crypto::hash_password(&pw)).await?
    };
    let (u, r) = (username.trim().to_owned(), role.as_str());
    state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO users(username, password_hash, role, created_at) VALUES (?1,?2,?3,?4)",
                rusqlite::params![u, hash, r, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await
}
