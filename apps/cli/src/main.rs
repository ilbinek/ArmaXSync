//! `armaxsync` — command line client.

mod cmd_build;
mod cmd_game;
mod cmd_inspect;
mod cmd_misc;
mod cmd_push;
mod cmd_repo;
mod cmd_sync;
mod legacy_args;
mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use axs_client::{Client, DataDirs};

/// Exit codes (stable, for scripts).
pub mod exit {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const NOT_FOUND: i32 = 3;
    pub const NETWORK: i32 = 4;
    pub const VERIFY_FAILED: i32 = 5;
    pub const UPDATES_AVAILABLE: i32 = 10;
}

#[derive(Parser)]
#[command(
    name = "armaxsync",
    version,
    about = "Sync, build and launch Arma mod repositories"
)]
pub struct Cli {
    /// Machine-readable JSON lines on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// More logging (-v, -vv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    /// Suppress progress output.
    #[arg(short, long, global = true)]
    quiet: bool,
    /// Configuration/data folder (default: OS location or portable mode).
    #[arg(long, global = true, env = "ARMAXSYNC_HOME")]
    home: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage registered repositories.
    #[command(subcommand)]
    Repo(cmd_repo::RepoCmd),
    /// Check repositories for updates.
    Check(cmd_sync::CheckArgs),
    /// Download a repository into its destination folder.
    Sync(cmd_sync::SyncArgs),
    /// Rehash the destination and compare with the repository.
    Verify(cmd_sync::VerifyArgs),
    /// List a repository's events.
    Events(cmd_misc::NameArg),
    /// Show a repository's changelog.
    Changelog(cmd_misc::NameArg),
    /// Build (or rebuild) a repository from a folder of mods.
    Build(cmd_build::BuildArgs),
    /// Start Arma 3 with the active (or given) profile.
    Launch(cmd_game::LaunchArgs),
    /// Manage launcher profiles (addon groups, options, executable).
    #[command(subcommand)]
    Profile(cmd_game::ProfileCmd),
    /// List the addons found for a profile.
    Addons {
        #[arg(long)]
        profile: Option<String>,
    },
    /// Add/refresh a repository or event modset in a profile.
    Modset(cmd_game::ModsetArgs),
    /// Favorite servers.
    #[command(subcommand)]
    Servers(cmd_game::ServerCmd),
    /// Prepare a dedicated server folder (mods, keys, -mod/-serverMod line).
    ServerHelper(cmd_game::ServerHelperArgs),
    /// Show (and follow) the newest Arma 3 RPT log.
    Rpt(cmd_game::RptArgs),
    /// Upload a local folder to an ArmaXSync server repository (delta push).
    Push(cmd_push::PushArgs),
    /// Check that every file of a repository exists on its server.
    CheckRemote(cmd_misc::NameArg),
    /// Copy all *.bikey files from a folder tree into one folder.
    ExtractKeys(cmd_misc::ExtractArgs),
    /// Import repositories from a legacy ArmA3Sync installation.
    ImportLegacy(cmd_misc::ImportArgs),
    /// Decode a legacy .a3s file, .zsync file or ArmaXSync manifest.
    Inspect(cmd_inspect::InspectArgs),
}

pub struct Ctx {
    pub json: bool,
    pub quiet: bool,
    pub home: Option<PathBuf>,
}

impl Ctx {
    pub fn client(&self) -> anyhow::Result<Client> {
        let dirs = match &self.home {
            Some(h) => DataDirs::at(h),
            None => DataDirs::resolve(),
        };
        Ok(Client::open(dirs)?)
    }
}

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let args = match legacy_args::translate(&args) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(exit::USAGE);
        }
    };
    let cli = Cli::parse_from(args);
    let level = match cli.verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ARMAXSYNC_LOG")
                .unwrap_or_else(|_| level.into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let ctx = Ctx {
        json: cli.json,
        quiet: cli.quiet || cli.json,
        home: cli.home.clone(),
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = match rt.block_on(run(cli.cmd, &ctx)) {
        Ok(code) => code,
        Err(e) => {
            let code = classify(&e);
            if ctx.json {
                output::json_line(&serde_json::json!({"error": format!("{e:#}"), "code": code}));
            } else {
                eprintln!("error: {e:#}");
            }
            code
        }
    };
    std::process::exit(code);
}

fn classify(e: &anyhow::Error) -> i32 {
    if let Some(c) = e.downcast_ref::<axs_client::ClientError>() {
        return match c {
            axs_client::ClientError::NotFound(_) => exit::NOT_FOUND,
            c if c.is_network() => exit::NETWORK,
            _ => exit::ERROR,
        };
    }
    if e.downcast_ref::<axs_transport::TransportError>().is_some()
        || e.downcast_ref::<axs_sync::SyncError>().is_some()
    {
        return exit::NETWORK;
    }
    exit::ERROR
}

async fn run(cmd: Command, ctx: &Ctx) -> anyhow::Result<i32> {
    match cmd {
        Command::Build(a) => cmd_build::run(a, ctx),
        Command::Inspect(a) => cmd_inspect::run(a, ctx),
        Command::ExtractKeys(a) => cmd_misc::extract_keys(a, ctx),
        Command::Repo(c) => cmd_repo::run(c, &mut ctx.client()?, ctx).await,
        Command::Check(a) => cmd_sync::check(a, &mut ctx.client()?, ctx).await,
        Command::Sync(a) => cmd_sync::sync(a, &mut ctx.client()?, ctx).await,
        Command::Verify(a) => cmd_sync::verify(a, &mut ctx.client()?, ctx).await,
        Command::Events(a) => cmd_misc::events(a, &ctx.client()?, ctx).await,
        Command::Changelog(a) => cmd_misc::changelog(a, &ctx.client()?, ctx).await,
        Command::CheckRemote(a) => cmd_misc::check_remote(a, &ctx.client()?, ctx).await,
        Command::Push(a) => cmd_push::run(a, ctx).await,
        Command::Launch(a) => cmd_game::launch(a, ctx).await,
        Command::Profile(c) => cmd_game::profile(c, ctx),
        Command::Addons { profile } => cmd_game::addons(profile, ctx),
        Command::Modset(a) => cmd_game::modset(a, ctx).await,
        Command::Servers(c) => cmd_game::servers(c, ctx),
        Command::ServerHelper(a) => cmd_game::server_helper(a, ctx),
        Command::Rpt(a) => cmd_game::rpt(a, ctx),
        Command::ImportLegacy(a) => cmd_misc::import_legacy(a, &mut ctx.client()?, ctx),
    }
}
