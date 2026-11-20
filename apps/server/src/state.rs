//! Shared server state and the repository model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::Bytes;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};

use axs_legacy::files as a3s;
use axs_model::{EventDef, GameId, ServerDef, AXS_FOLDER, INDEX_FILE};
use axs_transport::RateLimiter;

use crate::config::Config;
use crate::crypto::SecretBox;
use crate::db::Db;
use crate::jobs::JobManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    /// Downloads need one of the repository credentials (HTTP Basic auth).
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepoMode {
    /// Content managed by the server (uploads, file manager, push, mirror).
    #[default]
    Managed,
    /// Content lives in an existing folder edited outside the server.
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MirrorSettings {
    /// Remote repository: base URL or autoconfig URL.
    pub url: String,
    pub login: Option<String>,
    /// Encrypted with the server key.
    pub password_sealed: Option<String>,
    pub import_events: bool,
    pub import_hidden: bool,
    pub import_servers: bool,
    /// Only these top-level folders (empty = everything).
    pub include: Vec<String>,
}

impl Default for MirrorSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            login: None,
            password_sealed: None,
            import_events: true,
            import_hidden: true,
            import_servers: false,
            include: vec![],
        }
    }
}

/// Per-repository settings (stored as JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RepoSettings {
    pub visibility: Visibility,
    pub compress: bool,
    pub compressed_only: bool,
    /// Parallel connections advertised to clients.
    pub connections: u32,
    pub partial_transfer: bool,
    /// Publish one credential inside the legacy autoconfig (legacy behaviour).
    pub embed_credentials: bool,
    pub hidden: Vec<String>,
    pub exclude: Vec<String>,
    pub servers: Vec<ServerDef>,
    pub events: Vec<EventDef>,
    pub mirror: Option<MirrorSettings>,
    /// Bytes/s for this repository (0 = unlimited).
    pub bandwidth_limit: u64,
    /// Concurrent downloads for this repository (0 = unlimited).
    pub max_downloads: u32,
    /// Show on the public landing page.
    pub listed: bool,
}

impl Default for RepoSettings {
    fn default() -> Self {
        Self {
            visibility: Visibility::Public,
            compress: false,
            compressed_only: false,
            connections: 3,
            partial_transfer: true,
            embed_credentials: true,
            hidden: vec![],
            exclude: vec![],
            servers: vec![],
            events: vec![],
            mirror: None,
            bandwidth_limit: 0,
            max_downloads: 0,
            listed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub game: GameId,
    pub mode: RepoMode,
    pub path: Option<PathBuf>,
    pub settings: RepoSettings,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub id: i64,
    pub login: String,
    pub password: String,
    pub embed: bool,
    pub note: String,
}

/// Published metadata served from memory (switched atomically).
#[derive(Debug, Default)]
pub struct Published {
    pub revision: u32,
    pub build_date: i64,
    /// Repository-relative path → bytes (`.a3s/*`, `.axs/index.json`, manifests).
    pub files: HashMap<String, Bytes>,
    pub file_count: usize,
    pub total_size: u64,
    /// Addon folders (lowercase), longest first, for per-addon statistics.
    pub addons: Vec<String>,
}

impl Published {
    /// Load the metadata currently on disk.
    pub fn load(root: &std::path::Path) -> Self {
        let mut p = Published::default();
        for name in [
            a3s::SYNC,
            a3s::SERVERINFO,
            a3s::CHANGELOGS,
            a3s::EVENTS,
            a3s::AUTOCONFIG,
        ] {
            if let Ok(b) = std::fs::read(root.join(a3s::A3S_FOLDER).join(name)) {
                p.files
                    .insert(format!("{}/{name}", a3s::A3S_FOLDER), Bytes::from(b));
            }
        }
        if let Some(si) = p
            .files
            .get(".a3s/serverinfo")
            .and_then(|b| axs_legacy::read_server_info(b).ok())
        {
            p.revision = si.revision.max(0) as u32;
            p.build_date = si.build_date.unwrap_or(0);
            p.file_count = si.number_of_files.max(0) as usize;
            p.total_size = si.total_files_size.max(0) as u64;
        }
        if let Ok(idx) = std::fs::read(root.join(AXS_FOLDER).join(INDEX_FILE)) {
            if let Ok(i) = axs_model::RepoIndex::parse(&idx) {
                if let Ok(m) = std::fs::read(root.join(AXS_FOLDER).join(&i.manifest)) {
                    if let Ok(man) = axs_model::Manifest::from_zst(&m) {
                        p.file_count = man.files.len();
                        p.total_size = man.total_size();
                        p.addons = man.addons.iter().map(|a| a.to_ascii_lowercase()).collect();
                        p.addons.sort_by_key(|a| std::cmp::Reverse(a.len()));
                    }
                    p.files
                        .insert(format!("{AXS_FOLDER}/{}", i.manifest), Bytes::from(m));
                }
                p.revision = p.revision.max(i.revision);
                p.build_date = p.build_date.max(i.build_date);
                p.files
                    .insert(format!("{AXS_FOLDER}/{INDEX_FILE}"), Bytes::from(idx));
            }
        }
        // Older immutable manifests stay downloadable for clients mid-sync.
        if let Ok(rd) = std::fs::read_dir(root.join(AXS_FOLDER)) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                let key = format!("{AXS_FOLDER}/{n}");
                if n.starts_with("manifest.")
                    && n.ends_with(".json.zst")
                    && !p.files.contains_key(&key)
                {
                    if let Ok(b) = std::fs::read(e.path()) {
                        p.files.insert(key, Bytes::from(b));
                    }
                }
            }
        }
        p
    }
}

/// Runtime handles of one repository.
pub struct RepoRuntime {
    pub record: ArcSwap<RepoRecord>,
    pub root: PathBuf,
    pub published: ArcSwap<Published>,
    /// Held exclusively while content is being swapped into place.
    pub commit_lock: RwLock<()>,
    pub limiter: Arc<RateLimiter>,
    pub downloads: Arc<Semaphore>,
    pub bytes_served: AtomicU64,
    pub requests: AtomicU64,
    pub files_served: AtomicU64,
    pub credentials: ArcSwap<Vec<Credential>>,
}

impl RepoRuntime {
    pub fn record(&self) -> Arc<RepoRecord> {
        self.record.load_full()
    }
}

pub struct AppState {
    pub cfg: Config,
    pub db: Db,
    pub secrets: SecretBox,
    pub repos: std::sync::RwLock<HashMap<String, Arc<RepoRuntime>>>,
    pub jobs: JobManager,
    pub global_limiter: Arc<RateLimiter>,
    pub global_downloads: Arc<Semaphore>,
    pub started: i64,
    pub updates: crate::updates::Updates,
    pub stats: Arc<crate::stats::collect::Collector>,
}

pub type Shared = Arc<AppState>;

pub const MAX_PERMITS: usize = Semaphore::MAX_PERMITS;

impl AppState {
    pub fn repo(&self, slug: &str) -> Option<Arc<RepoRuntime>> {
        self.repos.read().unwrap().get(slug).cloned()
    }

    pub fn repo_list(&self) -> Vec<Arc<RepoRuntime>> {
        let mut v: Vec<_> = self.repos.read().unwrap().values().cloned().collect();
        v.sort_by_key(|r| r.record().name.to_lowercase());
        v
    }

    /// Content folder of a repository.
    pub fn repo_root(&self, r: &RepoRecord) -> PathBuf {
        match (r.mode, &r.path) {
            (RepoMode::External, Some(p)) => p.clone(),
            _ => self.cfg.repos_dir().join(&r.slug),
        }
    }

    pub fn cache_path(&self, slug: &str) -> PathBuf {
        self.cfg.cache_dir().join(format!("{slug}.zst"))
    }

    /// Load every repository from the database into memory.
    pub async fn load_repos(&self) -> anyhow::Result<()> {
        let recs = load_records(&self.db).await?;
        let mut map = HashMap::new();
        for r in recs {
            let rt = self.make_runtime(r).await?;
            map.insert(rt.record().slug.clone(), rt);
        }
        *self.repos.write().unwrap() = map;
        Ok(())
    }

    pub async fn make_runtime(&self, r: RepoRecord) -> anyhow::Result<Arc<RepoRuntime>> {
        let root = self.repo_root(&r);
        std::fs::create_dir_all(&root)?;
        let creds = self.load_credentials(r.id).await?;
        let published = Published::load(&root);
        let permits = if r.settings.max_downloads == 0 {
            MAX_PERMITS
        } else {
            r.settings.max_downloads as usize
        };
        Ok(Arc::new(RepoRuntime {
            limiter: Arc::new(RateLimiter::new(r.settings.bandwidth_limit)),
            downloads: Arc::new(Semaphore::new(permits)),
            record: ArcSwap::from_pointee(r),
            root,
            published: ArcSwap::from_pointee(published),
            commit_lock: RwLock::new(()),
            bytes_served: AtomicU64::new(0),
            requests: AtomicU64::new(0),
            files_served: AtomicU64::new(0),
            credentials: ArcSwap::from_pointee(creds),
        }))
    }

    pub async fn load_credentials(&self, repo_id: i64) -> anyhow::Result<Vec<Credential>> {
        let rows: Vec<(i64, String, String, bool, String)> = self
            .db
            .call(move |c| {
                let mut st = c.prepare("SELECT id, login, secret, embed, note FROM repo_credentials WHERE repo_id=?1 ORDER BY id")?;
                let rows = st.query_map([repo_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?)))?;
                rows.collect()
            })
            .await?;
        Ok(rows
            .into_iter()
            .map(|(id, login, secret, embed, note)| Credential {
                id,
                login,
                password: self.secrets.open(&secret).unwrap_or_default(),
                embed,
                note,
            })
            .collect())
    }

    /// Reload a repository's record and credentials from the database.
    pub async fn refresh_repo(&self, slug: &str) -> anyhow::Result<()> {
        let recs = load_records(&self.db).await?;
        let Some(r) = recs.into_iter().find(|r| r.slug == slug) else {
            self.repos.write().unwrap().remove(slug);
            return Ok(());
        };
        match self.repo(slug) {
            Some(rt) => {
                rt.limiter.set_rate(r.settings.bandwidth_limit);
                rt.credentials
                    .store(Arc::new(self.load_credentials(r.id).await?));
                rt.record.store(Arc::new(r));
            }
            None => {
                let rt = self.make_runtime(r).await?;
                self.repos.write().unwrap().insert(slug.to_owned(), rt);
            }
        }
        Ok(())
    }

    /// Build options for a repository (public URL + embedded credential).
    pub fn build_options(&self, rt: &RepoRuntime) -> axs_build::BuildOptions {
        let r = rt.record();
        let s = &r.settings;
        let mut o = axs_build::BuildOptions::new(&rt.root, &r.name);
        o.game = r.game;
        let creds = rt.credentials.load();
        let embed = creds
            .iter()
            .find(|c| c.embed)
            .filter(|_| s.embed_credentials);
        o.protocol = axs_legacy::urls::protocol_from_url(
            &self.cfg.repo_url(&r.slug),
            embed.map(|c| c.login.as_str()),
            embed.map(|c| c.password.as_str()),
        );
        o.embed_credentials = embed.is_some();
        o.compress_pbo = s.compress;
        o.compressed_pbo_only = s.compressed_only;
        o.connections = s.connections.clamp(1, 10);
        o.partial_transfer = s.partial_transfer;
        o.excluded_files = s.exclude.clone();
        o.hidden_paths = s.hidden.clone();
        o.favorite_servers = s.servers.iter().map(|x| x.to_legacy(&r.name)).collect();
        o.events = Some(s.events.clone());
        o.emit_legacy = r.game.supports_legacy();
        o.emit_modern = true;
        o
    }

    pub fn note_served(&self, rt: &RepoRuntime, bytes: u64) {
        rt.bytes_served.fetch_add(bytes, Ordering::Relaxed);
    }
}

pub async fn load_records(db: &Db) -> anyhow::Result<Vec<RepoRecord>> {
    db.call(|c| {
        let mut st = c.prepare("SELECT id, slug, name, description, game, mode, path, settings, created_at FROM repos ORDER BY name")?;
        let rows = st.query_map([], row_to_record)?;
        rows.collect()
    })
    .await
}

pub fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<RepoRecord> {
    let game: String = r.get(4)?;
    let mode: String = r.get(5)?;
    let settings: String = r.get(7)?;
    Ok(RepoRecord {
        id: r.get(0)?,
        slug: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        game: game.parse().unwrap_or_default(),
        mode: if mode == "external" {
            RepoMode::External
        } else {
            RepoMode::Managed
        },
        path: r.get::<_, Option<String>>(6)?.map(PathBuf::from),
        settings: serde_json::from_str(&settings).unwrap_or_default(),
        created_at: r.get(8)?,
    })
}

pub async fn repo_by_slug(db: &Db, slug: &str) -> anyhow::Result<Option<RepoRecord>> {
    let slug = slug.to_owned();
    db.call(move |c| {
        c.query_row(
            "SELECT id, slug, name, description, game, mode, path, settings, created_at FROM repos WHERE slug=?1",
            [slug],
            row_to_record,
        )
        .optional()
    })
    .await
}
