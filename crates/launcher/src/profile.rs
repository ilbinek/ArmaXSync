//! Launcher profiles, addon groups and launcher options.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How the game is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    /// Run the executable directly (legacy behaviour).
    #[default]
    Direct,
    /// `steam -applaunch 107410 …` (required for Proton on Linux).
    Steam,
}

/// All legacy launcher options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LauncherOptions {
    /// Game profile name (`-name=`), `None` = default profile.
    pub game_profile: Option<String>,
    pub show_script_errors: bool,
    pub no_pause: bool,
    pub window_mode: bool,
    pub file_patching: bool,
    pub check_signatures: bool,
    pub max_mem: Option<u32>,
    pub cpu_count: Option<u32>,
    pub ex_threads: Option<u32>,
    /// Allocator dll name without extension (`-malloc=`).
    pub malloc: Option<String>,
    pub enable_ht: bool,
    pub huge_pages: bool,
    pub no_splash: bool,
    pub default_world_empty: bool,
    pub no_logs: bool,
    pub auto_restart: bool,
}

/// A node of the addon groups tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GroupNode {
    Group {
        name: String,
        #[serde(default)]
        children: Vec<GroupNode>,
        /// Generated from a repository or event (read-only, refreshed on sync).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modset: Option<ModsetRef>,
    },
    Addon {
        /// Addon folder name (matched case-insensitively).
        name: String,
        #[serde(default)]
        selected: bool,
        #[serde(default)]
        optional: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ModsetRef {
    Repository { repository: String },
    Event { repository: String, event: String },
}

impl GroupNode {
    pub fn name(&self) -> &str {
        match self {
            GroupNode::Group { name, .. } | GroupNode::Addon { name, .. } => name,
        }
    }

    /// Selected addon names in tree order.
    pub fn selected(&self, out: &mut Vec<String>) {
        match self {
            GroupNode::Group { children, .. } => children.iter().for_each(|c| c.selected(out)),
            GroupNode::Addon {
                name,
                selected: true,
                ..
            } => out.push(name.clone()),
            _ => {}
        }
    }

    /// Select/deselect every addon below this node.
    pub fn set_selected(&mut self, v: bool) {
        match self {
            GroupNode::Group { children, .. } => {
                children.iter_mut().for_each(|c| c.set_selected(v))
            }
            GroupNode::Addon { selected, .. } => *selected = v,
        }
    }
}

/// A launcher profile (`profiles/<name>.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Game executable (arma3_x64.exe, arma3battleye.exe, arma3server_x64.exe…).
    pub executable: Option<PathBuf>,
    pub launch_mode: LaunchMode,
    /// Linux: launch through Steam/Proton and translate paths to `Z:\…`.
    pub proton: bool,
    pub options: LauncherOptions,
    /// Extra parameters (split on whitespace).
    pub additional_parameters: String,
    /// Folders searched for addons, in priority order.
    pub addon_search_dirs: Vec<PathBuf>,
    /// Detect Steam Workshop subscriptions as addons.
    pub use_workshop: bool,
    /// Addon load order (names); others follow alphabetically.
    pub addon_priority: Vec<String>,
    /// Addon groups tree.
    pub groups: Vec<GroupNode>,
    /// Favorite server to join (name), and modset to use with it.
    pub server: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            executable: None,
            launch_mode: LaunchMode::Direct,
            proton: false,
            options: LauncherOptions::default(),
            additional_parameters: String::new(),
            addon_search_dirs: vec![],
            use_workshop: true,
            addon_priority: vec![],
            groups: vec![],
            server: None,
        }
    }
}

impl Profile {
    pub fn selected_addons(&self) -> Vec<String> {
        let mut v = vec![];
        for g in &self.groups {
            g.selected(&mut v);
        }
        v
    }

    pub fn game_dir(&self) -> Option<&Path> {
        self.executable.as_deref().and_then(Path::parent)
    }
}

/// Favorite server (join target).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FavoriteServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    /// Group/repository/event to select when joining.
    pub modset: Option<String>,
    /// Repository whose auto-config provided this server.
    pub repository: Option<String>,
}

/// Program started before the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExternalApp {
    pub name: String,
    pub path: PathBuf,
    pub parameters: String,
    pub enabled: bool,
}

/// Profile files on disk.
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, name: &str) -> PathBuf {
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{}.toml", safe.trim()))
    }

    pub fn list(&self) -> Vec<Profile> {
        let mut v: Vec<Profile> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("toml"))
            .filter_map(|e| {
                std::fs::read_to_string(e.path())
                    .ok()
                    .and_then(|s| toml::from_str::<Profile>(&s).ok())
            })
            .collect();
        if !v.iter().any(|p| p.name == "Default") {
            v.push(Profile::default());
        }
        v.sort_by_key(|p| (p.name != "Default", p.name.to_lowercase()));
        v
    }

    pub fn get(&self, name: &str) -> Option<Profile> {
        self.list()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn save(&self, p: &Profile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let s = toml::to_string_pretty(p).map_err(std::io::Error::other)?;
        std::fs::write(self.file(&p.name), s)
    }

    pub fn remove(&self, name: &str) -> std::io::Result<()> {
        if name == "Default" {
            return Err(std::io::Error::other(
                "the Default profile cannot be removed",
            ));
        }
        std::fs::remove_file(self.file(name))
    }
}
