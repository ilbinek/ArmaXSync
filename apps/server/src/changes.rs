//! Changesets (staged edits), direct file uploads, resumable tus uploads and
//! zip extraction.

use std::io::{Read, Write};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures::StreamExt;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use axs_model::paths::{join_rel, sanitize_rel_path};

use crate::api::{header_str, ApiError, ApiResult};
use crate::auth::Principal;
use crate::db::now;
use crate::jobs::{JobKind, JobManager};
use crate::publish::{staged_file, staging_dir, CsOp};
use crate::state::{RepoMode, Shared};

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/repos/{slug}/changesets", get(list).post(create))
        .route("/changesets/{id}", get(get_one).delete(discard))
        .route("/changesets/{id}/ops", post(add_ops))
        .route(
            "/changesets/{id}/files/{*path}",
            put(put_file).layer(DefaultBodyLimit::disable()),
        )
        .route("/changesets/{id}/extract", post(extract_zip))
        .route("/changesets/{id}/commit", post(commit))
        .route("/uploads", post(tus_create).options(tus_options))
        .route(
            "/uploads/{id}",
            axum::routing::head(tus_head)
                .patch(tus_patch)
                .delete(tus_delete)
                .layer(DefaultBodyLimit::disable()),
        )
}

#[derive(Serialize)]
pub struct ChangesetOut {
    pub id: i64,
    pub repo: String,
    pub status: String,
    pub base_revision: Option<i64>,
    pub note: String,
    pub ops: Vec<CsOp>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Load a changeset and check the caller may edit it.
pub async fn load(state: &Shared, p: &Principal, id: i64) -> ApiResult<(ChangesetOut, i64)> {
    let row = state
        .db
        .call(move |c| {
            c.query_row(
                "SELECT c.id, r.slug, c.status, c.base_revision, c.note, c.ops, c.created_at, c.updated_at, c.repo_id FROM changesets c JOIN repos r ON r.id=c.repo_id WHERE c.id=?1",
                [id],
                |r| {
                    Ok((
                        ChangesetOut {
                            id: r.get(0)?,
                            repo: r.get(1)?,
                            status: r.get(2)?,
                            base_revision: r.get(3)?,
                            note: r.get(4)?,
                            ops: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
                            created_at: r.get(6)?,
                            updated_at: r.get(7)?,
                        },
                        r.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()
        })
        .await?
        .ok_or_else(ApiError::not_found)?;
    p.require_manage(row.1, &row.0.repo)?;
    Ok(row)
}

async fn save_ops(state: &Shared, id: i64, ops: &[CsOp]) -> ApiResult<()> {
    let js = serde_json::to_string(ops).map_err(ApiError::internal)?;
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET ops=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![js, now(), id],
            )
        })
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
}

async fn list(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rt = state.repo(&slug).ok_or_else(ApiError::not_found)?;
    p.require_manage(rt.record().id, &slug)?;
    let id = rt.record().id;
    let rows = state
        .db
        .call(move |c| {
            let mut st = c.prepare(
                "SELECT id, status, base_revision, note, created_at, updated_at, json_array_length(ops) FROM changesets WHERE repo_id=?1 AND (?2 IS NULL OR status=?2) ORDER BY id DESC LIMIT 100",
            )?;
            let r = st.query_map(rusqlite::params![id, q.status], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?, "status": r.get::<_, String>(1)?, "base_revision": r.get::<_, Option<i64>>(2)?,
                    "note": r.get::<_, String>(3)?, "created_at": r.get::<_, i64>(4)?, "updated_at": r.get::<_, i64>(5)?, "ops": r.get::<_, i64>(6)?,
                }))
            })?;
            r.collect()
        })
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Default)]
pub struct NewChangeset {
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub base_revision: Option<i64>,
}

pub async fn create_changeset(
    state: &Shared,
    p: &Principal,
    slug: &str,
    n: NewChangeset,
) -> ApiResult<i64> {
    let rt = state.repo(slug).ok_or_else(ApiError::not_found)?;
    let rec = rt.record();
    p.require_manage(rec.id, slug)?;
    if rec.mode == RepoMode::External {
        return Err(ApiError::bad(
            "external repositories are edited on disk; use build instead",
        ));
    }
    let (rid, uid) = (rec.id, p.user_id);
    let base = n
        .base_revision
        .unwrap_or(rt.published.load().revision as i64);
    let id = state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO changesets(repo_id, status, base_revision, note, created_by, created_at, updated_at) VALUES (?1,'draft',?2,?3,?4,?5,?5)",
                rusqlite::params![rid, base, n.note, uid, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    std::fs::create_dir_all(staging_dir(state, slug, id).join("files"))
        .map_err(ApiError::internal)?;
    Ok(id)
}

async fn create(
    State(state): State<Shared>,
    p: Principal,
    Path(slug): Path<String>,
    body: Option<Json<NewChangeset>>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let id = create_changeset(&state, &p, &slug, body.map(|b| b.0).unwrap_or_default()).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

async fn get_one(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChangesetOut>> {
    Ok(Json(load(&state, &p, id).await?.0))
}

async fn discard(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let (cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET status='discarded', updated_at=?1 WHERE id=?2",
                rusqlite::params![now(), id],
            )
        })
        .await?;
    let _ = std::fs::remove_dir_all(staging_dir(&state, &cs.repo, id));
    Ok(StatusCode::NO_CONTENT)
}

fn clean_path(p: &str) -> ApiResult<String> {
    let rel = sanitize_rel_path(p).ok_or_else(|| ApiError::bad(format!("invalid path {p:?}")))?;
    for seg in rel.split('/') {
        if axs_build::is_excluded_name(seg, false) && seg != rel.rsplit('/').next().unwrap_or("") {
            return Err(ApiError::bad(format!("reserved name in {rel:?}")));
        }
    }
    let name = axs_model::paths::file_name(&rel);
    if axs_build::is_excluded_name(name, false)
        || rel.starts_with(".a3s")
        || rel.starts_with(".axs")
    {
        return Err(ApiError::bad(format!(
            "{rel:?} uses a reserved name (.a3s, .axs, .zsync, .pbo.zip)"
        )));
    }
    Ok(rel)
}

/// Merge an op into a list (later ops supersede earlier ones on the same path).
pub fn push_op(ops: &mut Vec<CsOp>, op: CsOp) {
    let path = op.path().to_owned();
    match &op {
        CsOp::Delete { .. } => ops.retain(|o| !axs_model::paths::is_under(o.path(), &path, false)),
        _ => ops.retain(|o| !(o.path() == path && !matches!(o, CsOp::Delete { .. }))),
    }
    ops.push(op);
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum OpReq {
    Delete {
        path: String,
    },
    Mkdir {
        path: String,
    },
    /// Move a finished tus upload into the changeset.
    Put {
        path: String,
        upload: String,
    },
    /// Copy an existing live file/folder (stages a copy).
    Copy {
        from: String,
        to: String,
    },
    /// Move/rename an existing live file/folder.
    Move {
        from: String,
        to: String,
    },
}

async fn add_ops(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
    Json(reqs): Json<Vec<OpReq>>,
) -> ApiResult<Json<ChangesetOut>> {
    let (mut cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    let rt = state.repo(&cs.repo).ok_or_else(ApiError::not_found)?;
    for r in reqs {
        match r {
            OpReq::Delete { path } => push_op(
                &mut cs.ops,
                CsOp::Delete {
                    path: clean_path(&path)?,
                },
            ),
            OpReq::Mkdir { path } => push_op(
                &mut cs.ops,
                CsOp::Mkdir {
                    path: clean_path(&path)?,
                },
            ),
            OpReq::Put { path, upload } => {
                let rel = clean_path(&path)?;
                let (len, off) = upload_state(&state, &upload)
                    .await?
                    .ok_or_else(|| ApiError::bad("unknown upload"))?;
                if len != off {
                    return Err(ApiError::bad("upload is not complete"));
                }
                let dst = staged_file(&state, &cs.repo, id, &rel);
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d).map_err(ApiError::internal)?;
                }
                std::fs::rename(state.cfg.uploads_dir().join(&upload), &dst)
                    .or_else(|_| {
                        std::fs::copy(state.cfg.uploads_dir().join(&upload), &dst).map(|_| ())
                    })
                    .map_err(ApiError::internal)?;
                let u2 = upload.clone();
                state
                    .db
                    .call(move |c| c.execute("DELETE FROM uploads WHERE id=?1", [u2]))
                    .await?;
                push_op(
                    &mut cs.ops,
                    CsOp::Put {
                        path: rel,
                        size: len,
                    },
                );
            }
            OpReq::Copy { from, to } => stage_copy(
                &state,
                &rt.root,
                &cs.repo,
                id,
                &mut cs.ops,
                &from,
                &to,
                false,
            )?,
            OpReq::Move { from, to } => stage_copy(
                &state,
                &rt.root,
                &cs.repo,
                id,
                &mut cs.ops,
                &from,
                &to,
                true,
            )?,
        }
    }
    save_ops(&state, id, &cs.ops).await?;
    Ok(Json(cs))
}

#[allow(clippy::too_many_arguments)]
fn stage_copy(
    state: &Shared,
    root: &std::path::Path,
    slug: &str,
    id: i64,
    ops: &mut Vec<CsOp>,
    from: &str,
    to: &str,
    mv: bool,
) -> ApiResult<()> {
    let (from, to) = (clean_path(from)?, clean_path(to)?);
    if axs_model::paths::is_under(&to, &from, false) {
        return Err(ApiError::bad("cannot copy a folder into itself"));
    }
    let src = join_rel(root, &from);
    let md =
        std::fs::metadata(&src).map_err(|_| ApiError::bad(format!("{from} does not exist")))?;
    let mut files = vec![];
    if md.is_dir() {
        for e in walk(&src) {
            let rel = e
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let name = axs_model::paths::file_name(&rel).to_owned();
            if !axs_build::is_excluded_name(&name, false) {
                files.push((e, format!("{to}/{rel}")));
            }
        }
        push_op(ops, CsOp::Mkdir { path: to.clone() });
    } else {
        files.push((src.clone(), to.clone()));
    }
    for (abs, rel) in files {
        let dst = staged_file(state, slug, id, &rel);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(ApiError::internal)?;
        }
        // Hard link when possible (no extra space), else copy.
        if std::fs::hard_link(&abs, &dst).is_err() {
            std::fs::copy(&abs, &dst).map_err(ApiError::internal)?;
        }
        let size = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        push_op(ops, CsOp::Put { path: rel, size });
    }
    if mv {
        push_op(ops, CsOp::Delete { path: from });
    }
    Ok(())
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Stream a request body into a file (returns bytes written).
async fn body_to_file(
    body: Body,
    file: &std::path::Path,
    append: bool,
    max: u64,
) -> ApiResult<u64> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(file)
        .map_err(ApiError::internal)?;
    let mut stream = body.into_data_stream();
    let mut n = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::bad(format!("upload interrupted: {e}")))?;
        n += chunk.len() as u64;
        if n > max {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "upload too large",
            ));
        }
        f.write_all(&chunk).map_err(ApiError::internal)?;
    }
    f.flush().map_err(ApiError::internal)?;
    Ok(n)
}

/// `PUT /changesets/{id}/files/{path}` with the raw file as body.
async fn put_file(
    State(state): State<Shared>,
    p: Principal,
    Path((id, path)): Path<(i64, String)>,
    req: Request,
) -> ApiResult<Json<serde_json::Value>> {
    let (mut cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    let rel = clean_path(&path)?;
    let dst = staged_file(&state, &cs.repo, id, &rel);
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d).map_err(ApiError::internal)?;
    }
    let tmp = axs_build::sidecar(&dst, ".axs-part");
    let size = body_to_file(req.into_body(), &tmp, false, state.cfg.max_upload_size).await?;
    axs_build::rename_replace(&tmp, &dst).map_err(ApiError::internal)?;
    push_op(
        &mut cs.ops,
        CsOp::Put {
            path: rel.clone(),
            size,
        },
    );
    save_ops(&state, id, &cs.ops).await?;
    Ok(Json(serde_json::json!({"path": rel, "size": size})))
}

#[derive(Deserialize)]
struct ExtractReq {
    /// Finished tus upload containing a zip archive.
    upload: String,
    /// Folder inside the repository to extract into ("" = root).
    #[serde(default)]
    dest: String,
}

/// Extract an uploaded zip into the changeset (zip-bomb and traversal safe).
async fn extract_zip(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
    Json(r): Json<ExtractReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let (mut cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    let (len, off) = upload_state(&state, &r.upload)
        .await?
        .ok_or_else(|| ApiError::bad("unknown upload"))?;
    if len != off {
        return Err(ApiError::bad("upload is not complete"));
    }
    let dest = if r.dest.trim_matches('/').is_empty() {
        String::new()
    } else {
        clean_path(&r.dest)?
    };
    let zip_path = state.cfg.uploads_dir().join(&r.upload);
    let stage = staging_dir(&state, &cs.repo, id).join("files");
    let max_total = state.cfg.max_upload_size.saturating_mul(4);
    let dest2 = dest.clone();
    let extracted = tokio::task::spawn_blocking(move || -> Result<Vec<(String, u64)>, String> {
        let f = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut za = zip::ZipArchive::new(f).map_err(|e| format!("not a zip file: {e}"))?;
        let mut total = 0u64;
        let mut out = vec![];
        for i in 0..za.len() {
            let mut e = za.by_index(i).map_err(|e| e.to_string())?;
            if e.is_dir() {
                continue;
            }
            let Some(name) = e
                .enclosed_name()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
            else {
                return Err(format!("unsafe path in archive: {}", e.name()));
            };
            let rel = if dest2.is_empty() {
                name
            } else {
                format!("{dest2}/{name}")
            };
            let rel = sanitize_rel_path(&rel).ok_or_else(|| format!("invalid path {rel}"))?;
            let fname = axs_model::paths::file_name(&rel);
            if axs_build::is_excluded_name(fname, false) {
                continue;
            }
            if e.compressed_size() > 0 && e.size() / e.compressed_size().max(1) > 1000 {
                return Err(format!("suspicious compression ratio for {rel}"));
            }
            total += e.size();
            if total > max_total {
                return Err("archive expands beyond the upload limit".into());
            }
            let dst = join_rel(&stage, &rel);
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            let mut f = std::fs::File::create(&dst).map_err(|e| e.to_string())?;
            let esize = e.size();
            let n =
                std::io::copy(&mut (&mut e).take(esize + 1), &mut f).map_err(|e| e.to_string())?;
            if n > esize {
                return Err(format!("{rel}: size mismatch"));
            }
            out.push((rel, n));
        }
        Ok(out)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::bad)?;
    for (rel, size) in &extracted {
        push_op(
            &mut cs.ops,
            CsOp::Put {
                path: rel.clone(),
                size: *size,
            },
        );
    }
    save_ops(&state, id, &cs.ops).await?;
    let _ = std::fs::remove_file(state.cfg.uploads_dir().join(&r.upload));
    let u = r.upload.clone();
    state
        .db
        .call(move |c| c.execute("DELETE FROM uploads WHERE id=?1", [u]))
        .await?;
    Ok(Json(
        serde_json::json!({"files": extracted.len(), "dest": dest}),
    ))
}

#[derive(Deserialize, Default)]
struct CommitReq {
    /// Fail with 409 when the repository moved past this revision.
    expected_revision: Option<u32>,
    #[serde(default)]
    force: bool,
}

async fn commit(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
    body: Option<Json<CommitReq>>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let (cs, repo_id) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    if cs.ops.is_empty() {
        return Err(ApiError::bad("changeset is empty"));
    }
    let rt = state.repo(&cs.repo).ok_or_else(ApiError::not_found)?;
    let current = rt.published.load().revision;
    if let Some(exp) = req.expected_revision {
        if exp != current && !req.force {
            return Err(ApiError::conflict(format!(
                "repository is at revision {current}, expected {exp} (use force to publish anyway)"
            )));
        }
    }
    let job = JobManager::submit(
        &state,
        JobKind::Publish { changeset: id },
        Some(repo_id),
        Some(p.user_id),
    )
    .await?;
    crate::audit::record(
        &state,
        Some(&p),
        "changeset.commit",
        Some(&cs.repo),
        Some(serde_json::json!({"changeset": id, "ops": cs.ops.len(), "job": job})),
        None,
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"job": job}))))
}

// ----------------------------------------------------------- tus uploads ----

const TUS_VERSION: &str = "1.0.0";

fn tus_headers(h: &mut HeaderMap) {
    h.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
}

async fn tus_options(State(state): State<Shared>) -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    let h = r.headers_mut();
    tus_headers(h);
    h.insert("Tus-Version", HeaderValue::from_static(TUS_VERSION));
    h.insert(
        "Tus-Extension",
        HeaderValue::from_static("creation,termination"),
    );
    h.insert("Tus-Max-Size", HeaderValue::from(state.cfg.max_upload_size));
    r
}

async fn upload_state(state: &Shared, id: &str) -> ApiResult<Option<(u64, u64)>> {
    let id = id.to_owned();
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(None);
    }
    Ok(state
        .db
        .call(move |c| {
            c.query_row(
                "SELECT length, offset FROM uploads WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )
            .optional()
        })
        .await?)
}

async fn tus_create(
    State(state): State<Shared>,
    p: Principal,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !p.is_admin()
        && p.maintained.is_empty()
        && p.scopes
            .as_ref()
            .map(|s| !s.iter().any(|x| x.starts_with("push:")))
            .unwrap_or(true)
    {
        return Err(ApiError::forbidden());
    }
    let len: u64 = header_str(&headers, "upload-length")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad("Upload-Length required"))?;
    if len > state.cfg.max_upload_size {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "upload too large",
        ));
    }
    let meta = header_str(&headers, "upload-metadata")
        .unwrap_or("")
        .to_owned();
    let id = crate::crypto::random_token(18);
    std::fs::create_dir_all(state.cfg.uploads_dir()).map_err(ApiError::internal)?;
    std::fs::File::create(state.cfg.uploads_dir().join(&id)).map_err(ApiError::internal)?;
    let (i2, uid) = (id.clone(), p.user_id);
    state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO uploads(id, user_id, length, offset, metadata, created_at) VALUES (?1,?2,?3,0,?4,?5)",
                rusqlite::params![i2, uid, len as i64, meta, now()],
            )
        })
        .await?;
    let mut r = StatusCode::CREATED.into_response();
    tus_headers(r.headers_mut());
    r.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/api/v1/uploads/{id}")).map_err(ApiError::internal)?,
    );
    r.headers_mut()
        .insert("Upload-Offset", HeaderValue::from(0));
    Ok(r)
}

async fn tus_head(
    State(state): State<Shared>,
    _p: Principal,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let (len, off) = upload_state(&state, &id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let mut r = StatusCode::OK.into_response();
    tus_headers(r.headers_mut());
    r.headers_mut()
        .insert("Upload-Offset", HeaderValue::from(off));
    r.headers_mut()
        .insert("Upload-Length", HeaderValue::from(len));
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(r)
}

async fn tus_patch(
    State(state): State<Shared>,
    _p: Principal,
    Path(id): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    let (len, off) = upload_state(&state, &id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let sent: u64 = header_str(req.headers(), "upload-offset")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad("Upload-Offset required"))?;
    if sent != off {
        return Err(ApiError::conflict(format!(
            "offset mismatch (server has {off})"
        )));
    }
    let path = state.cfg.uploads_dir().join(&id);
    // Truncate to the recorded offset (drops bytes of an interrupted PATCH).
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_len(off))
        .map_err(ApiError::internal)?;
    let n = body_to_file(req.into_body(), &path, true, len - off).await;
    let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(off);
    let new_off = written.min(len);
    let i2 = id.clone();
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE uploads SET offset=?1 WHERE id=?2",
                rusqlite::params![new_off as i64, i2],
            )
        })
        .await?;
    n?;
    let mut r = StatusCode::NO_CONTENT.into_response();
    tus_headers(r.headers_mut());
    r.headers_mut()
        .insert("Upload-Offset", HeaderValue::from(new_off));
    Ok(r)
}

async fn tus_delete(
    State(state): State<Shared>,
    _p: Principal,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if upload_state(&state, &id).await?.is_none() {
        return Err(ApiError::not_found());
    }
    let _ = std::fs::remove_file(state.cfg.uploads_dir().join(&id));
    state
        .db
        .call(move |c| c.execute("DELETE FROM uploads WHERE id=?1", [id]))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
