//! ArmaXSync repository server (library part; the binary is `armaxsync-server`).

pub mod api;
pub mod audit;
pub mod auth;
pub mod changes;
pub mod config;
pub mod crypto;
pub mod db;
pub mod files;
pub mod jobs;
pub mod mirror;
pub mod publish;
pub mod push;
pub mod repos_api;
pub mod scheduler;
pub mod state;
pub mod stats;
pub mod updates;
pub mod web;

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use crate::state::{AppState, Shared};

/// First-run setup token (printed when no user exists).
pub static SETUP_TOKEN: OnceLock<String> = OnceLock::new();

pub async fn build_state(cfg: config::Config) -> anyhow::Result<Shared> {
    for d in [
        cfg.repos_dir(),
        cfg.staging_dir(),
        cfg.uploads_dir(),
        cfg.trash_dir(),
        cfg.cache_dir(),
    ] {
        std::fs::create_dir_all(&d)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", d.display()))?;
    }
    let db_path = cfg.db_path();
    let db = db::Db::open(&db_path)
        .map_err(|e| anyhow::anyhow!("cannot open database {}: {e:#}", db_path.display()))?;
    let key_path = cfg.data_dir.join("secret.key");
    let secrets = crypto::SecretBox::load_or_create(&key_path)
        .map_err(|e| anyhow::anyhow!("cannot read or create {}: {e:#}", key_path.display()))?;
    let max = if cfg.max_downloads == 0 {
        state::MAX_PERMITS
    } else {
        cfg.max_downloads as usize
    };
    let salt = stats::load_salt(&db)?;
    let st = Arc::new(AppState {
        stats: Arc::new(stats::collect::Collector::new(cfg.stats.players, salt)),
        global_limiter: Arc::new(axs_transport::RateLimiter::new(cfg.bandwidth_limit)),
        global_downloads: Arc::new(tokio::sync::Semaphore::new(max)),
        jobs: jobs::JobManager::new(cfg.job_workers),
        updates: updates::Updates::new(&cfg.updates),
        cfg,
        db,
        secrets,
        repos: Default::default(),
        started: db::now(),
    });
    jobs::JobManager::recover(&st)?;
    st.load_repos().await?;
    Ok(st)
}

/// Run the HTTP server until Ctrl-C.
pub async fn serve(st: Shared) -> anyhow::Result<()> {
    let users: i64 = st
        .db
        .with(|c| c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)))?;
    if users == 0 {
        let tok = crypto::random_token(12);
        let _ = SETUP_TOKEN.set(tok.clone());
        tracing::warn!("no users yet: open {}/admin/ and use setup token {tok} (or run `armaxsync-server init`)", st.cfg.public_url);
    }
    publish::recover_journals(&st).await?;
    scheduler::spawn(st.clone());
    updates::spawn(st.clone());
    let app = api::router(st.clone()).layer(
        tower_http::trace::TraceLayer::new_for_http()
            .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::DEBUG))
            .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::DEBUG)),
    );
    let listener = tokio::net::TcpListener::bind(st.cfg.bind).await?;
    tracing::info!(
        "armaxsync-server {} listening on {} (public url {})",
        env!("CARGO_PKG_VERSION"),
        st.cfg.bind,
        st.cfg.public_url
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        shutdown_signal().await;
        tracing::info!("shutting down");
    })
    .await?;
    let _ = stats::flush(&st).await;
    Ok(())
}

/// Ctrl-C, or SIGTERM (systemd stop/restart, `docker stop`).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
