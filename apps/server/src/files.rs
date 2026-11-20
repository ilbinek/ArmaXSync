//! Repository file serving (`/r/<slug>/<path>`), compatible with legacy
//! Arma3Sync clients:
//! * raw UTF-8 and percent-encoded paths, duplicate slashes;
//! * single byte ranges, clamped to the file length, exact Content-Length;
//! * never compressed; HTTP Basic auth for private repositories.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use axs_model::paths::sanitize_rel_path;

use crate::state::{AppState, RepoRuntime, Visibility};
use crate::stats::collect::{self, Download, Kind, Req};

/// Split `/r/<slug>/<rest>` (raw, undecoded path).
pub fn split_repo_path(raw: &str) -> Option<(String, String)> {
    let rest = raw.strip_prefix("/r/")?;
    let (slug, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    Some((percent_decode(slug)?, path.to_owned()))
}

/// Percent-decode a raw path (which may already contain UTF-8 bytes).
pub fn percent_decode(raw: &str) -> Option<String> {
    let bytes: Vec<u8> = percent_encoding::percent_decode(raw.as_bytes()).collect();
    String::from_utf8(bytes).ok()
}

fn unauthorized(slug: &str) -> Response {
    let mut r = (
        StatusCode::UNAUTHORIZED,
        "login or password wrong or missing",
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("Basic realm=\"{slug}\", charset=\"UTF-8\"")) {
        r.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
    r
}

/// Check HTTP Basic credentials against the repository's credentials.
pub fn authorized(rt: &RepoRuntime, headers: &HeaderMap) -> bool {
    if rt.record().settings.visibility == Visibility::Public {
        return true;
    }
    let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(b64) = v
        .strip_prefix("Basic ")
        .or_else(|| v.strip_prefix("basic "))
    else {
        return false;
    };
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
        return false;
    };
    let raw = String::from_utf8_lossy(&raw);
    let Some((login, pw)) = raw.split_once(':') else {
        return false;
    };
    rt.credentials
        .load()
        .iter()
        .any(|c| crate::crypto::ct_eq(&c.login, login) & crate::crypto::ct_eq(&c.password, pw))
}

#[derive(Debug, PartialEq, Eq)]
enum RangeReq {
    Full,
    Partial(u64, u64), // inclusive
    Unsatisfiable,
}

/// Parse a single `Range: bytes=…` header against a length.
fn parse_range(h: Option<&str>, len: u64) -> RangeReq {
    let Some(h) = h else { return RangeReq::Full };
    let Some(spec) = h.trim().strip_prefix("bytes=") else {
        return RangeReq::Full;
    };
    if spec.contains(',') {
        return RangeReq::Full; // multi-range: serve everything
    }
    let Some((a, b)) = spec.split_once('-') else {
        return RangeReq::Full;
    };
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        let Ok(n) = b.parse::<u64>() else {
            return RangeReq::Full;
        };
        if n == 0 || len == 0 {
            return RangeReq::Unsatisfiable;
        }
        return RangeReq::Partial(len.saturating_sub(n), len - 1);
    }
    let Ok(start) = a.parse::<u64>() else {
        return RangeReq::Full;
    };
    if start >= len {
        return RangeReq::Unsatisfiable;
    }
    let end = if b.is_empty() {
        len - 1
    } else {
        b.parse::<u64>().map(|e| e.min(len - 1)).unwrap_or(len - 1)
    };
    if end < start {
        return RangeReq::Full;
    }
    RangeReq::Partial(start, end)
}

fn base_headers(len: u64) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    h
}

/// Serve an in-memory metadata file with range support.
fn serve_bytes(data: Bytes, range: Option<&str>, head: bool) -> Response {
    let len = data.len() as u64;
    let mut h = base_headers(len);
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    match parse_range(range, len) {
        RangeReq::Full => (
            StatusCode::OK,
            h,
            if head {
                Body::empty()
            } else {
                Body::from(data)
            },
        )
            .into_response(),
        RangeReq::Partial(s, e) => {
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(e - s + 1));
            h.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {s}-{e}/{len}")).unwrap(),
            );
            let body = if head {
                Body::empty()
            } else {
                Body::from(data.slice(s as usize..=e as usize))
            };
            (StatusCode::PARTIAL_CONTENT, h, body).into_response()
        }
        RangeReq::Unsatisfiable => unsatisfiable(len),
    }
}

fn unsatisfiable(len: u64) -> Response {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes */{len}")).unwrap(),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(0));
    (StatusCode::RANGE_NOT_SATISFIABLE, h).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// Metadata paths served from memory; other `.axs` content is private.
fn metadata_key(rel: &str) -> Option<bool> {
    let lower = rel.to_ascii_lowercase();
    if lower.starts_with(".a3s/") || lower == ".a3s" {
        return Some(true);
    }
    if lower.starts_with(".axs/") || lower == ".axs" {
        return Some(true);
    }
    None
}

pub async fn serve(
    state: Arc<AppState>,
    slug: &str,
    raw_rest: &str,
    method: &Method,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
) -> Response {
    let Some(rt) = state.repo(slug) else {
        return not_found();
    };
    rt.requests.fetch_add(1, Ordering::Relaxed);
    let ip = crate::auth::client_ip(&state, peer, headers);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let mut req = Req::new(
        state.stats.clone(),
        rt.record().id,
        ua,
        ip,
        method == Method::HEAD,
    );
    let resp = serve_inner(&state, &rt, slug, raw_rest, method, headers, &mut req).await;
    req.finish(resp.status().as_u16());
    resp
}

async fn serve_inner(
    state: &Arc<AppState>,
    rt: &Arc<RepoRuntime>,
    slug: &str,
    raw_rest: &str,
    method: &Method,
    headers: &HeaderMap,
    req: &mut Req,
) -> Response {
    if !authorized(rt, headers) {
        return unauthorized(slug);
    }
    let head = method == Method::HEAD;
    let Some(decoded) = percent_decode(raw_rest) else {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    };
    let Some(rel) = sanitize_rel_path(&decoded) else {
        if decoded.trim_matches('/').is_empty() {
            // Browsers: send them to the landing page.
            let mut r = StatusCode::FOUND.into_response();
            r.headers_mut().insert(
                header::LOCATION,
                HeaderValue::from_str(&format!("/repo/{slug}"))
                    .unwrap_or(HeaderValue::from_static("/")),
            );
            return r;
        }
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    };
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    req.range = range.is_some();

    if metadata_key(&rel).is_some() {
        req.kind = Kind::Meta;
        req.check = collect::is_check(&rel);
        let p = rt.published.load();
        return match p.files.get(&rel) {
            Some(b) => {
                let r = serve_bytes(b.clone(), range, head);
                if !head {
                    req.meta_bytes = r
                        .headers()
                        .get(header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
                r
            }
            None => not_found(),
        };
    }
    let sidecar = collect::is_sidecar(&rel);
    req.kind = if sidecar {
        Kind::Sidecar
    } else {
        Kind::Content
    };
    let name = axs_model::paths::file_name(&rel);
    if name.ends_with(".axs-tmp") || name.ends_with(".axs-part") {
        return not_found();
    }

    // Open under the commit read-lock so a publish never swaps a file between
    // our existence check and open.
    let path = axs_model::paths::join_rel(&rt.root, &rel);
    let opened = {
        let _g = rt.commit_lock.read().await;
        match tokio::fs::File::open(&path).await {
            Ok(f) => match f.metadata().await {
                Ok(m) if m.is_file() => Some((f, m)),
                _ => None,
            },
            Err(_) => None,
        }
    };
    let Some((mut file, md)) = opened else {
        return not_found();
    };
    let len = md.len();
    let mut h = base_headers(len);
    if let Ok(mt) = md.modified() {
        let dt: chrono::DateTime<chrono::Utc> = mt.into();
        if let Ok(v) = HeaderValue::from_str(&dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()) {
            h.insert(header::LAST_MODIFIED, v);
        }
        let secs = mt
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        if let Ok(v) = HeaderValue::from_str(&format!("\"{len:x}-{secs:x}\"")) {
            h.insert(header::ETAG, v);
        }
    }
    let (status, start, end) = match parse_range(range, len) {
        RangeReq::Full => (StatusCode::OK, 0, len),
        RangeReq::Partial(s, e) => {
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(e - s + 1));
            h.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {s}-{e}/{len}")).unwrap(),
            );
            (StatusCode::PARTIAL_CONTENT, s, e + 1)
        }
        RangeReq::Unsatisfiable => return unsatisfiable(len),
    };
    if head {
        return (status, h, Body::empty()).into_response();
    }
    let full = start == 0 && end == len;
    if start == 0 {
        rt.files_served.fetch_add(1, Ordering::Relaxed);
    }
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "seek failed").into_response();
    }
    // Download slots (global and per repository) for the body's lifetime.
    let waited = std::time::Instant::now();
    let g = state.global_downloads.clone().acquire_owned().await;
    let r = rt.downloads.clone().acquire_owned().await;
    let (Ok(gp), Ok(rp)) = (g, r) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "busy").into_response();
    };
    let addon = collect::addon_of(&rel, &rt.published.load().addons);
    let mut dl = Download::new(
        state.stats.clone(),
        rt.record().id,
        addon,
        sidecar,
        full,
        end - start,
        waited.elapsed().as_millis() as u64,
    );
    let state2 = state.clone();
    let rt2 = rt.clone();
    let stream = async_stream::stream! {
        let _permits = (gp, rp);
        let mut left = end - start;
        let mut buf = vec![0u8; 64 * 1024];
        while left > 0 {
            let want = (left as usize).min(buf.len());
            match file.read(&mut buf[..want]).await {
                Ok(0) => break,
                Ok(n) => {
                    state2.global_limiter.consume(n as u64).await;
                    rt2.limiter.consume(n as u64).await;
                    state2.note_served(&rt2, n as u64);
                    dl.add(n as u64);
                    left -= n as u64;
                    yield Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => { yield Err(e); break; }
            }
        }
    };
    (status, h, Body::from_stream(stream)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(None, 10), RangeReq::Full);
        assert_eq!(parse_range(Some("bytes=0-1"), 10), RangeReq::Partial(0, 1));
        // Legacy client asks one byte past the block, possibly past EOF.
        assert_eq!(
            parse_range(Some("bytes=8-100"), 10),
            RangeReq::Partial(8, 9)
        );
        assert_eq!(parse_range(Some("bytes=5-"), 10), RangeReq::Partial(5, 9));
        assert_eq!(parse_range(Some("bytes=-3"), 10), RangeReq::Partial(7, 9));
        assert_eq!(parse_range(Some("bytes=10-"), 10), RangeReq::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=0-1,4-5"), 10), RangeReq::Full);
        assert_eq!(parse_range(Some("items=0-1"), 10), RangeReq::Full);
    }

    #[test]
    fn paths() {
        assert_eq!(
            split_repo_path("/r/main//@ace/x.pbo"),
            Some(("main".into(), "//@ace/x.pbo".into()))
        );
        assert_eq!(
            percent_decode("/@M%C3%B3d%20%C3%BC/a").as_deref(),
            Some("/@Mód ü/a")
        );
        assert_eq!(percent_decode("/@Mód%20ü/a").as_deref(), Some("/@Mód ü/a"));
    }
}
