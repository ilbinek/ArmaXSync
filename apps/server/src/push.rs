//! Desktop delta push.
//!
//! 1. client creates a changeset and `PUT`s its local manifest;
//! 2. the server answers which files are unchanged, reusable (same content
//!    elsewhere), need a delta (existing `.zsync` is the signature) or a full
//!    upload, and records deletions;
//! 3. deltas are streamed as COPY/DATA records and rebuilt against the live
//!    file; full files go through `PUT …/files/<path>` or tus;
//! 4. the client commits the changeset.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use axs_model::paths::{is_under, join_rel, sanitize_rel_path};

use crate::api::{ApiError, ApiResult};
use crate::auth::Principal;
use crate::changes::{load, push_op};
use crate::publish::{staged_file, CsOp};
use crate::state::Shared;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/changesets/{id}/manifest",
            put(put_manifest).layer(DefaultBodyLimit::max(256 << 20)),
        )
        .route(
            "/changesets/{id}/delta/{*path}",
            put(put_delta).layer(DefaultBodyLimit::disable()),
        )
        .route("/repos/{slug}/zsync/{*path}", get(get_zsync))
}

/// Delta stream format.
pub const DELTA_MAGIC: &[u8; 6] = b"AXSD1\n";
pub const REC_END: u8 = 0;
pub const REC_COPY: u8 = 1;
pub const REC_DATA: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushFile {
    pub path: String,
    pub size: u64,
    pub sha1: String,
    pub blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushManifest {
    pub files: Vec<PushFile>,
    /// Folders the push covers; live content inside them that is not in
    /// `files` is deleted. Empty = whole repository.
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub delete_missing: bool,
    #[serde(default)]
    pub dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushPlan {
    pub unchanged: usize,
    pub reused: Vec<String>,
    /// Paths to send as deltas (the server has an older version).
    pub delta: Vec<String>,
    /// Paths to upload in full.
    pub full: Vec<String>,
    pub deleted: Vec<String>,
}

async fn put_manifest(
    State(state): State<Shared>,
    p: Principal,
    Path(id): Path<i64>,
    Json(m): Json<PushManifest>,
) -> ApiResult<Json<PushPlan>> {
    let (mut cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    let rt = state.repo(&cs.repo).ok_or_else(ApiError::not_found)?;
    let live = crate::repos_api::manifest_of(&rt)
        .map(|m| m.files)
        .unwrap_or_default();
    let by_path: HashMap<&str, &axs_model::FileEntry> =
        live.iter().map(|f| (f.path.as_str(), f)).collect();
    let by_b3: HashMap<&str, &axs_model::FileEntry> = live
        .iter()
        .filter(|f| !f.blake3.is_empty())
        .map(|f| (f.blake3.as_str(), f))
        .collect();
    let scope: Vec<String> = m
        .scope
        .iter()
        .filter_map(|s| sanitize_rel_path(s))
        .collect();
    let in_scope = |p: &str| scope.is_empty() || scope.iter().any(|s| is_under(p, s, false));
    let mut plan = PushPlan::default();
    let mut wanted = std::collections::HashSet::new();
    for f in &m.files {
        let rel = sanitize_rel_path(&f.path)
            .ok_or_else(|| ApiError::bad(format!("invalid path {:?}", f.path)))?;
        if !in_scope(&rel) {
            return Err(ApiError::bad(format!("{rel} is outside the push scope")));
        }
        wanted.insert(rel.clone());
        match by_path.get(rel.as_str()) {
            Some(l) if l.blake3 == f.blake3 && l.size == f.size => plan.unchanged += 1,
            existing => {
                if let Some(src) = by_b3.get(f.blake3.as_str()) {
                    // Same content elsewhere in the repository: stage a link.
                    let from = join_rel(&rt.root, &src.path);
                    let dst = staged_file(&state, &cs.repo, id, &rel);
                    if let Some(d) = dst.parent() {
                        std::fs::create_dir_all(d).map_err(ApiError::internal)?;
                    }
                    if std::fs::hard_link(&from, &dst).is_err() {
                        std::fs::copy(&from, &dst).map_err(ApiError::internal)?;
                    }
                    push_op(
                        &mut cs.ops,
                        CsOp::Put {
                            path: rel.clone(),
                            size: f.size,
                        },
                    );
                    plan.reused.push(rel);
                } else if existing.map(|e| e.zsync && e.size > 0).unwrap_or(false)
                    && f.size > 64 * 1024
                {
                    plan.delta.push(rel);
                } else {
                    plan.full.push(rel);
                }
            }
        }
    }
    for d in &m.dirs {
        if let Some(d) = sanitize_rel_path(d) {
            if in_scope(&d) {
                push_op(&mut cs.ops, CsOp::Mkdir { path: d });
            }
        }
    }
    if m.delete_missing {
        for l in &live {
            if in_scope(&l.path) && !wanted.contains(&l.path) {
                push_op(
                    &mut cs.ops,
                    CsOp::Delete {
                        path: l.path.clone(),
                    },
                );
                plan.deleted.push(l.path.clone());
            }
        }
    }
    let js = serde_json::to_string(&cs.ops).map_err(ApiError::internal)?;
    let manifest_js = serde_json::to_string(&m.files).map_err(ApiError::internal)?;
    let stage = crate::publish::staging_dir(&state, &cs.repo, id);
    std::fs::create_dir_all(&stage).map_err(ApiError::internal)?;
    std::fs::write(stage.join("push-manifest.json"), manifest_js).map_err(ApiError::internal)?;
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET ops=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![js, crate::db::now(), id],
            )
        })
        .await?;
    Ok(Json(plan))
}

/// The live `.zsync` of a file (signature for delta pushes).
async fn get_zsync(
    State(state): State<Shared>,
    p: Principal,
    Path((slug, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    let rt = state.repo(&slug).ok_or_else(ApiError::not_found)?;
    p.require_manage(rt.record().id, &slug)?;
    let rel = sanitize_rel_path(&path).ok_or_else(|| ApiError::bad("bad path"))?;
    let z = axs_build::sidecar(&join_rel(&rt.root, &rel), ".zsync");
    let data = tokio::fs::read(&z)
        .await
        .map_err(|_| ApiError::not_found())?;
    let mut r = data.into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    Ok(r)
}

/// Rebuild a file from a delta stream against the live version.
async fn put_delta(
    State(state): State<Shared>,
    p: Principal,
    Path((id, path)): Path<(i64, String)>,
    req: Request,
) -> ApiResult<Json<serde_json::Value>> {
    let (mut cs, _) = load(&state, &p, id).await?;
    if cs.status != "draft" {
        return Err(ApiError::conflict(format!("changeset is {}", cs.status)));
    }
    let rel = sanitize_rel_path(&path).ok_or_else(|| ApiError::bad("bad path"))?;
    let rt = state.repo(&cs.repo).ok_or_else(ApiError::not_found)?;
    let base = join_rel(&rt.root, &rel);
    let dst = staged_file(&state, &cs.repo, id, &rel);
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d).map_err(ApiError::internal)?;
    }
    // Spool the request body (deltas are small compared to files).
    let spool = axs_build::sidecar(&dst, ".delta.axs-part");
    {
        let mut f = std::fs::File::create(&spool).map_err(ApiError::internal)?;
        let mut s = req.into_body().into_data_stream();
        let mut n = 0u64;
        while let Some(c) = s.next().await {
            let c = c.map_err(|e| ApiError::bad(e.to_string()))?;
            n += c.len() as u64;
            if n > state.cfg.max_upload_size {
                return Err(ApiError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "delta too large",
                ));
            }
            f.write_all(&c).map_err(ApiError::internal)?;
        }
    }
    let (sp, b, d) = (spool.clone(), base.clone(), dst.clone());
    let size = tokio::task::spawn_blocking(move || apply_delta(&sp, &b, &d))
        .await
        .map_err(ApiError::internal)?;
    let _ = std::fs::remove_file(&spool);
    let size = size.map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    push_op(
        &mut cs.ops,
        CsOp::Put {
            path: rel.clone(),
            size,
        },
    );
    let js = serde_json::to_string(&cs.ops).map_err(ApiError::internal)?;
    state
        .db
        .call(move |c| {
            c.execute(
                "UPDATE changesets SET ops=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![js, crate::db::now(), id],
            )
        })
        .await?;
    Ok(Json(serde_json::json!({"path": rel, "size": size})))
}

fn read_exact<R: Read>(r: &mut R, n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    r.read_exact(&mut b)
        .map_err(|e| format!("truncated delta: {e}"))?;
    Ok(b)
}

/// Apply a delta stream (see module docs); verifies BLAKE3 of the result.
pub fn apply_delta(
    delta: &std::path::Path,
    base: &std::path::Path,
    out: &std::path::Path,
) -> Result<u64, String> {
    let mut d = std::io::BufReader::new(std::fs::File::open(delta).map_err(|e| e.to_string())?);
    if read_exact(&mut d, 6)? != DELTA_MAGIC {
        return Err("not an ArmaXSync delta".into());
    }
    let size = u64::from_be_bytes(read_exact(&mut d, 8)?.try_into().unwrap());
    let bs = u32::from_be_bytes(read_exact(&mut d, 4)?.try_into().unwrap()) as u64;
    let want: [u8; 32] = read_exact(&mut d, 32)?.try_into().unwrap();
    if bs == 0 || bs > 1 << 24 {
        return Err("bad block size".into());
    }
    let mut src = std::fs::File::open(base).ok();
    let src_len = src
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len())
        .unwrap_or(0);
    let tmp = axs_build::sidecar(out, ".axs-part");
    let mut o = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
    let mut hasher = blake3::Hasher::new();
    let mut written = 0u64;
    let mut buf = vec![0u8; bs as usize];
    loop {
        let tag = read_exact(&mut d, 1)?[0];
        match tag {
            REC_END => break,
            REC_COPY => {
                let idx = u64::from_be_bytes(read_exact(&mut d, 8)?.try_into().unwrap());
                let count = u32::from_be_bytes(read_exact(&mut d, 4)?.try_into().unwrap()) as u64;
                let f = src.as_mut().ok_or("delta references a missing base file")?;
                for k in 0..count {
                    let off = (idx + k) * bs;
                    if off >= src_len {
                        return Err("COPY beyond base file".into());
                    }
                    let n = bs.min(src_len - off).min(size - written) as usize;
                    f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
                    f.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
                    o.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    hasher.update(&buf[..n]);
                    written += n as u64;
                }
            }
            REC_DATA => {
                let len = u32::from_be_bytes(read_exact(&mut d, 4)?.try_into().unwrap()) as usize;
                if written + len as u64 > size {
                    return Err("DATA beyond target size".into());
                }
                let data = read_exact(&mut d, len)?;
                o.write_all(&data).map_err(|e| e.to_string())?;
                hasher.update(&data);
                written += len as u64;
            }
            t => return Err(format!("bad record {t}")),
        }
        if written > size {
            return Err("delta overflows target size".into());
        }
    }
    o.flush().map_err(|e| e.to_string())?;
    drop(o);
    if written != size || hasher.finalize().as_bytes() != &want {
        let _ = std::fs::remove_file(&tmp);
        return Err("reconstructed file does not match (checksum)".into());
    }
    axs_build::rename_replace(&tmp, out).map_err(|e| e.to_string())?;
    Ok(size)
}
