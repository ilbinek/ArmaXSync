//! HTTP routing, API error type, and the users/tokens/jobs/stats endpoints.

use std::convert::Infallible;

use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::auth::{self, Principal, Role};
use crate::db::now;
use crate::state::Shared;

// ------------------------------------------------------------------ errors ----

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self {
            status,
            message: msg.into(),
        }
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not found")
    }
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden")
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "authentication required")
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, msg)
    }
    pub fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {e}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"error": self.message})),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::internal(format!("{e:#}"))
    }
}

// ------------------------------------------------------------------ router ----

pub fn router(state: Shared) -> Router {
    let api = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/setup", get(setup_status).post(setup))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", patch(update_user).delete(delete_user))
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}", delete(delete_token))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/events", get(job_events))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/audit", get(crate::audit::list))
        .route("/stats", get(crate::stats::stats))
        .merge(crate::stats::api::routes())
        .route("/server", get(server_info))
        .route("/system", get(system_info))
        .route("/system/update/check", post(update_check))
        .route("/system/update/install", post(update_install))
        .merge(crate::repos_api::routes())
        .merge(crate::changes::routes())
        .merge(crate::push::routes())
        .route("/public/repos", get(crate::repos_api::public_list))
        .route("/public/repos/{slug}", get(crate::repos_api::public_detail))
        .fallback(|| async { ApiError::not_found() })
        .layer(DefaultBodyLimit::max(8 << 20));

    Router::new()
        .nest("/api/v1", api)
        .route("/r/{*rest}", any(serve_repo))
        .route("/metrics", get(crate::stats::metrics))
        .route("/healthz", get(|| async { "ok" }))
        .merge(crate::web::routes())
        .fallback(fallback)
        .layer(axum::middleware::map_response(version_header))
        .with_state(state)
}

/// Lets open admin pages notice that the server was updated underneath them.
async fn version_header(mut res: Response) -> Response {
    res.headers_mut().insert(
        "x-axs-version",
        axum::http::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );
    res
}

async fn serve_repo(State(state): State<Shared>, req: Request) -> Response {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
    }
    let raw = req.uri().path().to_owned();
    match crate::files::split_repo_path(&raw) {
        Some((slug, rest)) => {
            let peer = req
                .extensions()
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0);
            crate::files::serve(state, &slug, &rest, &method, req.headers(), peer).await
        }
        None => ApiError::not_found().into_response(),
    }
}

/// Legacy URL aliases, then the web UI.
async fn fallback(State(state): State<Shared>, req: Request) -> Response {
    let raw = req.uri().path().to_owned();
    let method = req.method().clone();
    if method == Method::GET || method == Method::HEAD {
        for a in &state.cfg.aliases {
            let prefix = a.path.trim_end_matches('/');
            if let Some(rest) = raw.strip_prefix(prefix) {
                if rest.is_empty() || rest.starts_with('/') {
                    let peer = req
                        .extensions()
                        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                        .map(|c| c.0);
                    return crate::files::serve(
                        state.clone(),
                        &a.repo,
                        rest,
                        &method,
                        req.headers(),
                        peer,
                    )
                    .await;
                }
            }
        }
    }
    crate::web::fallback(State(state), req).await
}

// ------------------------------------------------------------ setup/users ----

#[derive(Serialize)]
struct SetupStatus {
    needs_setup: bool,
}

async fn user_count(state: &Shared) -> ApiResult<i64> {
    state
        .db
        .call(|c| c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)))
        .await
        .map_err(ApiError::internal)
}

async fn setup_status(State(state): State<Shared>) -> ApiResult<Json<SetupStatus>> {
    Ok(Json(SetupStatus {
        needs_setup: user_count(&state).await? == 0,
    }))
}

#[derive(Deserialize)]
struct SetupReq {
    token: String,
    username: String,
    password: String,
}

/// First-run: create the owner with the setup token printed at startup.
async fn setup(State(state): State<Shared>, Json(req): Json<SetupReq>) -> ApiResult<StatusCode> {
    if user_count(&state).await? > 0 {
        return Err(ApiError::conflict("already set up"));
    }
    let expected = crate::SETUP_TOKEN.get().cloned().unwrap_or_default();
    if expected.is_empty() || !crate::crypto::ct_eq(&req.token, &expected) {
        return Err(ApiError::forbidden());
    }
    auth::create_user(&state, &req.username, &req.password, Role::Owner)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    crate::audit::record(
        &state,
        None,
        "setup.owner_created",
        Some(&req.username),
        None,
        None,
    )
    .await;
    Ok(StatusCode::CREATED)
}

#[derive(Serialize)]
struct UserOut {
    id: i64,
    username: String,
    role: Role,
    disabled: bool,
    created_at: i64,
    maintains: Vec<String>,
}

async fn list_users(State(state): State<Shared>, p: Principal) -> ApiResult<Json<Vec<UserOut>>> {
    p.require_admin()?;
    let rows = state
        .db
        .call(|c| {
            let mut st = c.prepare(
                "SELECT u.id, u.username, u.role, u.disabled, u.created_at, \
                 (SELECT group_concat(r.slug, ',') FROM repo_roles rr JOIN repos r ON r.id=rr.repo_id WHERE rr.user_id=u.id) \
                 FROM users u ORDER BY u.username",
            )?;
            let r = st.query_map([], |r| {
                let m: Option<String> = r.get(5)?;
                Ok(UserOut {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    role: Role::parse(&r.get::<_, String>(2)?),
                    disabled: r.get::<_, i64>(3)? != 0,
                    created_at: r.get(4)?,
                    maintains: m.map(|s| s.split(',').map(str::to_owned).collect()).unwrap_or_default(),
                })
            })?;
            r.collect()
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NewUser {
    username: String,
    password: String,
    #[serde(default = "default_role")]
    role: Role,
}

fn default_role() -> Role {
    Role::User
}

async fn create_user(
    State(state): State<Shared>,
    p: Principal,
    Json(u): Json<NewUser>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    p.require_admin()?;
    if u.role == Role::Owner && p.role != Role::Owner {
        return Err(ApiError::forbidden());
    }
    let id = auth::create_user(&state, &u.username, &u.password, u.role)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    crate::audit::record(
        &state,
        Some(&p),
        "user.create",
        Some(&u.username),
        Some(serde_json::json!({"role": u.role})),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

#[derive(Deserialize)]
struct UserPatch {
    password: Option<String>,
    role: Option<Role>,
    disabled: Option<bool>,
}

async fn update_user(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
    Json(u): Json<UserPatch>,
) -> ApiResult<StatusCode> {
    let self_edit = id == p.user_id;
    if !self_edit {
        p.require_admin()?;
    }
    let target_role: Option<String> = state
        .db
        .call(move |c| {
            c.query_row("SELECT role FROM users WHERE id=?1", [id], |r| r.get(0))
                .optional()
        })
        .await
        .map_err(ApiError::internal)?;
    let Some(target_role) = target_role.map(|r| Role::parse(&r)) else {
        return Err(ApiError::not_found());
    };
    if target_role == Role::Owner && p.role != Role::Owner {
        return Err(ApiError::forbidden());
    }
    if (u.role.is_some() || u.disabled.is_some()) && (self_edit || !p.is_admin()) {
        return Err(ApiError::bad("cannot change your own role or status"));
    }
    if u.role == Some(Role::Owner) && p.role != Role::Owner {
        return Err(ApiError::forbidden());
    }
    if let Some(pw) = &u.password {
        if pw.len() < 8 {
            return Err(ApiError::bad("password must be at least 8 characters"));
        }
        let pw = pw.clone();
        let h = tokio::task::spawn_blocking(move || crate::crypto::hash_password(&pw))
            .await
            .map_err(ApiError::internal)?;
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE users SET password_hash=?1 WHERE id=?2",
                    rusqlite::params![h, id],
                )?;
                c.execute("DELETE FROM sessions WHERE user_id=?1", [id])
            })
            .await
            .map_err(ApiError::internal)?;
    }
    if let Some(r) = u.role {
        let r = r.as_str();
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE users SET role=?1 WHERE id=?2",
                    rusqlite::params![r, id],
                )
            })
            .await
            .map_err(ApiError::internal)?;
    }
    if let Some(d) = u.disabled {
        state
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE users SET disabled=?1 WHERE id=?2",
                    rusqlite::params![d as i64, id],
                )?;
                c.execute("DELETE FROM sessions WHERE user_id=?1", [id])
            })
            .await
            .map_err(ApiError::internal)?;
    }
    crate::audit::record(&state, Some(&p), "user.update", Some(&id.to_string()), Some(serde_json::json!({"role": u.role, "disabled": u.disabled, "password": u.password.is_some()})), None).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    p.require_admin()?;
    if id == p.user_id {
        return Err(ApiError::bad("cannot delete yourself"));
    }
    let role: Option<String> = state
        .db
        .call(move |c| {
            c.query_row("SELECT role FROM users WHERE id=?1", [id], |r| r.get(0))
                .optional()
        })
        .await
        .map_err(ApiError::internal)?;
    match role.as_deref() {
        None => return Err(ApiError::not_found()),
        Some("owner") => return Err(ApiError::bad("the owner cannot be deleted")),
        _ => {}
    }
    state
        .db
        .call(move |c| c.execute("DELETE FROM users WHERE id=?1", [id]))
        .await
        .map_err(ApiError::internal)?;
    crate::audit::record(
        &state,
        Some(&p),
        "user.delete",
        Some(&id.to_string()),
        None,
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ tokens ----

#[derive(Serialize)]
struct TokenOut {
    id: i64,
    name: String,
    scopes: Vec<String>,
    created_at: i64,
    last_used: Option<i64>,
    expires_at: Option<i64>,
}

async fn list_tokens(State(state): State<Shared>, p: Principal) -> ApiResult<Json<Vec<TokenOut>>> {
    let uid = p.user_id;
    let rows = state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT id, name, scopes, created_at, last_used, expires_at FROM api_tokens WHERE user_id=?1 ORDER BY id")?;
            let r = st.query_map([uid], |r| {
                Ok(TokenOut {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    scopes: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                    created_at: r.get(3)?,
                    last_used: r.get(4)?,
                    expires_at: r.get(5)?,
                })
            })?;
            r.collect()
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NewToken {
    name: String,
    /// "admin", "push:<slug>", "manage:<slug>"
    scopes: Vec<String>,
    #[serde(default)]
    expires_days: Option<i64>,
}

async fn create_token(
    State(state): State<Shared>,
    p: Principal,
    Json(t): Json<NewToken>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    if p.scopes.is_some() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "tokens cannot create tokens",
        ));
    }
    for s in &t.scopes {
        let ok = s == "admin" && p.is_admin()
            || s.strip_prefix("push:")
                .or_else(|| s.strip_prefix("manage:"))
                .map(|_| true)
                .unwrap_or(false);
        if !ok {
            return Err(ApiError::bad(format!("invalid scope {s:?}")));
        }
    }
    let secret = format!("axs_{}", crate::crypto::random_token(30));
    let h = crate::crypto::sha256_hex(&secret);
    let (uid, name, scopes) = (
        p.user_id,
        t.name.clone(),
        serde_json::to_string(&t.scopes).unwrap_or_default(),
    );
    let exp = t.expires_days.map(|d| now() + d * 86_400_000);
    let id = state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO api_tokens(user_id, name, token_hash, scopes, created_at, expires_at) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![uid, name, h, scopes, now(), exp],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await
        .map_err(ApiError::internal)?;
    crate::audit::record(
        &state,
        Some(&p),
        "token.create",
        Some(&t.name),
        Some(serde_json::json!({"scopes": t.scopes})),
        None,
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id": id, "token": secret})),
    ))
}

async fn delete_token(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let uid = p.user_id;
    let admin = p.is_admin();
    let n = state
        .db
        .call(move |c| {
            if admin {
                c.execute("DELETE FROM api_tokens WHERE id=?1", [id])
            } else {
                c.execute(
                    "DELETE FROM api_tokens WHERE id=?1 AND user_id=?2",
                    rusqlite::params![id, uid],
                )
            }
        })
        .await
        .map_err(ApiError::internal)?;
    if n == 0 {
        return Err(ApiError::not_found());
    }
    crate::audit::record(
        &state,
        Some(&p),
        "token.delete",
        Some(&id.to_string()),
        None,
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------------------- jobs ----

#[derive(Serialize)]
pub struct JobOut {
    pub id: i64,
    pub repo: Option<String>,
    pub kind: String,
    pub status: String,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub summary: Option<serde_json::Value>,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

#[derive(Deserialize)]
struct JobQuery {
    repo: Option<String>,
    limit: Option<i64>,
    /// `active` (queued or running), or one status.
    status: Option<String>,
    /// Only jobs with an id below this (paging).
    before: Option<i64>,
}

fn job_row(r: &rusqlite::Row, with_log: bool) -> rusqlite::Result<JobOut> {
    let summary: Option<String> = r.get(7)?;
    Ok(JobOut {
        id: r.get(0)?,
        repo: r.get(1)?,
        kind: r.get(2)?,
        status: r.get(3)?,
        created_at: r.get(4)?,
        started_at: r.get(5)?,
        finished_at: r.get(6)?,
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
        error: r.get(8)?,
        log: if with_log { r.get(9)? } else { None },
    })
}

const JOB_COLS: &str = "j.id, r.slug, j.kind, j.status, j.created_at, j.started_at, j.finished_at, j.summary, j.error, j.log, j.repo_id";

async fn list_jobs(
    State(state): State<Shared>,
    p: Principal,
    Query(q): Query<JobQuery>,
) -> ApiResult<Json<Vec<JobOut>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let repo = q.repo.clone();
    let status = q.status.clone().filter(|s| !s.is_empty());
    let before = q.before;
    let rows: Vec<(JobOut, Option<i64>)> = state
        .db
        .call(move |c| {
            let sql = format!("SELECT {JOB_COLS} FROM jobs j LEFT JOIN repos r ON r.id=j.repo_id WHERE (?1 IS NULL OR r.slug=?1) AND (?3 IS NULL OR (?3='active' AND j.status IN ('queued','running')) OR j.status=?3) AND (?4 IS NULL OR j.id < ?4) ORDER BY j.id DESC LIMIT ?2");
            let mut st = c.prepare(&sql)?;
            let r = st.query_map(rusqlite::params![repo, limit, status, before], |r| Ok((job_row(r, false)?, r.get(10)?)))?;
            r.collect()
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(
        rows.into_iter()
            .filter(|(j, rid)| {
                p.is_admin()
                    || rid
                        .map(|id| p.can_manage(id, j.repo.as_deref().unwrap_or("")))
                        .unwrap_or(false)
            })
            .map(|(j, _)| j)
            .collect(),
    ))
}

async fn load_job(state: &Shared, id: i64) -> ApiResult<(JobOut, Option<i64>)> {
    state
        .db
        .call(move |c| {
            c.query_row(&format!("SELECT {JOB_COLS} FROM jobs j LEFT JOIN repos r ON r.id=j.repo_id WHERE j.id=?1"), [id], |r| {
                Ok((job_row(r, true)?, r.get(10)?))
            })
            .optional()
        })
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(ApiError::not_found)
}

fn check_job_access(p: &Principal, job: &JobOut, repo_id: Option<i64>) -> ApiResult<()> {
    if p.is_admin()
        || repo_id
            .map(|id| p.can_manage(id, job.repo.as_deref().unwrap_or("")))
            .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn get_job(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<Json<JobOut>> {
    let (mut job, rid) = load_job(&state, id).await?;
    check_job_access(&p, &job, rid)?;
    if let Some((_, log)) = state.jobs.subscribe(id) {
        job.log = Some(log);
    }
    Ok(Json(job))
}

async fn cancel_job(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let (job, rid) = load_job(&state, id).await?;
    check_job_access(&p, &job, rid)?;
    if state.jobs.cancel(id) {
        crate::audit::record(
            &state,
            Some(&p),
            "job.cancel",
            Some(&id.to_string()),
            None,
            None,
        )
        .await;
        Ok(StatusCode::ACCEPTED)
    } else {
        Err(ApiError::conflict("job is not running"))
    }
}

/// Server-sent events: log lines, progress and status of a running job.
async fn job_events(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let (job, rid) = load_job(&state, id).await?;
    check_job_access(&p, &job, rid)?;
    let sub = state.jobs.subscribe(id);
    let stream = async_stream::stream! {
        match sub {
            None => {
                yield Ok::<Event, Infallible>(Event::default().event("status").data(serde_json::json!({"type":"status","status": job.status}).to_string()));
            }
            Some((mut rx, log)) => {
                for line in log.lines() {
                    yield Ok(Event::default().event("log").data(serde_json::json!({"type":"log","line": line}).to_string()));
                }
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            let kind = match &ev { crate::jobs::JobEvent::Log{..} => "log", crate::jobs::JobEvent::Progress{..} => "progress", crate::jobs::JobEvent::Status{..} => "status" };
                            let done = matches!(&ev, crate::jobs::JobEvent::Status{status} if status != "running");
                            yield Ok(Event::default().event(kind).data(serde_json::to_string(&ev).unwrap_or_default()));
                            if done { break; }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

// ------------------------------------------------------------------ server ----

async fn server_info(State(state): State<Shared>, _p: Principal) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "public_url": state.cfg.public_url,
        "started": state.started,
        "repositories": state.repo_list().len(),
    }))
}

/// Version, install kind and update status (any signed-in user; update
/// details only for admins).
async fn system_info(State(state): State<Shared>, p: Principal) -> Json<serde_json::Value> {
    let mut v = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "public_url": state.cfg.public_url,
        "started": state.started,
        "repositories": state.repo_list().len(),
    });
    if p.is_admin() {
        v["update"] =
            serde_json::to_value(state.updates.snapshot(&state.cfg.data_dir)).unwrap_or_default();
    }
    Json(v)
}

async fn update_check(
    State(state): State<Shared>,
    p: Principal,
) -> ApiResult<Json<crate::updates::UpdateStatus>> {
    p.require_admin()?;
    crate::updates::check(&state)
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(Json(state.updates.snapshot(&state.cfg.data_dir)))
}

#[derive(Deserialize)]
struct InstallReq {
    version: String,
}

async fn update_install(
    State(state): State<Shared>,
    p: Principal,
    Json(req): Json<InstallReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if p.role != Role::Owner || p.scopes.is_some() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "only the owner can install updates (from the web UI)",
        ));
    }
    let st = state.updates.snapshot(&state.cfg.data_dir);
    if !st.can_install {
        return Err(ApiError::conflict(match st.install_kind {
            crate::updates::InstallKind::Systemd => {
                "no installable update; check for updates first"
            }
            _ => "this server cannot update itself; update the container or binary instead",
        }));
    }
    if st.staged.is_some() {
        return Err(ApiError::conflict("an update is already being installed"));
    }
    let latest = st.latest.map(|l| l.version).unwrap_or_default();
    if latest != req.version {
        return Err(ApiError::conflict(format!(
            "the available version is {latest}, not {}",
            req.version
        )));
    }
    let job = crate::updates::start_install(&state, req.version.clone(), Some(p.user_id)).await?;
    crate::audit::record(
        &state,
        Some(&p),
        "server.update",
        None,
        Some(serde_json::json!({"from": env!("CARGO_PKG_VERSION"), "to": req.version})),
        None,
    )
    .await;
    Ok(Json(serde_json::json!({"job": job})))
}

pub fn header_str<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}
