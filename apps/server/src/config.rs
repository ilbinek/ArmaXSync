//! Server configuration (`config.toml` + `ARMAXSYNC_*` environment overrides).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Address for HTTP (repositories, API, admin UI).
    pub bind: SocketAddr,
    /// Public base URL as seen by clients, e.g. `https://mods.example.com`.
    /// Used in autoconfig files, zsync headers and links.
    pub public_url: String,
    /// Data folder (database, repositories, staging, uploads).
    pub data_dir: PathBuf,
    /// Reverse proxies whose `X-Forwarded-For` is trusted.
    pub trusted_proxies: Vec<String>,
    /// Global download bandwidth limit in bytes/s (0 = unlimited).
    pub bandwidth_limit: u64,
    /// Maximum concurrent file downloads across all repositories (0 = unlimited).
    pub max_downloads: u32,
    /// Maximum size of a single upload in bytes.
    pub max_upload_size: u64,
    /// Heavy jobs (builds, mirrors, publishes) running in parallel.
    pub job_workers: usize,
    /// Expose `/metrics` (Prometheus). Optional bearer token.
    pub metrics: bool,
    pub metrics_token: Option<String>,
    /// Session lifetime in hours.
    pub session_hours: u64,
    /// Legacy URL aliases: path prefix → repository slug.
    pub aliases: Vec<Alias>,
    /// Serve the admin UI from this folder instead of the embedded copy (dev).
    pub ui_dir: Option<PathBuf>,
    /// Release checks and self-update.
    pub updates: crate::updates::UpdateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub path: String,
    pub repo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".parse().unwrap(),
            public_url: "http://localhost:8080".into(),
            data_dir: PathBuf::from("data"),
            trusted_proxies: vec!["127.0.0.1".into(), "::1".into()],
            bandwidth_limit: 0,
            max_downloads: 0,
            max_upload_size: 64 << 30,
            job_workers: 2,
            metrics: true,
            metrics_token: None,
            session_hours: 24 * 14,
            aliases: vec![],
            ui_dir: None,
            updates: Default::default(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut c: Config = match path {
            Some(p) => {
                let s = std::fs::read_to_string(p)
                    .map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
                toml::from_str(&s).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?
            }
            None => Config::default(),
        };
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if let Some(v) = env("ARMAXSYNC_BIND") {
            c.bind = v.parse()?;
        }
        if let Some(v) = env("ARMAXSYNC_PUBLIC_URL") {
            c.public_url = v;
        }
        if let Some(v) = env("ARMAXSYNC_DATA_DIR") {
            c.data_dir = v.into();
        }
        if let Some(v) = env("ARMAXSYNC_BANDWIDTH_LIMIT") {
            c.bandwidth_limit = v.parse()?;
        }
        if let Some(v) = env("ARMAXSYNC_METRICS_TOKEN") {
            c.metrics_token = Some(v);
        }
        c.public_url = c.public_url.trim_end_matches('/').to_owned();
        Ok(c)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("armaxsync.db")
    }
    pub fn repos_dir(&self) -> PathBuf {
        self.data_dir.join("repos")
    }
    pub fn staging_dir(&self) -> PathBuf {
        self.data_dir.join("staging")
    }
    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }
    pub fn trash_dir(&self) -> PathBuf {
        self.data_dir.join("trash")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }
    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }

    /// Public base URL of a repository.
    pub fn repo_url(&self, slug: &str) -> String {
        format!("{}/r/{slug}", self.public_url)
    }
}

pub const EXAMPLE: &str = r#"# ArmaXSync server configuration
bind = "0.0.0.0:8080"
# URL clients use to reach this server (put your reverse proxy's https URL here)
public_url = "https://mods.example.com"
data_dir = "/var/lib/armaxsync"
trusted_proxies = ["127.0.0.1", "::1"]
# bytes per second, 0 = unlimited
bandwidth_limit = 0
max_downloads = 0
job_workers = 2
metrics = true
# metrics_token = "secret"

# New releases are announced in the admin UI. Under systemd (deb/rpm) the
# owner can install them with one click; Docker and manual installs only get
# the notice.
[updates]
check = true
interval_hours = 6
# Install new releases automatically, at most once per window (server time).
auto_install = false
auto_install_cron = "0 4 * * *"

# Keep old repository URLs working after migrating to ArmaXSync:
# [[aliases]]
# path = "/arma3/main"
# repo = "main"
"#;
