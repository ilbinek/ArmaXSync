//! Repository management API and public repository info.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use axs_model::{EventDef, GameId};

use crate::api::{ApiError, ApiResult};
use crate::auth::Principal;
use crate::db::now;
use crate::jobs::{JobKind, JobManager};
use crate::state::{
    repo_by_slug, MirrorSettings, RepoMode, RepoRecord, RepoRuntime, RepoSettings, Shared,
    Visibility,
};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/repos", get(list).post(create))
        .route("/repos/{slug}", get(detail).patch(update).delete(remove))
        .route("/repos/{slug}/credentials", get(list_creds).post(add_cred))
        .route("/repos/{slug}/credentials/{id}", delete(delete_cred))
        .route("/repos/{slug}/events", get(get_events).put(put_events))
        .route(
            "/repos/{slug}/schedules",
            get(get_schedules).put(put_schedules),
        )
        .route(
            "/repos/{slug}/maintainers",
            get(get_maintainers).put(put_maintainers),
        )
        .route("/repos/{slug}/tree", get(tree))
        .route("/repos/{slug}/build", post(build))
        .route("/repos/{slug}/mirror", post(mirror))
}

pub fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !s.starts_with('-')
}

#[derive(Serialize)]
pub struct RepoSummary {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub game: GameId,
    pub mode: RepoMode,
    pub visibility: Visibility,
    pub revision: u32,
    pub build_date: i64,
    pub files: usize,
    pub size: u64,
    pub url: String,
    pub autoconfig_url: String,
    pub deep_link: String,
    pub bytes_served: u64,
    pub requests: u64,
    pub has_mirror: bool,
}

fn summary(state: &Shared, rt: &RepoRuntime) -> RepoSummary {
    let r = rt.record();
    let p = rt.published.load();
    let url = state.cfg.repo_url(&r.slug);
    let enc = |s: &str| {
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    RepoSummary {
        slug: r.slug.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        game: r.game,
        mode: r.mode,
        visibility: r.settings.visibility,
        revision: p.revision,
        build_date: p.build_date,
        files: p.file_count,
        size: p.total_size,
        autoconfig_url: format!("{url}/.a3s/autoconfig"),
        deep_link: format!("armaxsync://add?url={}&name={}", enc(&url), enc(&r.name)),
        url,
        bytes_served: rt.bytes_served.load(std::sync::atomic::Ordering::Relaxed),
        requests: rt.requests.load(std::sync::atomic::Ordering::Relaxed),
        has_mirror: r.settings.mirror.is_some(),
    }
}

fn runtime(state: &Shared, slug: &str) -> ApiResult<std::sync::Arc<RepoRuntime>> {
    state.repo(slug).ok_or_else(ApiError::not_found)
}

fn manage(state: &Shared, p: &Principal, slug: &str) -> ApiResult<std::sync::Arc<RepoRuntime>> {
    let rt = runtime(state, slug)?;
    p.require_manage(rt.record().id, slug)?;
    Ok(rt)
}

async fn list(State(state): State<Shared>, p: Principal) -> Json<Vec<RepoSummary>> {
    Json(
        state
            .repo_list()
            .iter()
            .filter(|rt| p.can_manage(rt.record().id, &rt.record().slug) || p.is_admin())
            .map(|rt| summary(&state, rt))
            .collect(),
    )
}

#[derive(Deserialize)]
pub struct NewRepo {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub game: GameId,
    #[serde(default)]
    pub mode: RepoMode,
    /// Existing folder for `external` repositories.
    pub path: Option<String>,
    #[serde(default)]
    pub settings: Option<RepoSettings>,
}

pub async fn create_repo(
    state: &Shared,
    p: Option<&Principal>,
    n: NewRepo,
) -> ApiResult<RepoRecord> {
    let slug = n.slug.trim().to_lowercase();
    if !valid_slug(&slug) {
        return Err(ApiError::bad(
            "slug may contain letters, digits, '-' and '_' only",
        ));
    }
    if n.name.trim().is_empty() {
        return Err(ApiError::bad("name is required"));
    }
    if n.mode == RepoMode::External {
        let path = n
            .path
            .as_deref()
            .ok_or_else(|| ApiError::bad("external repositories need a path"))?;
        if !std::path::Path::new(path).is_dir() {
            return Err(ApiError::bad(format!(
                "{path} is not a folder on the server"
            )));
        }
    }
    if repo_by_slug(&state.db, &slug).await?.is_some() {
        return Err(ApiError::conflict("a repository with this slug exists"));
    }
    let settings = n.settings.unwrap_or_default();
    let js = serde_json::to_string(&settings).map_err(ApiError::internal)?;
    let (s2, name, desc, game, mode, path) = (
        slug.clone(),
        n.name.trim().to_owned(),
        n.description.clone(),
        n.game.to_string(),
        if n.mode == RepoMode::External {
            "external"
        } else {
            "managed"
        },
        n.path.clone().filter(|_| n.mode == RepoMode::External),
    );
    state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO repos(slug, name, description, game, mode, path, settings, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![s2, name, desc, game, mode, path, js, now()],
            )
        })
        .await?;
    state.refresh_repo(&slug).await?;
    let rec = repo_by_slug(&state.db, &slug)
        .await?
        .ok_or_else(ApiError::not_found)?;
    crate::audit::record(
        state,
        p,
        "repo.create",
        Some(&slug),
        Some(serde_json::json!({"mode": mode})),
        None,
    )
    .await;
    Ok(rec)
}

async fn create(
    State(state): State<Shared>,
    p: Principal,
    Json(n): Json<NewRepo>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    p.require_admin()?;
    let rec = create_repo(&state, Some(&p), n).await?;
    // Publish an initial (possibly empty / existing external) revision.
    let job = JobManager::submit(&state, JobKind::Build, Some(rec.id), Some(p.user_id)).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"slug": rec.slug, "job": job})),
    ))
}

#[derive(Serialize)]
struct RepoDetail {
    #[serde(flatten)]
    summary: RepoSummary,
    settings: RepoSettings,
    path: Option<String>,
    created_at: i64,
}

async fn detail(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<Json<RepoDetail>> {
    let rt = manage(&state, &p, &slug)?;
    let r = rt.record();
    let mut settings = r.settings.clone();
    if let Some(m) = settings.mirror.as_mut() {
        m.password_sealed = m.password_sealed.as_ref().map(|_| "********".into());
    }
    Ok(Json(RepoDetail {
        summary: summary(&state, &rt),
        settings,
        path: r
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .or_else(|| Some(rt.root.display().to_string())),
        created_at: r.created_at,
    }))
}

#[derive(Deserialize)]
struct RepoPatch {
    name: Option<String>,
    description: Option<String>,
    settings: Option<RepoSettings>,
    /// New mirror password (plain text; stored encrypted). Empty clears it.
    mirror_password: Option<String>,
    /// Rebuild afterwards (default: when published metadata is affected).
    rebuild: Option<bool>,
}

async fn update(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Json(u): Json<RepoPatch>,
) -> ApiResult<Json<serde_json::Value>> {
    let rt = manage(&state, &p, &slug)?;
    let old = rt.record();
    let mut name = old.name.clone();
    let mut desc = old.description.clone();
    let mut settings = old.settings.clone();
    if let Some(n) = u.name.filter(|n| !n.trim().is_empty()) {
        name = n.trim().to_owned();
    }
    if let Some(d) = u.description {
        desc = d;
    }
    if let Some(mut s) = u.settings {
        // Keep the sealed mirror password unless replaced.
        if let (Some(new_m), Some(old_m)) = (s.mirror.as_mut(), old.settings.mirror.as_ref()) {
            new_m.password_sealed = old_m.password_sealed.clone();
        }
        if let Some(m) = s.mirror.as_mut() {
            m.url = m.url.trim().to_owned();
            if m.url.is_empty() {
                s.mirror = None;
            }
        }
        s.connections = s.connections.clamp(1, 10);
        s.hidden = s
            .hidden
            .iter()
            .filter_map(|h| axs_model::paths::sanitize_rel_path(h))
            .collect();
        s.exclude = s
            .exclude
            .iter()
            .filter_map(|h| axs_model::paths::sanitize_rel_path(h))
            .collect();
        settings = s;
    }
    if let Some(pw) = u.mirror_password {
        let m = settings.mirror.get_or_insert_with(MirrorSettings::default);
        m.password_sealed = if pw.is_empty() {
            None
        } else {
            Some(state.secrets.seal(&pw))
        };
    }
    let affects_metadata = name != old.name
        || settings.compress != old.settings.compress
        || settings.compressed_only != old.settings.compressed_only
        || settings.connections != old.settings.connections
        || settings.partial_transfer != old.settings.partial_transfer
        || settings.embed_credentials != old.settings.embed_credentials
        || settings.hidden != old.settings.hidden
        || settings.exclude != old.settings.exclude
        || settings.servers != old.settings.servers
        || settings.events != old.settings.events;
    let js = serde_json::to_string(&settings).map_err(ApiError::internal)?;
    let id = old.id;
    let (n2, d2) = (name.clone(), desc.clone());
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE repos SET name=?1, description=?2, settings=?3 WHERE id=?4",
                rusqlite::params![n2, d2, js, id],
            )
        })
        .await?;
    state.refresh_repo(&slug).await?;
    crate::audit::record(&state, Some(&p), "repo.update", Some(&slug), None, None).await;
    let job = if u.rebuild.unwrap_or(affects_metadata) {
        Some(JobManager::submit(&state, JobKind::Build, Some(id), Some(p.user_id)).await?)
    } else {
        None
    };
    Ok(Json(serde_json::json!({"ok": true, "job": job})))
}

#[derive(Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    delete_files: bool,
}

async fn remove(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> ApiResult<StatusCode> {
    p.require_admin()?;
    let rt = runtime(&state, &slug)?;
    let r = rt.record();
    let id = r.id;
    state
        .db
        .call(move |c| c.execute("DELETE FROM repos WHERE id=?1", [id]))
        .await?;
    state.repos.write().unwrap().remove(&slug);
    if q.delete_files && r.mode == RepoMode::Managed {
        let _ = std::fs::remove_dir_all(&rt.root);
    }
    let _ = std::fs::remove_file(state.cache_path(&slug));
    crate::audit::record(
        &state,
        Some(&p),
        "repo.delete",
        Some(&slug),
        Some(serde_json::json!({"files": q.delete_files})),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ credentials ----

#[derive(Serialize)]
struct CredOut {
    id: i64,
    login: String,
    embed: bool,
    note: String,
}

async fn list_creds(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<CredOut>>> {
    let rt = manage(&state, &p, &slug)?;
    Ok(Json(
        rt.credentials
            .load()
            .iter()
            .map(|c| CredOut {
                id: c.id,
                login: c.login.clone(),
                embed: c.embed,
                note: c.note.clone(),
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct NewCred {
    login: String,
    /// Generated when absent.
    password: Option<String>,
    #[serde(default)]
    embed: bool,
    #[serde(default)]
    note: String,
}

async fn add_cred(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Json(n): Json<NewCred>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let rt = manage(&state, &p, &slug)?;
    let login = n.login.trim().to_owned();
    if login.is_empty() || login.contains(':') || login.eq_ignore_ascii_case("anonymous") {
        return Err(ApiError::bad("invalid login"));
    }
    let password = n
        .password
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| crate::crypto::random_token(12));
    let sealed = state.secrets.seal(&password);
    let id = rt.record().id;
    let (l2, embed, note) = (login.clone(), n.embed, n.note.clone());
    state
        .db
        .call(move |c| {
            if embed {
                c.execute("UPDATE repo_credentials SET embed=0 WHERE repo_id=?1", [id])?;
            }
            c.execute(
                "INSERT INTO repo_credentials(repo_id, login, secret, embed, note, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![id, l2, sealed, embed as i64, note, now()],
            )
        })
        .await?;
    state.refresh_repo(&slug).await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.credential.add",
        Some(&slug),
        Some(serde_json::json!({"login": login, "embed": n.embed})),
        None,
    )
    .await;
    let job = if n.embed {
        Some(JobManager::submit(&state, JobKind::Build, Some(id), Some(p.user_id)).await?)
    } else {
        None
    };
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"login": login, "password": password, "job": job})),
    ))
}

async fn delete_cred(
    State(state): State<Shared>,
    p: Principal,
    Path((slug, cid)): Path<(String, i64)>,
) -> ApiResult<StatusCode> {
    let rt = manage(&state, &p, &slug)?;
    let id = rt.record().id;
    let was_embedded = rt.credentials.load().iter().any(|c| c.id == cid && c.embed);
    let n = state
        .db
        .call(move |c| {
            c.execute(
                "DELETE FROM repo_credentials WHERE id=?1 AND repo_id=?2",
                rusqlite::params![cid, id],
            )
        })
        .await?;
    if n == 0 {
        return Err(ApiError::not_found());
    }
    state.refresh_repo(&slug).await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.credential.delete",
        Some(&slug),
        Some(serde_json::json!({"id": cid})),
        None,
    )
    .await;
    if was_embedded {
        JobManager::submit(&state, JobKind::Build, Some(id), Some(p.user_id)).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

// ----------------------------------------------------------------- events ----

async fn get_events(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rt = manage(&state, &p, &slug)?;
    let r = rt.record();
    // Addon and userconfig folder names available for selection.
    let p2 = rt.published.load();
    let manifest = p2
        .files
        .iter()
        .find(|(k, _)| k.starts_with(".axs/manifest."))
        .and_then(|(_, v)| axs_model::Manifest::from_zst(v).ok());
    let (addons, userconfig) = match manifest {
        Some(m) => (
            m.addons
                .iter()
                .map(|a| axs_model::paths::file_name(a).to_owned())
                .collect::<Vec<_>>(),
            m.dirs
                .iter()
                .filter(|d| {
                    d.to_lowercase().starts_with("userconfig/") && d.matches('/').count() == 1
                })
                .map(|d| axs_model::paths::file_name(d).to_owned())
                .collect::<Vec<_>>(),
        ),
        None => (vec![], vec![]),
    };
    Ok(Json(
        serde_json::json!({"events": r.settings.events, "addons": addons, "userconfig": userconfig}),
    ))
}

async fn put_events(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Json(events): Json<Vec<EventDef>>,
) -> ApiResult<Json<serde_json::Value>> {
    let rt = manage(&state, &p, &slug)?;
    let mut names = std::collections::HashSet::new();
    for e in &events {
        if e.name.trim().is_empty() || !names.insert(e.name.to_lowercase()) {
            return Err(ApiError::bad(format!(
                "event names must be unique and non-empty ({:?})",
                e.name
            )));
        }
    }
    let r = rt.record();
    let mut s = r.settings.clone();
    s.events = events;
    let js = serde_json::to_string(&s).map_err(ApiError::internal)?;
    let id = r.id;
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE repos SET settings=?1 WHERE id=?2",
                rusqlite::params![js, id],
            )
        })
        .await?;
    state.refresh_repo(&slug).await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.events.update",
        Some(&slug),
        None,
        None,
    )
    .await;
    let job = JobManager::submit(&state, JobKind::Build, Some(id), Some(p.user_id)).await?;
    Ok(Json(serde_json::json!({"job": job})))
}

// -------------------------------------------------------------- schedules ----

#[derive(Serialize, Deserialize)]
struct ScheduleDef {
    #[serde(default)]
    id: Option<i64>,
    kind: String,
    cron: String,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    last_run: Option<i64>,
    #[serde(default)]
    next_run: Option<i64>,
}

fn yes() -> bool {
    true
}

async fn get_schedules(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<ScheduleDef>>> {
    let rt = manage(&state, &p, &slug)?;
    let id = rt.record().id;
    let rows = state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT id, kind, cron, enabled, last_run, next_run FROM schedules WHERE repo_id=?1 ORDER BY id")?;
            let r = st.query_map([id], |r| {
                Ok(ScheduleDef { id: r.get(0)?, kind: r.get(1)?, cron: r.get(2)?, enabled: r.get::<_, i64>(3)? != 0, last_run: r.get(4)?, next_run: r.get(5)? })
            })?;
            r.collect()
        })
        .await?;
    Ok(Json(rows))
}

async fn put_schedules(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Json(list): Json<Vec<ScheduleDef>>,
) -> ApiResult<StatusCode> {
    let rt = manage(&state, &p, &slug)?;
    let mut rows = vec![];
    for s in list {
        if s.kind != "build" && s.kind != "mirror" {
            return Err(ApiError::bad("schedule kind must be build or mirror"));
        }
        let next = crate::scheduler::next_run(&s.cron, now())
            .ok_or_else(|| ApiError::bad(format!("invalid cron expression {:?}", s.cron)))?;
        rows.push((s.kind, s.cron, s.enabled, next));
    }
    let id = rt.record().id;
    state
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM schedules WHERE repo_id=?1", [id])?;
            for (k, cron, en, next) in rows {
                tx.execute(
                    "INSERT INTO schedules(repo_id, kind, cron, enabled, next_run) VALUES (?1,?2,?3,?4,?5)",
                    rusqlite::params![id, k, cron, en as i64, next],
                )?;
            }
            tx.commit()
        })
        .await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.schedules.update",
        Some(&slug),
        None,
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ maintainers ----

async fn get_maintainers(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<Json<Vec<String>>> {
    let rt = manage(&state, &p, &slug)?;
    let id = rt.record().id;
    let v = state
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT u.username FROM repo_roles rr JOIN users u ON u.id=rr.user_id WHERE rr.repo_id=?1 ORDER BY u.username")?;
            let r = st.query_map([id], |r| r.get(0))?;
            r.collect()
        })
        .await?;
    Ok(Json(v))
}

async fn put_maintainers(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Json(users): Json<Vec<String>>,
) -> ApiResult<StatusCode> {
    p.require_admin()?;
    let rt = runtime(&state, &slug)?;
    let id = rt.record().id;
    let u2 = users.clone();
    state
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM repo_roles WHERE repo_id=?1", [id])?;
            for u in u2 {
                tx.execute(
                    "INSERT OR IGNORE INTO repo_roles(repo_id, user_id, role) SELECT ?1, id, 'maintainer' FROM users WHERE username=?2",
                    rusqlite::params![id, u],
                )?;
            }
            tx.commit()
        })
        .await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.maintainers.update",
        Some(&slug),
        Some(serde_json::json!(users)),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ tree ----

#[derive(Deserialize)]
struct TreeQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
struct TreeEntry {
    name: String,
    dir: bool,
    size: u64,
    modified: i64,
    addon: bool,
}

async fn tree(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Query(q): Query<TreeQuery>,
) -> ApiResult<Json<Vec<TreeEntry>>> {
    let rt = manage(&state, &p, &slug)?;
    let dir = if q.path.trim_matches('/').is_empty() {
        rt.root.clone()
    } else {
        let rel = axs_model::paths::sanitize_rel_path(&q.path)
            .ok_or_else(|| ApiError::bad("bad path"))?;
        axs_model::paths::join_rel(&rt.root, &rel)
    };
    let entries = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<TreeEntry>> {
        let mut out = vec![];
        for e in std::fs::read_dir(&dir)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(md) = e.metadata() else { continue };
            if axs_build::is_excluded_name(&name, md.is_dir()) {
                continue;
            }
            let addon = md.is_dir()
                && std::fs::read_dir(e.path())
                    .map(|rd| {
                        rd.flatten().any(|c| {
                            c.file_name()
                                .to_string_lossy()
                                .eq_ignore_ascii_case("addons")
                        })
                    })
                    .unwrap_or(false);
            out.push(TreeEntry {
                name,
                dir: md.is_dir(),
                size: if md.is_dir() { 0 } else { md.len() },
                modified: md
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                addon,
            });
        }
        out.sort_by(|a, b| {
            b.dir
                .cmp(&a.dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(|_| ApiError::not_found())?;
    Ok(Json(entries))
}

// ------------------------------------------------------------------- jobs ----

async fn build(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let rt = manage(&state, &p, &slug)?;
    let job = JobManager::submit(
        &state,
        JobKind::Build,
        Some(rt.record().id),
        Some(p.user_id),
    )
    .await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.build",
        Some(&slug),
        Some(serde_json::json!({"job": job})),
        None,
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"job": job}))))
}

async fn mirror(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let rt = manage(&state, &p, &slug)?;
    if rt.record().settings.mirror.is_none() {
        return Err(ApiError::bad("no mirror source configured"));
    }
    if rt.record().mode == RepoMode::External {
        return Err(ApiError::bad(
            "external repositories cannot be mirrored into",
        ));
    }
    let job = JobManager::submit(
        &state,
        JobKind::Mirror,
        Some(rt.record().id),
        Some(p.user_id),
    )
    .await?;
    crate::audit::record(
        &state,
        Some(&p),
        "repo.mirror",
        Some(&slug),
        Some(serde_json::json!({"job": job})),
        None,
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"job": job}))))
}

// ----------------------------------------------------------------- public ----

pub async fn public_list(State(state): State<Shared>) -> Json<Vec<RepoSummary>> {
    Json(
        state
            .repo_list()
            .iter()
            .filter(|rt| rt.record().settings.listed)
            .map(|rt| summary(&state, rt))
            .map(|mut s| {
                s.bytes_served = 0;
                s.requests = 0;
                s
            })
            .collect(),
    )
}

pub fn manifest_of(rt: &RepoRuntime) -> Option<axs_model::Manifest> {
    let p = rt.published.load();
    let idx = p
        .files
        .get(".axs/index.json")
        .and_then(|b| axs_model::RepoIndex::parse(b).ok())?;
    p.files
        .get(&format!(".axs/{}", idx.manifest))
        .and_then(|b| axs_model::Manifest::from_zst(b).ok())
}

pub async fn public_detail(
    State(state): State<Shared>,
    Path(slug): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rt = runtime(&state, &slug)?;
    if !rt.record().settings.listed {
        return Err(ApiError::not_found());
    }
    let mut s = summary(&state, &rt);
    s.bytes_served = 0;
    s.requests = 0;
    let m = manifest_of(&rt);
    Ok(Json(serde_json::json!({
        "repository": s,
        "addons": m.as_ref().map(|m| m.addons.clone()).unwrap_or_default(),
        "events": m.as_ref().map(|m| m.events.iter().map(|e| serde_json::json!({"name": e.name, "description": e.description, "addons": e.addons.len()})).collect::<Vec<_>>()).unwrap_or_default(),
        "changelogs": m.as_ref().map(|m| m.changelogs.clone()).unwrap_or_default(),
        "servers": m.as_ref().map(|m| m.servers.iter().map(|s| serde_json::json!({"name": s.name, "host": s.host, "port": s.port, "modset": s.modset})).collect::<Vec<_>>()).unwrap_or_default(),
    })))
}
