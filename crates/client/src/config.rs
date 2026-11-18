//! Client configuration files (TOML).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use axs_model::GameId;
use axs_sync::RepoStatus;

/// `config.toml`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Default destination folder for new repositories.
    pub default_destination: Option<PathBuf>,
    /// Minutes between repository checks while the GUI is open (0 = startup only).
    pub check_interval_minutes: u32,
    /// Proxy URL for all connections (`http://`, `socks5://` …).
    pub proxy: Option<String>,
    /// Global download speed limit in bytes/s (0 = unlimited).
    pub max_speed: u64,
    /// Files at least this large use parallel range requests (0 = off).
    pub segment_threshold: u64,
    /// Name of the active launcher profile.
    pub profile: Option<String>,
    pub favorite_servers: Vec<axs_launcher::FavoriteServer>,
    pub external_apps: Vec<axs_launcher::ExternalApp>,
    /// What the GUI does once the game started.
    pub at_launch: AtLaunch,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AtLaunch {
    #[default]
    Nothing,
    Minimize,
    Close,
}

/// Local build settings for repositories administered from this machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AdminSettings {
    /// Folder holding the repository content.
    pub path: Option<PathBuf>,
    /// Public base URL (defaults to the repository URL).
    pub public_url: Option<String>,
    pub compress: bool,
    pub compressed_only: bool,
    pub connections: u32,
    pub partial_transfer: bool,
    pub embed_credentials: bool,
    pub exclude: Vec<String>,
    pub hidden: Vec<String>,
    pub servers: Vec<axs_model::ServerDef>,
    /// ArmaXSync server to push to (`https://host`), its repository slug and an API token.
    pub push_server: Option<String>,
    pub push_repo: Option<String>,
    pub push_token: Option<String>,
}

/// Mutable sync state of a repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RepoState {
    /// Revision of the last successful full sync.
    pub revision: Option<u32>,
    pub remote_revision: Option<u32>,
    pub status: RepoStatus,
    pub last_check: Option<i64>,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
}

impl Default for RepoState {
    fn default() -> Self {
        Self {
            revision: None,
            remote_revision: None,
            status: RepoStatus::Unknown,
            last_check: None,
            last_sync: None,
            last_error: None,
        }
    }
}

/// `repositories/<file>.toml`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RepoConfig {
    pub name: String,
    pub game: GameId,
    /// Base URL of the repository (`https://host/path`, `ftp://…`).
    pub url: String,
    pub login: Option<String>,
    pub password: Option<String>,
    /// Destination folder for downloads.
    pub destination: Option<PathBuf>,
    pub exact_match: bool,
    /// Reuse addons already present in other search folders.
    pub auto_discover: bool,
    /// Notify when updates are available.
    pub notify: bool,
    /// Download updates automatically.
    pub auto: bool,
    /// Client-side connection cap (0 = server's value).
    pub connections: u32,
    /// Bytes/s (0 = unlimited).
    pub max_speed: u64,
    pub connect_timeout_secs: Option<u64>,
    pub read_timeout_secs: Option<u64>,
    pub accept_invalid_certs: bool,
    /// Folders whose extra local content is kept.
    pub keep_extras: Vec<String>,
    /// Use the modern manifest when the server publishes one.
    pub prefer_modern: bool,
    pub admin: Option<AdminSettings>,
    pub state: RepoState,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            game: GameId::Arma3,
            url: String::new(),
            login: None,
            password: None,
            destination: None,
            exact_match: false,
            auto_discover: false,
            notify: true,
            auto: false,
            connections: 0,
            max_speed: 0,
            connect_timeout_secs: None,
            read_timeout_secs: None,
            accept_invalid_certs: false,
            keep_extras: vec![],
            prefer_modern: true,
            admin: None,
            state: RepoState::default(),
        }
    }
}

impl RepoConfig {
    pub fn endpoint(&self, proxy: Option<&str>) -> axs_transport::Result<axs_transport::Endpoint> {
        let mut ep = axs_transport::Endpoint::parse(
            &self.url,
            self.login.as_deref(),
            self.password.as_deref(),
        )?;
        ep.connect_timeout = Some(std::time::Duration::from_secs(
            self.connect_timeout_secs.unwrap_or(30),
        ));
        ep.read_timeout = self
            .read_timeout_secs
            .filter(|&s| s > 0)
            .map(std::time::Duration::from_secs)
            .or(Some(std::time::Duration::from_secs(300)));
        ep.accept_invalid_certs = self.accept_invalid_certs;
        ep.proxy = proxy.map(str::to_owned);
        Ok(ep)
    }

    /// Legacy autoconfig URL for this repository.
    pub fn autoconfig_url(&self) -> String {
        format!("{}/.a3s/autoconfig", self.url.trim_end_matches('/'))
    }
}

/// File name for a repository config (stable, filesystem-safe).
pub fn repo_file_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with('.') {
        s.insert(0, 'r');
    }
    format!("{s}.toml")
}

pub(crate) fn read_toml<T: serde::de::DeserializeOwned + Default>(p: &Path) -> crate::Result<T> {
    match std::fs::read_to_string(p) {
        Ok(s) => toml::from_str(&s)
            .map_err(|e| crate::ClientError::Config(format!("{}: {e}", p.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn write_toml<T: Serialize>(p: &Path, v: &T) -> crate::Result<()> {
    let s = toml::to_string_pretty(v).map_err(|e| crate::ClientError::Config(e.to_string()))?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    axs_build::write_atomic(p, s.as_bytes())?;
    restrict_permissions(p);
    Ok(())
}

/// Config files may contain repository passwords.
fn restrict_permissions(_p: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(_p, std::fs::Permissions::from_mode(0o600));
    }
}
