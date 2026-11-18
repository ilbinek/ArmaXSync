//! Client services shared by the CLI and the GUI: configuration, the
//! repository registry, status checks and synchronization.

pub mod config;
pub mod dirs;
pub mod game;
pub mod legacy_import;
pub mod push;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use axs_build::FileHashCache;
use axs_sync::{
    CancellationToken, EventFn, ExecOptions, ExecReport, PlanOptions, RemoteSnapshot, RepoStatus,
    SourceKind, SyncPlan,
};
use axs_transport::{RateLimiter, Transport};

pub use config::{AdminSettings, AppConfig, RepoConfig, RepoState};
pub use dirs::DataDirs;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("repository {0:?} not found")]
    NotFound(String),
    #[error("repository {0:?} already exists")]
    Exists(String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Sync(#[from] axs_sync::SyncError),
    #[error(transparent)]
    Transport(#[from] axs_transport::TransportError),
    #[error(transparent)]
    Build(#[from] axs_build::BuildError),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

impl ClientError {
    pub fn is_network(&self) -> bool {
        matches!(
            self,
            ClientError::Transport(_) | ClientError::Sync(axs_sync::SyncError::Transport(_))
        )
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Outcome of a status check.
#[derive(Debug, Clone, Serialize)]
pub struct CheckOutcome {
    pub name: String,
    pub status: RepoStatus,
    pub local_revision: Option<u32>,
    pub remote_revision: Option<u32>,
    pub source: Option<SourceKind>,
    pub error: Option<String>,
    #[serde(skip)]
    pub snapshot: Option<RemoteSnapshot>,
}

/// What to sync.
#[derive(Debug, Clone, Default)]
pub struct SyncRequest {
    pub destination: Option<PathBuf>,
    pub event: Option<String>,
    pub exact_match: Option<bool>,
    /// Ignore the hash cache and rehash all local files.
    pub deep: bool,
    pub dry_run: bool,
    /// Skip deletions.
    pub no_delete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncOutcome {
    pub name: String,
    pub revision: u32,
    pub destination: PathBuf,
    pub plan: SyncPlan,
    pub report: Option<ExecReport>,
}

/// The client: configuration plus repository registry.
pub struct Client {
    pub dirs: DataDirs,
    pub config: AppConfig,
    repos: Vec<RepoConfig>,
}

impl Client {
    pub fn open(dirs: DataDirs) -> Result<Self> {
        std::fs::create_dir_all(&dirs.config)?;
        let config: AppConfig = config::read_toml(&dirs.config_file())?;
        let mut repos = vec![];
        if let Ok(rd) = std::fs::read_dir(dirs.repositories()) {
            for e in rd.flatten() {
                if e.path().extension().and_then(|x| x.to_str()) == Some("toml") {
                    let r: RepoConfig = config::read_toml(&e.path())?;
                    if !r.name.is_empty() {
                        repos.push(r);
                    }
                }
            }
        }
        repos.sort_by_key(|r| r.name.to_lowercase());
        Ok(Self {
            dirs,
            config,
            repos,
        })
    }

    pub fn save_config(&self) -> Result<()> {
        config::write_toml(&self.dirs.config_file(), &self.config)
    }

    pub fn repos(&self) -> &[RepoConfig] {
        &self.repos
    }

    pub fn repo(&self, name: &str) -> Result<&RepoConfig> {
        self.repos
            .iter()
            .find(|r| r.name == name)
            .or_else(|| {
                self.repos
                    .iter()
                    .find(|r| r.name.eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| ClientError::NotFound(name.to_owned()))
    }

    fn repo_path(&self, name: &str) -> PathBuf {
        self.dirs.repositories().join(config::repo_file_name(name))
    }

    /// Insert or replace a repository and persist it.
    pub fn save_repo(&mut self, r: RepoConfig) -> Result<()> {
        if r.name.trim().is_empty()
            || r.name
                .chars()
                .any(|c| matches!(c, '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|'))
        {
            return Err(ClientError::Invalid(format!(
                "invalid repository name {:?}",
                r.name
            )));
        }
        config::write_toml(&self.repo_path(&r.name), &r)?;
        match self.repos.iter_mut().find(|x| x.name == r.name) {
            Some(x) => *x = r,
            None => {
                self.repos.push(r);
                self.repos.sort_by_key(|r| r.name.to_lowercase());
            }
        }
        Ok(())
    }

    pub fn add_repo(&mut self, r: RepoConfig) -> Result<()> {
        if self
            .repos
            .iter()
            .any(|x| x.name.eq_ignore_ascii_case(&r.name))
        {
            return Err(ClientError::Exists(r.name));
        }
        self.save_repo(r)
    }

    pub fn rename_repo(&mut self, old: &str, new: &str) -> Result<()> {
        if old != new && self.repos.iter().any(|x| x.name.eq_ignore_ascii_case(new)) {
            return Err(ClientError::Exists(new.into()));
        }
        let mut r = self.repo(old)?.clone();
        let old_name = r.name.clone();
        r.name = new.to_owned();
        self.save_repo(r)?;
        if old_name != new {
            self.remove_repo(&old_name)?;
        }
        Ok(())
    }

    pub fn remove_repo(&mut self, name: &str) -> Result<()> {
        let real = self.repo(name)?.name.clone();
        let p = self.repo_path(&real);
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.repos.retain(|r| r.name != real);
        Ok(())
    }

    /// Create a repository entry from an autoconfig URL, a repository base URL
    /// or an `armaxsync://add?url=…&name=…` deep link. Not saved yet.
    pub async fn resolve_url(
        &self,
        input: &str,
        login: Option<&str>,
        password: Option<&str>,
    ) -> Result<RepoConfig> {
        let (url, name_hint) =
            parse_deep_link(input).unwrap_or_else(|| (input.trim().to_owned(), None));
        let lower = url.to_lowercase();
        let mut r = RepoConfig {
            destination: self.config.default_destination.clone(),
            ..Default::default()
        };
        if lower.ends_with("/autoconfig") {
            // Legacy auto-config: credentials and canonical URL come from it.
            let (ac, _ep) = axs_sync::fetch_autoconfig(&url).await?;
            let p = ac
                .protocole
                .clone()
                .ok_or_else(|| ClientError::Invalid("autoconfig has no protocol".into()))?;
            r.url = axs_legacy::urls::base_url(&p);
            r.login = p
                .login
                .clone()
                .filter(|l| !l.eq_ignore_ascii_case("anonymous"));
            r.password = p.password.clone().filter(|p| !p.is_empty());
            r.name = ac.repository_name.clone().unwrap_or_default();
        } else {
            let mut ep = axs_transport::Endpoint::parse(&url, login, password)?;
            ep.proxy = self.config.proxy.clone();
            r.url = ep.base_url();
            let t = Transport::new(ep)?;
            let snap = axs_sync::fetch_snapshot(&t, true).await?;
            r.name = snap.manifest.name.clone();
            r.game = snap.manifest.game;
            if let Some(ac) = &snap.autoconfig {
                if let Some(p) = &ac.protocole {
                    if login.is_none() {
                        r.login = p
                            .login
                            .clone()
                            .filter(|l| !l.eq_ignore_ascii_case("anonymous"));
                        r.password = p.password.clone().filter(|p| !p.is_empty());
                    }
                }
            }
        }
        if login.is_some() {
            r.login = login.map(str::to_owned);
            r.password = password.map(str::to_owned);
        }
        if let Some(n) = name_hint {
            r.name = n;
        }
        if r.name.trim().is_empty() {
            r.name = r
                .url
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or("repository")
                .to_owned();
        }
        // Unique name.
        let base = r.name.clone();
        let mut i = 2;
        while self
            .repos
            .iter()
            .any(|x| x.name.eq_ignore_ascii_case(&r.name))
        {
            r.name = format!("{base} ({i})");
            i += 1;
        }
        Ok(r)
    }

    pub fn transport(&self, r: &RepoConfig) -> Result<Transport> {
        Ok(Transport::new(r.endpoint(self.config.proxy.as_deref())?)?)
    }

    /// Check one repository and record the result.
    pub async fn check(&mut self, name: &str) -> Result<CheckOutcome> {
        let r = self.repo(name)?.clone();
        let res = async {
            let t = self.transport(&r)?;
            Ok::<_, ClientError>(axs_sync::fetch_snapshot(&t, r.prefer_modern).await?)
        }
        .await;
        let mut r2 = r.clone();
        r2.state.last_check = Some(now_ms());
        let out = match res {
            Ok(snap) => {
                let status = axs_sync::repo_status(r.state.revision, &snap.manifest);
                r2.state.status = status;
                r2.state.remote_revision = Some(snap.manifest.revision);
                r2.state.last_error = None;
                CheckOutcome {
                    name: r.name.clone(),
                    status,
                    local_revision: r.state.revision,
                    remote_revision: Some(snap.manifest.revision),
                    source: Some(snap.source),
                    error: None,
                    snapshot: Some(snap),
                }
            }
            Err(e) => {
                r2.state.status = RepoStatus::Error;
                r2.state.last_error = Some(e.to_string());
                CheckOutcome {
                    name: r.name.clone(),
                    status: RepoStatus::Error,
                    local_revision: r.state.revision,
                    remote_revision: r.state.remote_revision,
                    source: None,
                    error: Some(e.to_string()),
                    snapshot: None,
                }
            }
        };
        self.save_repo(r2)?;
        Ok(out)
    }

    /// Plan and (unless `dry_run`) execute a sync.
    pub async fn sync(
        &mut self,
        name: &str,
        req: &SyncRequest,
        events: EventFn,
        cancel: CancellationToken,
    ) -> Result<SyncOutcome> {
        let r = self.repo(name)?.clone();
        let dest = req
            .destination
            .clone()
            .or_else(|| r.destination.clone())
            .ok_or_else(|| {
                ClientError::Invalid(format!("no destination folder set for {:?}", r.name))
            })?;
        std::fs::create_dir_all(&dest)?;
        let t = self.transport(&r)?;
        let snap = axs_sync::fetch_snapshot(&t, r.prefer_modern).await?;
        let selection = match &req.event {
            Some(ev) => Some(
                axs_sync::event_selection(&snap.manifest, ev)
                    .ok_or_else(|| ClientError::Invalid(format!("event {ev:?} not found")))?,
            ),
            None => None,
        };
        let popts = PlanOptions {
            selection,
            exact_match: req.exact_match.unwrap_or(r.exact_match) && req.event.is_none(),
            keep_extras: r.keep_extras.clone(),
            case_insensitive: cfg!(windows),
        };
        let mut cache = if req.deep {
            FileHashCache::in_memory()
        } else {
            FileHashCache::load(self.dirs.hash_cache())
        };
        let (m, d, p2) = (snap.manifest.clone(), dest.clone(), popts.clone());
        let (local, cache) = tokio::task::spawn_blocking(move || {
            let l = axs_sync::snapshot_local(&d, &m, &p2, &mut cache);
            (l, cache)
        })
        .await
        .map_err(|e| ClientError::Io(std::io::Error::other(e)))?;
        let local = local?;
        if !req.deep {
            let mut cache = cache;
            let _ = axs_build::HashCache::flush(&mut cache);
        }
        let mut plan = axs_sync::plan(&snap.manifest, &local, &popts);
        if req.no_delete {
            plan.ops
                .retain(|o| !matches!(o, axs_sync::Op::Delete { .. }));
            plan.delete_count = 0;
        }
        let mut outcome = SyncOutcome {
            name: r.name.clone(),
            revision: snap.manifest.revision,
            destination: dest.clone(),
            plan,
            report: None,
        };
        if req.dry_run {
            return Ok(outcome);
        }
        let server_conns = snap.manifest.connections.max(1) as usize;
        let conns = if r.connections > 0 {
            (r.connections as usize).min(server_conns)
        } else {
            server_conns
        };
        let speed = if r.max_speed > 0 {
            r.max_speed
        } else {
            self.config.max_speed
        };
        let eo = ExecOptions {
            connections: conns,
            limiter: (speed > 0).then(|| Arc::new(RateLimiter::new(speed))),
            ranges: snap.ranges_supported,
            compressed_only: snap.manifest.compressed_only,
            segment_threshold: if snap.source == SourceKind::Modern {
                self.config.segment_threshold
            } else {
                0
            },
            ..Default::default()
        };
        let report = axs_sync::execute(&outcome.plan, &t, &dest, &eo, events, cancel).await;
        if report.ok() {
            let mut r2 = self.repo(&r.name)?.clone();
            r2.state.last_sync = Some(now_ms());
            if req.event.is_none() {
                r2.state.revision = Some(snap.manifest.revision);
                r2.state.status = RepoStatus::Ok;
            }
            r2.state.remote_revision = Some(snap.manifest.revision);
            self.save_repo(r2)?;
        }
        outcome.report = Some(report);
        Ok(outcome)
    }

    /// Build a repository administered from this machine (legacy `-build`).
    pub fn build_admin(
        &mut self,
        name: &str,
        progress: &(dyn Fn(axs_build::BuildEvent) + Sync),
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<axs_build::BuildReport> {
        let r = self.repo(name)?.clone();
        let a = r.admin.clone().ok_or_else(|| {
            ClientError::Invalid(format!(
                "{:?} has no local build settings (repo admin)",
                r.name
            ))
        })?;
        let path = a.path.clone().ok_or_else(|| {
            ClientError::Invalid("repository main folder location is empty".into())
        })?;
        let opts = admin_build_options(&r, &a, &path)?;
        let mut cache = FileHashCache::load(path.join(".axs").join("build-cache.zst"));
        Ok(axs_build::build(&opts, &mut cache, progress, cancel)?)
    }
}

/// Build options derived from a repository's admin settings.
pub fn admin_build_options(
    r: &RepoConfig,
    a: &AdminSettings,
    path: &Path,
) -> Result<axs_build::BuildOptions> {
    let mut o = axs_build::BuildOptions::new(path, &r.name);
    let url = a.public_url.clone().unwrap_or_else(|| r.url.clone());
    o.protocol =
        axs_legacy::urls::protocol_from_url(&url, r.login.as_deref(), r.password.as_deref());
    o.game = r.game;
    o.embed_credentials = a.embed_credentials;
    o.compress_pbo = a.compress;
    o.compressed_pbo_only = a.compressed_only;
    o.connections = a.connections.clamp(1, 10);
    o.partial_transfer = a.partial_transfer;
    o.excluded_files = a.exclude.clone();
    o.hidden_paths = a.hidden.clone();
    o.favorite_servers = a.servers.iter().map(|s| s.to_legacy(&r.name)).collect();
    o.emit_legacy = r.game.supports_legacy();
    Ok(o)
}

/// Parse `armaxsync://add?url=…&name=…`.
pub fn parse_deep_link(s: &str) -> Option<(String, Option<String>)> {
    let rest = s.trim().strip_prefix("armaxsync://")?;
    let (_, query) = rest.split_once('?')?;
    let mut url = None;
    let mut name = None;
    for kv in query.split('&') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let v = percent_decode(v);
        match k {
            "url" => url = Some(v),
            "name" => name = Some(v).filter(|n| !n.is_empty()),
            _ => {}
        }
    }
    Some((url?, name))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                match h {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links() {
        let (u, n) = parse_deep_link(
            "armaxsync://add?url=https%3A%2F%2Fmods.example.com%2Fr%2Fmain&name=Main+Repo",
        )
        .unwrap();
        assert_eq!(u, "https://mods.example.com/r/main");
        assert_eq!(n.as_deref(), Some("Main Repo"));
        assert!(parse_deep_link("https://x").is_none());
    }

    #[test]
    fn registry_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Client::open(DataDirs::at(dir.path())).unwrap();
        let r = RepoConfig {
            name: "My Repo".into(),
            url: "https://h/r/x".into(),
            ..Default::default()
        };
        c.add_repo(r.clone()).unwrap();
        assert!(c.add_repo(r).is_err());
        let c2 = Client::open(DataDirs::at(dir.path())).unwrap();
        assert_eq!(c2.repo("my repo").unwrap().url, "https://h/r/x");
        let mut c2 = c2;
        c2.rename_repo("My Repo", "Other").unwrap();
        c2.remove_repo("other").unwrap();
        assert!(Client::open(DataDirs::at(dir.path()))
            .unwrap()
            .repos()
            .is_empty());
    }
}
