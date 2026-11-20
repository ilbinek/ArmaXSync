//! Web UI: server-rendered public pages (`/`, `/repo/<slug>`) and the
//! embedded admin single-page app (`/admin/*`).
//!
//! The public pages link the admin build's stylesheet (design tokens,
//! components, bundled fonts), so both share one look without a second build.

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

/// Fallback styles when no admin UI is embedded (dev builds without `pnpm build`).
const FALLBACK_STYLE: &str = r#"
:root{--bg:#0b0d10;--surface-1:#13171b;--surface-2:#191e23;--surface-3:#21272d;--line:#262d35;--line-strong:#36404a;--fg:#e8ebee;--fg-2:#a1abb6;--fg-3:#6f7a86;--accent:#f08a24;--accent-fg:#170c02;--accent-text:#ffa34d;--accent-soft:rgb(240 138 36/.13);--r:5px;--r-lg:8px;--font:system-ui,sans-serif;--font-display:"Arial Narrow",system-ui,sans-serif;--mono:ui-monospace,Consolas,monospace}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 var(--font)}a{color:var(--accent-text)}
.card{background:var(--surface-1);border:1px solid var(--line);border-radius:var(--r-lg);padding:16px}
.btn{display:inline-flex;align-items:center;gap:7px;height:34px;padding:0 14px;border:1px solid var(--line-strong);background:var(--surface-2);color:var(--fg);border-radius:var(--r);font:inherit;font-weight:600;text-decoration:none;cursor:pointer}
.btn.primary{background:var(--accent);border-color:var(--accent);color:var(--accent-fg)}.muted{color:var(--fg-2)}
.badge,.chip{display:inline-flex;align-items:center;padding:1px 8px;border-radius:3px;background:var(--surface-3);font-size:12.5px}
code{font-family:var(--mono);font-size:12.5px}
"#;

/// Page layout on top of the shared kit styles.
const PUBLIC_STYLE: &str = r#"
body{min-height:100vh;display:flex;flex-direction:column}
.top{position:relative;overflow:hidden;flex-shrink:0;border-bottom:1px solid var(--line);background:var(--surface-1)}
.top .in{position:relative;z-index:1}
.top .in,.wrap{width:100%;max-width:980px;margin:0 auto;padding:0 18px}
.top .in{display:flex;align-items:center;gap:14px;height:62px}
.brand{display:flex;align-items:center;gap:10px;color:var(--fg);text-decoration:none!important;line-height:1}
.brand b{font-family:var(--font-display);font-size:20px;letter-spacing:.06em;text-transform:uppercase}.brand b i{font-style:normal;color:var(--accent)}
.brand small{font-family:var(--font-display);font-size:11px;letter-spacing:.18em;text-transform:uppercase;color:var(--fg-3)}
.top nav{margin-left:auto;display:flex;gap:6px}
.wrap{flex:1;padding-bottom:40px}
.hero{padding:34px 0 6px}.hero h1{font-family:var(--font-display);font-size:34px;text-transform:uppercase;letter-spacing:.03em;margin:0 0 8px}
.hero p{margin:0;color:var(--fg-2);max-width:66ch}
.stackp{display:flex;flex-direction:column;gap:14px;margin-top:18px}
.rc h2{font-family:var(--font-display);font-size:26px;text-transform:uppercase;letter-spacing:.03em;margin:0}
.rc h2 a{color:inherit;text-decoration:none}.rc h2 a:hover{color:var(--accent-text)}
.meta{display:flex;flex-wrap:wrap;gap:6px;margin:10px 0 0}
.desc{margin:12px 0 0;color:var(--fg-2)}
.acts{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}
.cf{display:flex;flex-direction:column;gap:5px;margin-top:14px}.cf label{font-size:12.5px;font-weight:600;color:var(--fg-2)}
.cfbox{display:flex;align-items:center;gap:6px;padding:4px 4px 4px 10px;border:1px solid var(--line);border-radius:var(--r);background:var(--bg)}
.cfbox code{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:none;border:0;padding:0;color:var(--fg)}
.steps{margin:0;padding-left:20px;display:flex;flex-direction:column;gap:6px;color:var(--fg-2)}.steps b{color:var(--fg)}
.sec h3{margin:0 0 12px}
.mods{display:flex;flex-wrap:wrap;gap:6px}.mods .chip{font-family:var(--mono);font-size:12px}
.ev{padding:10px 0;border-bottom:1px solid var(--line)}.ev:last-child{border-bottom:0}.ev small{color:var(--fg-2)}
.cl{display:flex;gap:14px;padding:10px 0;border-bottom:1px solid var(--line)}.cl:last-child{border-bottom:0}
.cl .rev{font-family:var(--font-display);font-weight:700;font-size:16px;min-width:46px;color:var(--accent-text)}
.cl .when{font-size:12.5px;color:var(--fg-3);margin-bottom:4px}.cl .b{display:flex;flex-direction:column;gap:4px;min-width:0}
.cl code{background:none;border:0;padding:0;overflow-wrap:anywhere}
details summary{cursor:pointer;color:var(--accent-text);margin-top:10px}
.grid2p{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:14px}
.srv{display:flex;justify-content:space-between;gap:10px;padding:8px 0;border-bottom:1px solid var(--line)}.srv:last-child{border-bottom:0}
footer{color:var(--fg-3);font-size:12.5px;text-align:center;padding:22px 0}
@media (max-width:640px){.hero h1{font-size:26px}.top small{display:none}.top nav .ghost:first-child{display:none}}
"#;

const LOGO: &str = r#"<svg width="32" height="32" viewBox="0 0 32 32" aria-hidden="true"><rect x="1" y="1" width="30" height="30" rx="6" fill="var(--accent-soft)" stroke="var(--accent)" stroke-opacity=".45"/><path d="M16 4v4M16 24v4M4 16h4M24 16h4" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/><path d="M10.5 11.5l5.5 4.5-5.5 4.5" fill="none" stroke="var(--fg)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/><path d="M21.5 11.5l-5.5 4.5 5.5 4.5" fill="none" stroke="var(--accent)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

const COPY_SCRIPT: &str = "function axsCopy(t){if(navigator.clipboard&&window.isSecureContext)return navigator.clipboard.writeText(t).then(()=>true,()=>false);var a=document.createElement('textarea');a.value=t;a.style.position='fixed';a.style.top='-1000px';document.body.appendChild(a);a.select();var ok=false;try{ok=document.execCommand('copy')}catch(e){}document.body.removeChild(a);return Promise.resolve(ok)}document.querySelectorAll('[data-copy]').forEach(b=>b.addEventListener('click',()=>{axsCopy(b.dataset.copy).then(ok=>{b.textContent=ok?'Copied':'Press Ctrl+C';setTimeout(()=>b.textContent='Copy',1500)})}))";

/// URL of a (content-hashed) asset of the admin build, e.g. its stylesheet
/// or the topographic pattern, if a UI is embedded (or `ui_dir` is set).
fn kit_asset(state: &Shared, prefix: &str, ext: &str) -> Option<String> {
    let found = |name: &str| name.starts_with(prefix) && name.ends_with(ext);
    if let Some(dir) = &state.cfg.ui_dir {
        return std::fs::read_dir(dir.join("assets"))
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|n| found(n))
            .map(|n| format!("/admin/assets/{n}"));
    }
    AdminAssets::iter()
        .filter_map(|f| f.strip_prefix("assets/").map(str::to_owned))
        .find(|n| found(n))
        .map(|n| format!("/admin/assets/{n}"))
}

fn kit_css(state: &Shared) -> Option<String> {
    use std::sync::OnceLock;
    static CSS: OnceLock<Option<String>> = OnceLock::new();
    if state.cfg.ui_dir.is_some() {
        return kit_asset(state, "index-", ".css").or_else(|| kit_asset(state, "", ".css"));
    }
    CSS.get_or_init(|| kit_asset(state, "index-", ".css").or_else(|| kit_asset(state, "", ".css")))
        .clone()
}

fn topo_svg(state: &Shared) -> Option<String> {
    use std::sync::OnceLock;
    static TOPO: OnceLock<Option<String>> = OnceLock::new();
    if state.cfg.ui_dir.is_some() {
        return kit_asset(state, "topo", ".svg");
    }
    TOPO.get_or_init(|| kit_asset(state, "topo", ".svg"))
        .clone()
}

fn release_url() -> String {
    format!(
        "https://github.com/{}/releases/latest",
        crate::updates::RELEASE_REPO
    )
}

fn page(state: &Shared, title: &str, description: &str, body: &str) -> Html<String> {
    let mut css = match kit_css(state) {
        Some(href) => format!("<link rel=\"stylesheet\" href=\"{}\">", esc(&href)),
        None => format!("<style>{FALLBACK_STYLE}</style>"),
    };
    if let Some(topo) = topo_svg(state) {
        css.push_str(&format!(
            "<style>.top::before{{content:\"\";position:absolute;inset:0;pointer-events:none;opacity:.14;background:var(--accent);-webkit-mask:url({0}) 0 0/640px 640px repeat;mask:url({0}) 0 0/640px 640px repeat}}</style>",
            esc(&topo)
        ));
    }
    let title = esc(title);
    let desc = esc(description);
    let rel = esc(&release_url());
    let ver = env!("CARGO_PKG_VERSION");
    Html(format!(
        "<!doctype html><html lang=\"en\" data-theme=\"system\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{title}</title><meta name=\"description\" content=\"{desc}\">\
<meta property=\"og:title\" content=\"{title}\"><meta property=\"og:description\" content=\"{desc}\"><meta property=\"og:type\" content=\"website\">\
<meta name=\"theme-color\" content=\"#f08a24\"><meta name=\"color-scheme\" content=\"dark light\">\
<link rel=\"icon\" type=\"image/svg+xml\" href=\"/admin/favicon.svg\">\
{css}<style>{PUBLIC_STYLE}</style></head><body>\
<header class=\"top\"><div class=\"in\"><a class=\"brand\" href=\"/\">{LOGO}<span><b>Arma<i>X</i>Sync</b><br><small>Mod repositories</small></span></a>\
<nav><a class=\"btn ghost sm\" href=\"{rel}\" rel=\"noopener\">Get ArmaXSync</a><a class=\"btn ghost sm\" href=\"/admin/\">Admin</a></nav></div></header>\
<main class=\"wrap\">{body}</main><footer>Powered by ArmaXSync {ver}</footer><script>{COPY_SCRIPT}</script></body></html>"
    ))
}

fn copy_field(label: &str, value: &str) -> String {
    let v = esc(value);
    format!(
        "<div class=\"cf\"><label>{}</label><div class=\"cfbox\"><code title=\"{v}\">{v}</code><button class=\"btn sm\" data-copy=\"{v}\">Copy</button></div></div>",
        esc(label)
    )
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
        format!("<a href=\"/repo/{}\">{}</a>", esc(&r.slug), esc(&r.name))
    } else {
        esc(&r.name)
    };
    let mut meta = vec![
        format!("<span class=\"badge\">{}</span>", esc(&r.game.to_string())),
        format!("<span class=\"badge plain\">Revision {}</span>", p.revision),
        format!(
            "<span class=\"badge plain\">{} files · {}</span>",
            p.file_count,
            human(p.total_size)
        ),
    ];
    if p.build_date > 0 {
        meta.push(format!(
            "<span class=\"badge plain\">Updated {}</span>",
            date(p.build_date)
        ));
    }
    if r.settings.visibility == Visibility::Private {
        meta.push("<span class=\"badge warn\">Login required</span>".into());
    }
    let desc = if r.description.is_empty() {
        String::new()
    } else {
        format!("<p class=\"desc\">{}</p>", esc(&r.description))
    };
    let details = if link {
        format!(
            "<a class=\"btn\" href=\"/repo/{}\">Details</a>",
            esc(&r.slug)
        )
    } else {
        String::new()
    };
    format!(
        "<article class=\"card rc\"><h2>{title}</h2><div class=\"meta\">{}</div>{desc}<div class=\"acts\"><a class=\"btn primary\" href=\"{}\">Open in ArmaXSync</a>{details}</div>{}</article>",
        meta.join(""),
        esc(&deep),
        copy_field("ArmA3Sync auto-config URL", &auto),
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
        "<div class=\"card muted\" style=\"margin-top:18px\">No public repositories yet.</div>"
            .to_owned()
    } else {
        format!("<div class=\"stackp\">{cards}</div>")
    };
    page(
        &state,
        "Mod repositories",
        "Arma mod repositories hosted with ArmaXSync.",
        &format!(
            "<section class=\"hero\"><h1>Mod repositories</h1><p>Install ArmaXSync, then click <b>Open in ArmaXSync</b> to add a repository. ArmA3Sync users can paste the auto-config URL instead.</p></section>{body}"
        ),
    )
}

fn not_found(state: &Shared, what: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        page(
            state,
            "Not found",
            "",
            &format!(
                "<section class=\"hero\"><h1>Not found</h1><p>{}</p><p style=\"margin-top:14px\"><a class=\"btn\" href=\"/\">All repositories</a></p></section>",
                esc(what)
            ),
        ),
    )
        .into_response()
}

fn changelog_entry(c: &axs_model::ChangelogEntry) -> String {
    let mut parts = vec![];
    for (l, v, tone) in [
        ("Added", &c.new_addons, "ok"),
        ("Updated", &c.updated_addons, "info"),
        ("Removed", &c.deleted_addons, "err"),
    ] {
        if !v.is_empty() {
            parts.push(format!(
                "<div><span class=\"badge {tone}\">{l}</span> <code>{}</code></div>",
                esc(&v.join(", "))
            ));
        }
    }
    if parts.is_empty() {
        parts.push(format!(
            "<div class=\"muted\">{}</div>",
            if c.content_updated {
                "Content updated"
            } else {
                "No content change"
            }
        ));
    }
    format!(
        "<div class=\"cl\"><span class=\"rev\">#{}</span><div class=\"b\"><div class=\"when\">{}</div>{}</div></div>",
        c.revision,
        date(c.build_date),
        parts.join("")
    )
}

async fn repo_page(State(state): State<Shared>, Path(slug): Path<String>) -> Response {
    let Some(rt) = state.repo(&slug) else {
        return not_found(&state, "This repository does not exist.");
    };
    let r = rt.record();
    if !r.settings.listed {
        return not_found(&state, "This repository does not exist.");
    }
    let private = r.settings.visibility == Visibility::Private;
    let url = state.cfg.repo_url(&r.slug);
    let mut body = format!("<div class=\"stackp\">{}", repo_card(&state, &rt, false));
    body.push_str(&format!(
        "<section class=\"card sec\"><h3>How to join</h3><ol class=\"steps\"><li><b>Install ArmaXSync</b> from <a href=\"{rel}\" rel=\"noopener\">the download page</a> (Windows and Linux).</li><li>Click <b>Open in ArmaXSync</b> above, choose a download folder and start the download.</li><li>Start the game from the <b>Play</b> page; the repository's mods are selected for you.</li><li><b>Still on ArmA3Sync?</b> Add a repository with the auto-config URL above.</li></ol>{}</section>",
        copy_field("Repository URL", &url),
        rel = esc(&release_url()),
    ));
    if let Some(m) = crate::repos_api::manifest_of(&rt) {
        let mut cols = String::new();
        if !m.events.is_empty() {
            cols.push_str("<section class=\"card sec\"><h3>Events</h3>");
            for e in &m.events {
                let req = e.addons.iter().filter(|a| !a.optional).count();
                let opt = e.addons.len() - req;
                cols.push_str(&format!(
                    "<div class=\"ev\"><b>{}</b><br><small>{}{} required{}</small></div>",
                    esc(&e.name),
                    e.description
                        .as_deref()
                        .map(|d| format!("{} · ", esc(d)))
                        .unwrap_or_default(),
                    req,
                    if opt > 0 {
                        format!(", {opt} optional")
                    } else {
                        String::new()
                    }
                ));
            }
            cols.push_str("</section>");
        }
        if !m.servers.is_empty() {
            cols.push_str("<section class=\"card sec\"><h3>Servers</h3>");
            for s in &m.servers {
                cols.push_str(&format!(
                    "<div class=\"srv\"><b>{}</b><code>{}:{}</code></div>",
                    esc(&s.name),
                    esc(&s.host),
                    s.port
                ));
            }
            cols.push_str("</section>");
        }
        if !cols.is_empty() {
            body.push_str(&format!("<div class=\"grid2p\">{cols}</div>"));
        }
        // Private repositories don't advertise their content.
        if !private && !m.addons.is_empty() {
            body.push_str(&format!(
                "<section class=\"card sec\"><h3>Mods ({})</h3><div class=\"mods\">",
                m.addons.len()
            ));
            for a in &m.addons {
                body.push_str(&format!(
                    "<span class=\"chip\">{}</span>",
                    esc(axs_model::paths::file_name(a))
                ));
            }
            body.push_str("</div></section>");
        }
        if !m.changelogs.is_empty() {
            let all: Vec<String> = m.changelogs.iter().rev().map(changelog_entry).collect();
            body.push_str("<section class=\"card sec\"><h3>Changelog</h3>");
            body.push_str(&all.iter().take(10).cloned().collect::<String>());
            if all.len() > 10 {
                body.push_str(&format!(
                    "<details><summary>{} older revisions</summary>{}</details>",
                    all.len() - 10,
                    all[10..].concat()
                ));
            }
            body.push_str("</section>");
        }
    }
    body.push_str("</div>");
    let desc = if r.description.is_empty() {
        format!("{} mod repository for {}", r.name, r.game)
    } else {
        r.description.clone()
    };
    page(&state, &r.name, &desc, &body).into_response()
}

/// `/admin/*` (SPA with index.html fallback) and 404 for everything else.
pub async fn fallback(State(state): State<Shared>, req: Request) -> Response {
    let path = req.uri().path();
    let Some(rest) = path.strip_prefix("/admin") else {
        return not_found(&state, "There is no page at this address.");
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
        // Vite content-hashes everything under assets/.
        Some(f) => asset_response(file, f.data.into_owned(), file.starts_with("assets/")),
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
