//! ArmaXSync domain model and the modern repository manifest.
//!
//! A repository published by ArmaXSync carries, next to the legacy `.a3s/`
//! folder, an `.axs/` folder:
//!
//! * `.axs/index.json` — small, mutable pointer to the current manifest;
//! * `.axs/manifest.<revision>.json.zst` — immutable, zstd-compressed manifest.
//!
//! Content files and their `.zsync` / `.pbo.zip` sidecars are shared with the
//! legacy layout.

use serde::{Deserialize, Serialize};

pub mod paths;

pub const AXS_FOLDER: &str = ".axs";
pub const INDEX_FILE: &str = "index.json";
pub const INDEX_FORMAT: &str = "armaxsync-index/1";
pub const MANIFEST_FORMAT: &str = "armaxsync-manifest/1";

/// Game a repository targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameId {
    #[default]
    Arma3,
    Reforger,
}

impl GameId {
    /// Whether repositories for this game also publish the legacy `.a3s` format.
    pub fn supports_legacy(self) -> bool {
        matches!(self, GameId::Arma3)
    }
    pub fn steam_app_id(self) -> u32 {
        match self {
            GameId::Arma3 => 107410,
            GameId::Reforger => 1874880,
        }
    }
}

impl std::fmt::Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            GameId::Arma3 => "arma3",
            GameId::Reforger => "reforger",
        })
    }
}

impl std::str::FromStr for GameId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "arma3" | "a3" => Ok(GameId::Arma3),
            "reforger" | "armareforger" => Ok(GameId::Reforger),
            other => Err(format!("unknown game {other:?}")),
        }
    }
}

/// `.axs/index.json`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoIndex {
    pub format: String,
    pub game: GameId,
    pub revision: u32,
    /// Build time, milliseconds since the Unix epoch.
    pub build_date: i64,
    /// File name of the manifest inside `.axs/`.
    pub manifest: String,
    pub manifest_size: u64,
    pub manifest_blake3: String,
}

/// A content file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// `/`-separated path relative to the repository root.
    pub path: String,
    pub size: u64,
    /// Lowercase hex SHA-1 (`"0"` for empty files, as in legacy).
    pub sha1: String,
    pub blake3: String,
    /// A `<path>.zsync` sidecar is published.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub zsync: bool,
    /// Size of the published `<path>.zip` (only for `.pbo` files).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zip_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Addon folder name → optional.
    #[serde(default)]
    pub addons: Vec<EventItem>,
    /// `userconfig` sub-folder name → optional.
    #[serde(default)]
    pub userconfig: Vec<EventItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventItem {
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub revision: u32,
    pub build_date: i64,
    pub content_updated: bool,
    #[serde(default)]
    pub addons: Vec<String>,
    #[serde(default)]
    pub new_addons: Vec<String>,
    #[serde(default)]
    pub updated_addons: Vec<String>,
    #[serde(default)]
    pub deleted_addons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ServerDef {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Modset (repository/event/group name) to join with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modset: Option<String>,
}

/// Mod provenance (used by Reforger/Workshop sources in v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRef {
    /// Folder (relative path) holding the mod in this repository.
    pub path: String,
    pub source: ModSourceRef,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ModSourceRef {
    Local,
    SteamWorkshop { id: u64 },
    ReforgerWorkshop { id: String, version: Option<String> },
}

/// `.axs/manifest.<rev>.json.zst`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub game: GameId,
    pub name: String,
    pub revision: u32,
    pub build_date: i64,
    pub content_updated: bool,
    /// Maximum parallel connections a client should use.
    pub connections: u32,
    /// Server supports HTTP range requests (delta + resume).
    pub partial_transfer: bool,
    /// Only `.pbo.zip` files are published for compressed PBOs.
    #[serde(default)]
    pub compressed_only: bool,
    /// zsync block size used for sidecars.
    pub block_size: u32,
    /// Folders clients should not touch (no extra-file cleanup inside).
    #[serde(default)]
    pub hidden_paths: Vec<String>,
    /// All directories (including empty ones), `/`-separated.
    #[serde(default)]
    pub dirs: Vec<String>,
    pub files: Vec<FileEntry>,
    /// Paths of addon folders (folders containing an `addons` child).
    #[serde(default)]
    pub addons: Vec<String>,
    #[serde(default)]
    pub events: Vec<EventDef>,
    #[serde(default)]
    pub changelogs: Vec<ChangelogEntry>,
    #[serde(default)]
    pub servers: Vec<ServerDef>,
    #[serde(default)]
    pub mods: Vec<ModRef>,
    /// Reserved for a future signature scheme; always absent in v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("unsupported manifest format {0:?}")]
    Format(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest is too large")]
    TooLarge,
}

/// Maximum decompressed manifest size accepted from remote servers.
pub const MAX_MANIFEST_BYTES: usize = 1 << 30;

impl Manifest {
    pub fn file_name(revision: u32) -> String {
        format!("manifest.{revision}.json.zst")
    }

    pub fn to_zst(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).expect("manifest serializes");
        zstd::encode_all(&json[..], 9).expect("in-memory zstd")
    }

    pub fn from_zst(data: &[u8]) -> Result<Self, ManifestError> {
        use std::io::Read;
        let mut dec = zstd::Decoder::new(data)?;
        let mut json = Vec::new();
        (&mut dec)
            .take(MAX_MANIFEST_BYTES as u64 + 1)
            .read_to_end(&mut json)?;
        if json.len() > MAX_MANIFEST_BYTES {
            return Err(ManifestError::TooLarge);
        }
        let m: Manifest = serde_json::from_slice(&json)?;
        if !m.format.starts_with("armaxsync-manifest/") {
            return Err(ManifestError::Format(m.format));
        }
        Ok(m)
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

impl RepoIndex {
    pub fn parse(data: &[u8]) -> Result<Self, ManifestError> {
        let i: RepoIndex = serde_json::from_slice(data)?;
        if !i.format.starts_with("armaxsync-index/") {
            return Err(ManifestError::Format(i.format));
        }
        Ok(i)
    }
}

// ----------------------------------------------------- legacy conversions ----

impl From<&axs_legacy::Changelog> for ChangelogEntry {
    fn from(c: &axs_legacy::Changelog) -> Self {
        Self {
            revision: c.revision.max(0) as u32,
            build_date: c.build_date.unwrap_or(0),
            content_updated: c.content_updated,
            addons: c.addons.clone(),
            new_addons: c.new_addons.clone(),
            updated_addons: c.updated_addons.clone(),
            deleted_addons: c.deleted_addons.clone(),
        }
    }
}

impl From<&ChangelogEntry> for axs_legacy::Changelog {
    fn from(c: &ChangelogEntry) -> Self {
        Self {
            revision: c.revision as i32,
            build_date: Some(c.build_date),
            content_updated: c.content_updated,
            addons: c.addons.clone(),
            new_addons: c.new_addons.clone(),
            updated_addons: c.updated_addons.clone(),
            deleted_addons: c.deleted_addons.clone(),
        }
    }
}

impl From<&axs_legacy::Event> for EventDef {
    fn from(e: &axs_legacy::Event) -> Self {
        let items = |v: &[(String, bool)]| {
            let mut out: Vec<EventItem> = v
                .iter()
                .map(|(n, o)| EventItem {
                    name: n.clone(),
                    optional: *o,
                })
                .collect();
            out.sort_by_key(|a| a.name.to_lowercase());
            out
        };
        Self {
            name: e.name.clone().unwrap_or_default(),
            description: e.description.clone(),
            addons: items(&e.addon_names),
            userconfig: items(e.userconfig_folder_names.as_deref().unwrap_or(&[])),
        }
    }
}

impl From<&EventDef> for axs_legacy::Event {
    fn from(e: &EventDef) -> Self {
        Self {
            name: Some(e.name.clone()),
            description: e.description.clone(),
            addon_names: e
                .addons
                .iter()
                .map(|i| (i.name.clone(), i.optional))
                .collect(),
            userconfig_folder_names: Some(
                e.userconfig
                    .iter()
                    .map(|i| (i.name.clone(), i.optional))
                    .collect(),
            ),
        }
    }
}

impl From<&axs_legacy::FavoriteServer> for ServerDef {
    fn from(s: &axs_legacy::FavoriteServer) -> Self {
        Self {
            name: s.name.clone().unwrap_or_default(),
            host: s.ip_address.clone().unwrap_or_default(),
            port: s.port.clamp(0, 65535) as u16,
            password: s.password.clone().filter(|p| !p.is_empty()),
            modset: s.modset_name.clone().filter(|p| !p.is_empty()),
        }
    }
}

impl ServerDef {
    pub fn to_legacy(&self, repository_name: &str) -> axs_legacy::FavoriteServer {
        axs_legacy::FavoriteServer {
            name: Some(self.name.clone()),
            ip_address: Some(self.host.clone()),
            port: self.port as i32,
            password: Some(self.password.clone().unwrap_or_default()),
            selected: false,
            modset_name: self.modset.clone(),
            repository_name: Some(repository_name.to_owned()),
        }
    }
}
