//! Data directory resolution (OS standard locations or portable mode).

use std::path::PathBuf;

/// Where the client keeps its files.
#[derive(Debug, Clone)]
pub struct DataDirs {
    /// Configuration (config.toml, repositories/, profiles/).
    pub config: PathBuf,
    /// Caches and state (hash cache, logs).
    pub data: PathBuf,
    pub portable: bool,
}

impl DataDirs {
    /// Resolution order:
    /// 1. `ARMAXSYNC_HOME` environment variable;
    /// 2. portable mode: a `portable` file next to the executable → `<exe dir>/data`;
    /// 3. OS standard directories.
    pub fn resolve() -> Self {
        if let Some(home) = std::env::var_os("ARMAXSYNC_HOME").filter(|v| !v.is_empty()) {
            let p = PathBuf::from(home);
            return Self {
                config: p.clone(),
                data: p,
                portable: true,
            };
        }
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        {
            if dir.join("portable").exists() {
                let p = dir.join("data");
                return Self {
                    config: p.clone(),
                    data: p,
                    portable: true,
                };
            }
        }
        match directories::ProjectDirs::from("eu", "ArmaXSync", "ArmaXSync") {
            Some(pd) => Self {
                config: pd.config_dir().to_path_buf(),
                data: pd.data_local_dir().to_path_buf(),
                portable: false,
            },
            None => {
                let p = PathBuf::from(".armaxsync");
                Self {
                    config: p.clone(),
                    data: p,
                    portable: true,
                }
            }
        }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        let p = root.into();
        Self {
            config: p.clone(),
            data: p,
            portable: true,
        }
    }

    pub fn repositories(&self) -> PathBuf {
        self.config.join("repositories")
    }
    pub fn profiles(&self) -> PathBuf {
        self.config.join("profiles")
    }
    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }
    pub fn hash_cache(&self) -> PathBuf {
        self.data.join("hashcache.zst")
    }
}
