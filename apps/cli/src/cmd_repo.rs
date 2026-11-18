//! `armaxsync repo …` — manage registered repositories.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};

use axs_client::{AdminSettings, Client, RepoConfig};

use crate::{output, Ctx};

#[derive(Subcommand)]
pub enum RepoCmd {
    /// Register a repository from an autoconfig URL, base URL or armaxsync:// link.
    Add(AddArgs),
    /// List registered repositories.
    List,
    /// Show one repository's settings.
    Show { name: String },
    /// Remove a repository (local files are kept).
    Remove { name: String },
    /// Change repository settings.
    Set(SetArgs),
    /// Configure local build settings (repository administration).
    Admin(AdminArgs),
}

#[derive(Args)]
pub struct AddArgs {
    /// Autoconfig URL (…/.a3s/autoconfig), repository URL or armaxsync:// link.
    url: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    login: Option<String>,
    #[arg(long, env = "ARMAXSYNC_REPO_PASSWORD", hide_env_values = true)]
    password: Option<String>,
    /// Download destination folder.
    #[arg(long)]
    dest: Option<PathBuf>,
    /// Do not contact the server; register `url` as-is.
    #[arg(long)]
    offline: bool,
}

#[derive(Args)]
pub struct SetArgs {
    name: String,
    #[arg(long)]
    rename: Option<String>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    login: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    dest: Option<PathBuf>,
    #[arg(long)]
    exact_match: Option<bool>,
    #[arg(long)]
    auto_discover: Option<bool>,
    #[arg(long)]
    notify: Option<bool>,
    #[arg(long)]
    auto: Option<bool>,
    /// Client-side connection cap (0 = server value).
    #[arg(long)]
    connections: Option<u32>,
    /// Download speed limit in bytes/s (0 = unlimited); accepts K/M suffixes.
    #[arg(long)]
    max_speed: Option<String>,
    #[arg(long)]
    connect_timeout: Option<u64>,
    #[arg(long)]
    read_timeout: Option<u64>,
    #[arg(long)]
    accept_invalid_certs: Option<bool>,
    /// Keep extra local content in this folder (repeatable; replaces list).
    #[arg(long = "keep-extras")]
    keep_extras: Vec<String>,
    #[arg(long)]
    prefer_modern: Option<bool>,
}

#[derive(Args)]
pub struct AdminArgs {
    name: String,
    /// Repository content folder.
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long)]
    public_url: Option<String>,
    #[arg(long)]
    compress: Option<bool>,
    #[arg(long)]
    compressed_only: Option<bool>,
    #[arg(long)]
    connections: Option<u32>,
    #[arg(long)]
    partial_transfer: Option<bool>,
    #[arg(long)]
    embed_credentials: Option<bool>,
    #[arg(long = "exclude")]
    exclude: Vec<String>,
    #[arg(long = "hidden")]
    hidden: Vec<String>,
}

pub fn parse_speed(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&s[..s.len() - 1], 1_000u64),
        Some('M') => (&s[..s.len() - 1], 1_000_000),
        Some('G') => (&s[..s.len() - 1], 1_000_000_000),
        _ => (s, 1),
    };
    let v: f64 = num.trim().parse().context("bad speed")?;
    Ok((v * mult as f64) as u64)
}

fn print_repo(r: &RepoConfig, ctx: &Ctx) {
    if ctx.json {
        let mut v = serde_json::to_value(r).unwrap_or_default();
        if let Some(p) = v.get_mut("password") {
            if !p.is_null() {
                *p = serde_json::json!("********");
            }
        }
        output::json_line(&v);
        return;
    }
    println!("{}", r.name);
    println!("  url:          {}", r.url);
    println!("  autoconfig:   {}", r.autoconfig_url());
    println!(
        "  login:        {}",
        r.login.as_deref().unwrap_or("anonymous")
    );
    println!(
        "  destination:  {}",
        r.destination
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  exact match:  {}   auto-discover: {}   notify: {}   auto: {}",
        r.exact_match, r.auto_discover, r.notify, r.auto
    );
    println!(
        "  revision:     local {}  remote {}  status {:?}",
        r.state
            .revision
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into()),
        r.state
            .remote_revision
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into()),
        r.state.status
    );
    if let Some(a) = &r.admin {
        println!(
            "  admin path:   {}",
            a.path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
}

pub async fn run(cmd: RepoCmd, client: &mut Client, ctx: &Ctx) -> anyhow::Result<i32> {
    match cmd {
        RepoCmd::Add(a) => {
            let mut r = if a.offline {
                let ep = axs_transport::Endpoint::parse(
                    &a.url,
                    a.login.as_deref(),
                    a.password.as_deref(),
                )?;
                RepoConfig {
                    name: a.name.clone().unwrap_or_else(|| ep.host.clone()),
                    url: ep.base_url(),
                    login: a.login.clone(),
                    password: a.password.clone(),
                    destination: client.config.default_destination.clone(),
                    ..Default::default()
                }
            } else {
                client
                    .resolve_url(&a.url, a.login.as_deref(), a.password.as_deref())
                    .await?
            };
            if let Some(n) = a.name {
                r.name = n;
            }
            if let Some(d) = a.dest {
                r.destination = Some(d);
            }
            client.add_repo(r.clone())?;
            if ctx.json {
                output::json_line(&serde_json::json!({"added": r.name, "url": r.url}));
            } else {
                println!("Repository {} added ({}).", r.name, r.url);
                if r.destination.is_none() {
                    println!(
                        "Set a destination with: armaxsync repo set {:?} --dest <folder>",
                        r.name
                    );
                }
            }
        }
        RepoCmd::List => {
            if ctx.json {
                for r in client.repos() {
                    output::json_line(&serde_json::json!({
                        "name": r.name, "url": r.url, "status": r.state.status,
                        "revision": r.state.revision, "remote_revision": r.state.remote_revision,
                        "destination": r.destination,
                    }));
                }
            } else {
                println!("Number of repositories found: {}", client.repos().len());
                for r in client.repos() {
                    println!(
                        "{:<30} {:<8} {}",
                        r.name,
                        format!("{:?}", r.state.status).to_uppercase(),
                        r.url
                    );
                }
            }
        }
        RepoCmd::Show { name } => print_repo(client.repo(&name)?, ctx),
        RepoCmd::Remove { name } => {
            client.remove_repo(&name)?;
            if !ctx.json {
                println!("Repository {name} removed.");
            }
        }
        RepoCmd::Set(a) => {
            let mut r = client.repo(&a.name)?.clone();
            let old = r.name.clone();
            if let Some(v) = a.url {
                r.url = axs_transport::Endpoint::parse(&v, None, None)?.base_url();
            }
            if let Some(v) = a.login {
                r.login = Some(v).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("anonymous"));
            }
            if let Some(v) = a.password {
                r.password = Some(v).filter(|s| !s.is_empty());
            }
            if let Some(v) = a.dest {
                r.destination = Some(v);
            }
            macro_rules! set { ($($f:ident),*) => { $( if let Some(v) = a.$f { r.$f = v; } )* } }
            set!(
                exact_match,
                auto_discover,
                notify,
                auto,
                connections,
                accept_invalid_certs,
                prefer_modern
            );
            if let Some(v) = a.max_speed {
                r.max_speed = parse_speed(&v)?;
            }
            if let Some(v) = a.connect_timeout {
                r.connect_timeout_secs = Some(v);
            }
            if let Some(v) = a.read_timeout {
                r.read_timeout_secs = Some(v);
            }
            if !a.keep_extras.is_empty() {
                r.keep_extras = a
                    .keep_extras
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            client.save_repo(r)?;
            if let Some(new) = a.rename {
                client.rename_repo(&old, &new)?;
            }
        }
        RepoCmd::Admin(a) => {
            let mut r = client.repo(&a.name)?.clone();
            let mut s = r.admin.clone().unwrap_or(AdminSettings {
                connections: 1,
                partial_transfer: true,
                embed_credentials: true,
                ..Default::default()
            });
            if let Some(p) = a.path {
                anyhow::ensure!(p.is_dir(), "{} is not a directory", p.display());
                s.path = Some(p);
            }
            macro_rules! set { ($($f:ident),*) => { $( if let Some(v) = a.$f { s.$f = v; } )* } }
            set!(
                compress,
                compressed_only,
                connections,
                partial_transfer,
                embed_credentials
            );
            if let Some(u) = a.public_url {
                s.public_url = Some(u).filter(|u| !u.is_empty());
            }
            if !a.exclude.is_empty() {
                s.exclude = a.exclude;
            }
            if !a.hidden.is_empty() {
                s.hidden = a.hidden;
            }
            r.admin = Some(s);
            client.save_repo(r)?;
        }
    }
    Ok(crate::exit::OK)
}
