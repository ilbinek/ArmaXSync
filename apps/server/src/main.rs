//! `armaxsync-server` — repository hosting with a web administration UI.

use std::path::PathBuf;

use armaxsync_server::auth::{self, Role};
use armaxsync_server::{build_state, config, crypto, serve};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "armaxsync-server",
    version,
    about = "ArmaXSync repository server"
)]
struct Cli {
    /// Configuration file (TOML). Environment: ARMAXSYNC_CONFIG.
    #[arg(short, long, global = true, env = "ARMAXSYNC_CONFIG")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the server (default).
    Serve,
    /// Create the owner account.
    Init {
        #[arg(long)]
        username: String,
        #[arg(long, env = "ARMAXSYNC_OWNER_PASSWORD", hide_env_values = true)]
        password: String,
    },
    /// Manage users.
    #[command(subcommand)]
    User(UserCmd),
    /// Print an example configuration file.
    ExampleConfig,
}

#[derive(Subcommand)]
enum UserCmd {
    List,
    Add {
        username: String,
        #[arg(long, env = "ARMAXSYNC_USER_PASSWORD", hide_env_values = true)]
        password: String,
        #[arg(long, default_value = "user")]
        role: String,
    },
    Passwd {
        username: String,
        #[arg(long, env = "ARMAXSYNC_USER_PASSWORD", hide_env_values = true)]
        password: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ARMAXSYNC_LOG")
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();
    let cli = Cli::parse();
    if let Some(Cmd::ExampleConfig) = cli.cmd {
        print!("{}", config::EXAMPLE);
        return Ok(());
    }
    let cfg = config::Config::load(cli.config.as_deref())?;
    let st = build_state(cfg).await?;
    match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Serve => serve(st).await,
        Cmd::Init { username, password } => {
            let n: i64 = st.db.with(|c| {
                c.query_row("SELECT COUNT(*) FROM users WHERE role='owner'", [], |r| {
                    r.get(0)
                })
            })?;
            anyhow::ensure!(n == 0, "an owner already exists");
            auth::create_user(&st, &username, &password, Role::Owner).await?;
            println!("owner {username} created");
            Ok(())
        }
        Cmd::User(UserCmd::List) => {
            let rows: Vec<(String, String, bool)> = st.db.with(|c| {
                let mut s =
                    c.prepare("SELECT username, role, disabled FROM users ORDER BY username")?;
                let r =
                    s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)))?;
                r.collect()
            })?;
            for (u, r, d) in rows {
                println!("{u:<24} {r:<6}{}", if d { " (disabled)" } else { "" });
            }
            Ok(())
        }
        Cmd::User(UserCmd::Add {
            username,
            password,
            role,
        }) => {
            let role = Role::parse(&role);
            auth::create_user(&st, &username, &password, role).await?;
            println!("user {username} created");
            Ok(())
        }
        Cmd::User(UserCmd::Passwd { username, password }) => {
            anyhow::ensure!(
                password.len() >= 8,
                "password must be at least 8 characters"
            );
            let h = crypto::hash_password(&password);
            let n = st.db.with(|c| {
                c.execute(
                    "UPDATE users SET password_hash=?1 WHERE username=?2",
                    rusqlite::params![h, username],
                )
            })?;
            anyhow::ensure!(n == 1, "no such user");
            println!("password updated");
            Ok(())
        }
        Cmd::ExampleConfig => unreachable!(),
    }
}
