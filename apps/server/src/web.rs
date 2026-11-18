//! Web UI: server-rendered public pages (`/`, `/repo/<slug>`) and the
//! embedded admin single-page app (`/admin/*`).

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::state::{Shared, Visibility};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../ui/apps/admin/build"]
#[allow_missing = true]
struct AdminAssets;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/", get(index))
        .route("/repo/{slug}", get(repo_page))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn human(n: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < units.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", units[u])
    }
}

fn date(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default()
}

const STYLE: &str = r#"
:root{--bg:#f6f7f9;--fg:#15181d;--muted:#5b6472;--card:#fff;--line:#e3e6ea;--accent:#2f6fde;--accent-fg:#fff;--chip:#eef2f8}
@media (prefers-color-scheme: dark){:root{--bg:#111418;--fg:#e8ebef;--muted:#9aa4b2;--card:#1a1f25;--line:#2a313a;--accent:#5b8ff0;--accent-fg:#0b0f14;--chip:#232a33}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}
header{border-bottom:1px solid var(--line);background:var(--card)}header .in{display:flex;align-items:center;gap:12px;padding:14px 16px;max-width:960px;margin:0 auto}
header b{font-size:17px}header a{color:var(--muted);text-decoration:none;margin-left:auto}main{max-width:960px;margin:0 auto;padding:20px 16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:18px;margin-bottom:14px}
.card h2{margin:0 0 4px;font-size:18px}.muted{color:var(--muted)}.row{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin-top:12px}
.btn{display:inline-block;padding:8px 14px;border-radius:8px;background:var(--accent);color:var(--accent-fg);text-decoration:none;font-weight:600;border:0;cursor:pointer;font:inherit}
.btn.ghost{background:var(--chip);color:var(--fg)}code{background:var(--chip);padding:3px 6px;border-radius:6px;font-size:13px;word-break:break-all}
.chips span{display:inline-block;background:var(--chip);border-radius:999px;padding:2px 10px;margin:2px;font-size:13px}
table{width:100%;border-collapse:collapse}td,th{text-align:left;padding:6px 4px;border-bottom:1px solid var(--line);font-size:14px;vertical-align:top}
"#;

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{STYLE}</style></head><body><header><div class=\"in\"><b>ArmaXSync</b><span class=\"muted\">mod repositories</span><a href=\"/admin/\">Admin</a></div></header><main>{body}</main><script>document.querySelectorAll('[data-copy]').forEach(b=>b.addEventListener('click',()=>{{navigator.clipboard.writeText(b.dataset.copy);b.textContent='Copied';setTimeout(()=>b.textContent='Copy',1500)}}))</script></body></html>",
        esc(title)
    ))
}

fn repo_card(state: &Shared, rt: &crate::state::RepoRuntime, link: bool) -> String {
    let r = rt.record();
    let p = rt.published.load();
    let url = state.cfg.repo_url(&r.slug);
    let auto = format!("{url}/.a3s/autoconfig");
    let enc = |s: &str| {
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    let deep = format!("armaxsync://add?url={}&name={}", enc(&url), enc(&r.name));
    let title = if link {
        format!(
            "<a href=\"/repo/{0}\" style=\"color:inherit\">{1}</a>",
            esc(&r.slug),
            esc(&r.name)
        )
    } else {
        esc(&r.name)
    };
    let private = if r.settings.visibility == Visibility::Private {
        " · private (login required)"
    } else {
        ""
    };
    format!(
        "<div class=\"card\"><h2>{title}</h2><div class=\"muted\">{} · revision {} · {} files · {}{}{}</div>{}<div class=\"row\"><a class=\"btn\" href=\"{}\">Open in ArmaXSync</a></div><div class=\"row\"><span class=\"muted\">Arma3Sync auto-config:</span><code>{}</code><button class=\"btn ghost\" data-copy=\"{}\">Copy</button></div></div>",
        esc(&r.game.to_string()),
        p.revision,
        p.file_count,
        human(p.total_size),
        if p.build_date > 0 { format!(" · updated {}", date(p.build_date)) } else { String::new() },
        private,
        if r.description.is_empty() { String::new() } else { format!("<p>{}</p>", esc(&r.description)) },
        esc(&deep),
        esc(&auto),
        esc(&auto)
    )
}

async fn index(State(state): State<Shared>) -> Html<String> {
    let cards: String = state
        .repo_list()
        .iter()
        .filter(|rt| rt.record().settings.listed)
        .map(|rt| repo_card(&state, rt, true))
        .collect();
    let body = if cards.is_empty() {
        "<div class=\"card muted\">No public repositories yet.</div>".to_owned()
    } else {
        cards
    };
    page("ArmaXSync repositories", &body)
}

async fn repo_page(State(state): State<Shared>, Path(slug): Path<String>) -> Response {
    let Some(rt) = state.repo(&slug) else {
        return (
            StatusCode::NOT_FOUND,
            page(
                "Not found",
                "<div class=\"card\">Repository not found.</div>",
            ),
        )
            .into_response();
    };
    if !rt.record().settings.listed {
        return (
            StatusCode::NOT_FOUND,
            page(
                "Not found",
                "<div class=\"card\">Repository not found.</div>",
            ),
        )
            .into_response();
    }
    let mut body = repo_card(&state, &rt, false);
    if let Some(m) = crate::repos_api::manifest_of(&rt) {
        if !m.addons.is_empty() {
            body.push_str("<div class=\"card\"><h2>Mods</h2><div class=\"chips\">");
            for a in &m.addons {
                body.push_str(&format!(
                    "<span>{}</span>",
                    esc(axs_model::paths::file_name(a))
                ));
            }
            body.push_str("</div></div>");
        }
        if !m.events.is_empty() {
            body.push_str("<div class=\"card\"><h2>Events</h2><table>");
            for e in &m.events {
                body.push_str(&format!(
                    "<tr><td><b>{}</b><br><span class=\"muted\">{}</span></td><td>{}</td></tr>",
                    esc(&e.name),
                    esc(e.description.as_deref().unwrap_or("")),
                    e.addons
                        .iter()
                        .map(|a| format!(
                            "{}{}",
                            esc(&a.name),
                            if a.optional { " (optional)" } else { "" }
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            body.push_str("</table></div>");
        }
        if !m.servers.is_empty() {
            body.push_str("<div class=\"card\"><h2>Servers</h2><table>");
            for s in &m.servers {
                body.push_str(&format!(
                    "<tr><td>{}</td><td><code>{}:{}</code></td></tr>",
                    esc(&s.name),
                    esc(&s.host),
                    s.port
                ));
            }
            body.push_str("</table></div>");
        }
        if !m.changelogs.is_empty() {
            body.push_str("<div class=\"card\"><h2>Changelog</h2><table>");
            for c in m.changelogs.iter().rev() {
                let mut parts = vec![];
                for (l, v) in [
                    ("new", &c.new_addons),
                    ("updated", &c.updated_addons),
                    ("removed", &c.deleted_addons),
                ] {
                    if !v.is_empty() {
                        parts.push(format!("<b>{l}:</b> {}", esc(&v.join(", "))));
                    }
                }
                if parts.is_empty() {
                    parts.push(if c.content_updated {
                        "content updated".into()
                    } else {
                        "<span class=\"muted\">no content change</span>".into()
                    });
                }
                body.push_str(&format!(
                    "<tr><td>#{}<br><span class=\"muted\">{}</span></td><td>{}</td></tr>",
                    c.revision,
                    date(c.build_date),
                    parts.join("<br>")
                ));
            }
            body.push_str("</table></div>");
        }
    }
    page(&rt.record().name, &body).into_response()
}

/// `/admin/*` (SPA with index.html fallback) and 404 for everything else.
pub async fn fallback(State(state): State<Shared>, req: Request) -> Response {
    let path = req.uri().path();
    let Some(rest) = path.strip_prefix("/admin") else {
        return (
            StatusCode::NOT_FOUND,
            page("Not found", "<div class=\"card\">Page not found.</div>"),
        )
            .into_response();
    };
    let rest = rest.trim_start_matches('/');
    if let Some(dir) = &state.cfg.ui_dir {
        let p = dir.join(if rest.is_empty() { "index.html" } else { rest });
        let p = if p.is_file() {
            p
        } else {
            dir.join("index.html")
        };
        return match tokio::fs::read(&p).await {
            Ok(b) => asset_response(&p.to_string_lossy(), b, false),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    }
    let file = if rest.is_empty() { "index.html" } else { rest };
    match AdminAssets::get(file) {
        Some(f) => asset_response(file, f.data.into_owned(), file.contains("/immutable/")),
        None => match AdminAssets::get("index.html") {
            Some(f) => asset_response("index.html", f.data.into_owned(), false),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

fn asset_response(name: &str, data: Vec<u8>, immutable: bool) -> Response {
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    let mut r = Response::new(Body::from(data));
    if let Ok(v) = HeaderValue::from_str(mime.as_ref()) {
        r.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        }),
    );
    r.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    r.headers_mut()
        .insert("x-frame-options", HeaderValue::from_static("DENY"));
    r
}
