//! `armaxsync build` — build a repository folder.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use anyhow::{bail, Context};
use clap::Args;
use serde::{Deserialize, Serialize};

use axs_build::{BuildEvent, BuildOptions, FileHashCache};
use axs_model::GameId;

use crate::output;
use crate::Ctx;

/// Settings persisted in `<repo>/.axs/build.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildConfig {
    pub name: Option<String>,
    pub game: Option<GameId>,
    /// Public base URL of the repository (`https://host/path`).
    pub url: Option<String>,
    pub login: Option<String>,
    pub password: Option<String>,
    /// Publish login/password in the public autoconfig (legacy behaviour).
    pub embed_credentials: Option<bool>,
    pub compress_pbo: Option<bool>,
    pub compressed_pbo_only: Option<bool>,
    pub connections: Option<u32>,
    pub partial_transfer: Option<bool>,
    pub zsync: Option<bool>,
    pub exclude: Vec<String>,
    pub hidden: Vec<String>,
    pub servers: Vec<ServerEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub modset: Option<String>,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Repository folder.
    path: Option<PathBuf>,
    /// Build a repository registered in the client by name (legacy `-build`).
    #[arg(long, conflicts_with = "path")]
    repo: Option<String>,
    /// Repository display name.
    #[arg(long)]
    name: Option<String>,
    /// Public base URL clients use, e.g. https://mods.example.com/r/main
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    login: Option<String>,
    #[arg(long, env = "ARMAXSYNC_REPO_PASSWORD", hide_env_values = true)]
    password: Option<String>,
    /// Do not publish login/password inside the public autoconfig.
    #[arg(long)]
    no_embed_credentials: bool,
    /// Create .pbo.zip sidecars.
    #[arg(long)]
    compress: bool,
    #[arg(long)]
    no_compress: bool,
    /// Only advertise compressed PBOs to clients.
    #[arg(long)]
    compressed_only: bool,
    /// Max parallel connections per client (1-10).
    #[arg(long)]
    connections: Option<u32>,
    /// Tell clients the server has no HTTP range support.
    #[arg(long)]
    no_partial: bool,
    /// Do not generate .zsync files.
    #[arg(long)]
    no_zsync: bool,
    /// Exclude a file (relative path); repeatable.
    #[arg(long = "exclude")]
    exclude: Vec<String>,
    /// Folder clients must not clean up (relative path); repeatable.
    #[arg(long = "hidden")]
    hidden: Vec<String>,
    /// Favorite server "name=host:port[,password[,modset]]"; repeatable.
    #[arg(long = "server")]
    servers: Vec<String>,
    #[arg(long)]
    game: Option<GameId>,
    /// Only publish the legacy .a3s metadata.
    #[arg(long)]
    legacy_only: bool,
    /// Use this config file instead of <path>/.axs/build.toml.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Save the effective settings to the config file.
    #[arg(long)]
    save: bool,
    /// Hashing threads (default: all cores).
    #[arg(long, default_value_t = 0)]
    threads: usize,
}

fn parse_server(s: &str) -> anyhow::Result<ServerEntry> {
    let (name, rest) = s
        .split_once('=')
        .context("expected name=host:port[,password[,modset]]")?;
    let mut parts = rest.splitn(3, ',');
    let hp = parts.next().unwrap_or_default();
    let (host, port) = hp.rsplit_once(':').context("server needs host:port")?;
    Ok(ServerEntry {
        name: name.trim().into(),
        host: host.trim().into(),
        port: port.trim().parse().context("bad port")?,
        password: parts.next().map(str::to_owned).filter(|p| !p.is_empty()),
        modset: parts.next().map(str::to_owned).filter(|p| !p.is_empty()),
    })
}

pub fn run(a: BuildArgs, ctx: &Ctx) -> anyhow::Result<i32> {
    if let Some(name) = &a.repo {
        return build_registered(name, ctx);
    }
    let Some(root) = a.path.clone() else {
        bail!("missing repository folder (or --repo <name>)");
    };
    let root = std::fs::canonicalize(&root).with_context(|| format!("{}", root.display()))?;
    let root = PathBuf::from(dunce_simplify(&root));
    let cfg_path = a
        .config
        .clone()
        .unwrap_or_else(|| root.join(".axs").join("build.toml"));
    let mut cfg: BuildConfig = match std::fs::read_to_string(&cfg_path) {
        Ok(s) => toml_from_str(&s).with_context(|| format!("reading {}", cfg_path.display()))?,
        Err(_) => BuildConfig::default(),
    };
    // Flags override the stored config.
    macro_rules! set {
        ($field:ident, $v:expr) => {
            if let Some(v) = $v {
                cfg.$field = Some(v);
            }
        };
    }
    set!(name, a.name.clone());
    set!(url, a.url.clone());
    set!(login, a.login.clone());
    set!(password, a.password.clone());
    set!(connections, a.connections);
    set!(game, a.game);
    if a.no_embed_credentials {
        cfg.embed_credentials = Some(false);
    }
    if a.compress {
        cfg.compress_pbo = Some(true);
    }
    if a.no_compress {
        cfg.compress_pbo = Some(false);
    }
    if a.compressed_only {
        cfg.compressed_pbo_only = Some(true);
    }
    if a.no_partial {
        cfg.partial_transfer = Some(false);
    }
    if a.no_zsync {
        cfg.zsync = Some(false);
    }
    if !a.exclude.is_empty() {
        cfg.exclude = a.exclude.clone();
    }
    if !a.hidden.is_empty() {
        cfg.hidden = a.hidden.clone();
    }
    if !a.servers.is_empty() {
        cfg.servers = a
            .servers
            .iter()
            .map(|s| parse_server(s))
            .collect::<anyhow::Result<_>>()?;
    }

    let name = cfg.name.clone().unwrap_or_else(|| {
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repository".into())
    });
    let mut opts = BuildOptions::new(&root, &name);
    opts.game = cfg.game.unwrap_or_default();
    if let Some(url) = &cfg.url {
        opts.protocol = Some(
            axs_legacy::urls::protocol_from_url(url, cfg.login.as_deref(), cfg.password.as_deref())
                .with_context(|| {
                    format!("unsupported url {url:?} (expected http://, https:// or ftp://)")
                })?,
        );
    } else if !ctx.quiet {
        eprintln!(
            "note: no --url given; the published autoconfig will not contain a download address"
        );
    }
    opts.embed_credentials = cfg.embed_credentials.unwrap_or(true);
    opts.compress_pbo = cfg.compress_pbo.unwrap_or(false);
    opts.compressed_pbo_only = cfg.compressed_pbo_only.unwrap_or(false);
    opts.connections = cfg.connections.unwrap_or(1).clamp(1, 10);
    opts.partial_transfer = cfg.partial_transfer.unwrap_or(true);
    opts.zsync = cfg.zsync;
    opts.excluded_files = cfg.exclude.clone();
    opts.hidden_paths = cfg.hidden.clone();
    opts.favorite_servers = cfg
        .servers
        .iter()
        .map(|s| {
            axs_model::ServerDef {
                name: s.name.clone(),
                host: s.host.clone(),
                port: s.port,
                password: s.password.clone(),
                modset: s.modset.clone(),
            }
            .to_legacy(&name)
        })
        .collect();
    opts.emit_modern = !a.legacy_only;
    opts.emit_legacy = opts.game.supports_legacy();
    opts.threads = a.threads;

    if a.save {
        let mut saved = cfg.clone();
        saved.name = Some(name.clone());
        if let Some(dir) = cfg_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&cfg_path, toml_to_string(&saved)?)?;
    }

    let mut cache = FileHashCache::load(root.join(".axs").join("build-cache.zst"));
    let pb = output::bar(ctx.quiet, 0, "hashing");
    let pb_lock = Mutex::new(pb);
    let progress = |ev: BuildEvent| {
        let pb = pb_lock.lock().unwrap();
        match ev {
            BuildEvent::Scanned { files, bytes } => {
                pb.println(format!(
                    "Building {name}: {files} files, {}",
                    output::bytes(bytes)
                ));
            }
            BuildEvent::Hashing {
                bytes_done,
                bytes_total,
                ..
            } => {
                pb.set_length(bytes_total);
                pb.set_position(bytes_done);
            }
            BuildEvent::Compressing { done, total } => {
                pb.set_message("compressing");
                pb.set_length(total as u64);
                pb.set_position(done as u64);
            }
            BuildEvent::WritingMetadata => pb.set_message("publishing"),
        }
    };
    let cancel = AtomicBool::new(false);
    let report = axs_build::build(&opts, &mut cache, &progress, &cancel)?;
    pb_lock.lock().unwrap().finish_and_clear();

    if ctx.json {
        output::json_line(
            &serde_json::json!({"event": "build", "repository": name, "report": report}),
        );
    } else {
        println!(
            "Repository {name} - build finished: revision {}, {} files, {} ({} hashed, {} zsync, {} zip, {} stale sidecars removed) in {:.1}s",
            report.revision,
            report.files,
            output::bytes(report.total_size),
            report.hashed_files,
            report.zsync_written,
            report.zips_written,
            report.sidecars_removed,
            report.duration_ms as f64 / 1000.0
        );
        for (label, list) in [
            ("new", &report.new_addons),
            ("updated", &report.updated_addons),
            ("deleted", &report.deleted_addons),
        ] {
            if !list.is_empty() {
                println!("  {label}: {}", list.join(", "));
            }
        }
        if let Some(p) = &opts.protocol {
            println!("  autoconfig: {}", axs_legacy::urls::autoconfig_url(p));
        }
    }
    Ok(crate::exit::OK)
}

/// Build a repository registered with `repo admin` (legacy `-build <name>`).
fn build_registered(name: &str, ctx: &Ctx) -> anyhow::Result<i32> {
    let mut client = ctx.client()?;
    let pb = Mutex::new(output::bar(ctx.quiet, 0, "hashing"));
    let progress = |ev: BuildEvent| {
        let pb = pb.lock().unwrap();
        if let BuildEvent::Hashing {
            bytes_done,
            bytes_total,
            ..
        } = ev
        {
            pb.set_length(bytes_total);
            pb.set_position(bytes_done);
        }
    };
    let report = client.build_admin(name, &progress, &AtomicBool::new(false))?;
    pb.lock().unwrap().finish_and_clear();
    if ctx.json {
        output::json_line(
            &serde_json::json!({"event": "build", "repository": name, "report": report}),
        );
    } else {
        println!(
            "Repository {name} - build finished: revision {}, {} files, {}.",
            report.revision,
            report.files,
            output::bytes(report.total_size)
        );
    }
    Ok(crate::exit::OK)
}

/// Strip the `\\?\` prefix Windows canonicalization adds.
fn dunce_simplify(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\")
        .map(str::to_owned)
        .unwrap_or_else(|| s.into_owned())
}

fn toml_from_str(s: &str) -> anyhow::Result<BuildConfig> {
    Ok(toml::from_str(s)?)
}

fn toml_to_string(c: &BuildConfig) -> anyhow::Result<String> {
    Ok(toml::to_string_pretty(c)?)
}
