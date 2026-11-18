//! Addon discovery (legacy ArmA3Sync rules + Steam Workshop).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Folders of the game installation that are never addons.
pub const GAME_SYSTEM_FOLDERS: [&str; 7] = [
    "Addons",
    "Curator",
    "Kart",
    "Heli",
    "Mark",
    "Expansion",
    "Argo",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AddonSource {
    Local,
    Workshop { id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Addon {
    /// Unique key: lowercase folder name, `*`-suffixed on duplicates.
    pub key: String,
    /// Folder name (e.g. `@ace`).
    pub name: String,
    /// Name from mod.cpp/meta.cpp (Workshop items), else the folder name.
    pub display_name: String,
    /// Full path of the addon folder.
    pub path: PathBuf,
    pub source: AddonSource,
}

impl Addon {
    pub fn parent(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new(""))
    }
}

/// A folder is an addon when it has an `addons` child folder containing a
/// `.pbo` file (case-insensitive).
pub fn is_addon_dir(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        if e.file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("addons")
            && e.path().is_dir()
        {
            if let Ok(inner) = std::fs::read_dir(e.path()) {
                if inner.flatten().any(|f| {
                    f.file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(".pbo")
                }) {
                    return true;
                }
            }
        }
    }
    false
}

/// Read `name = "…";` from a mod.cpp / meta.cpp.
pub fn cpp_name(dir: &Path) -> Option<String> {
    for f in ["mod.cpp", "meta.cpp"] {
        let Ok(s) = std::fs::read_to_string(dir.join(f)) else {
            continue;
        };
        for line in s.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("name") {
                let rest = rest.trim_start();
                if let Some(v) = rest.strip_prefix('=') {
                    let v = v
                        .trim()
                        .trim_end_matches(';')
                        .trim()
                        .trim_matches('"')
                        .trim();
                    if !v.is_empty() {
                        return Some(v.to_owned());
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Default, Clone)]
pub struct DiscoverOptions {
    pub search_dirs: Vec<PathBuf>,
    /// Game executable folder (its system folders are skipped).
    pub game_dir: Option<PathBuf>,
    /// Steam Workshop content folders (`…/workshop/content/107410`).
    pub workshop_dirs: Vec<PathBuf>,
    /// Max folder depth to search below a search directory.
    pub max_depth: usize,
}

/// Discover addons; result is keyed by [`Addon::key`] (lowercase).
pub fn discover(opts: &DiscoverOptions) -> BTreeMap<String, Addon> {
    let mut out: BTreeMap<String, Addon> = BTreeMap::new();
    let ci = cfg!(windows);
    let norm = |p: &Path| -> String {
        let s = p
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_owned();
        if ci {
            s.to_lowercase()
        } else {
            s
        }
    };
    // Drop missing folders and folders nested inside another search folder.
    let mut dirs: Vec<PathBuf> = opts
        .search_dirs
        .iter()
        .filter(|d| d.is_dir())
        .cloned()
        .collect();
    dirs.sort_by_key(|d| norm(d).len());
    let mut kept: Vec<PathBuf> = vec![];
    for d in dirs {
        let nd = norm(&d);
        if !kept
            .iter()
            .any(|k| nd.starts_with(&format!("{}/", norm(k))) || nd == norm(k))
        {
            kept.push(d);
        }
    }
    let system: Vec<String> = match &opts.game_dir {
        Some(g) => GAME_SYSTEM_FOLDERS
            .iter()
            .map(|f| norm(&g.join(f)))
            .collect(),
        None => vec![],
    };
    let max_depth = if opts.max_depth == 0 {
        6
    } else {
        opts.max_depth
    };
    fn insert(out: &mut BTreeMap<String, Addon>, mut a: Addon) {
        let mut key = a.name.to_lowercase();
        while out.contains_key(&key) {
            key.push('*');
        }
        a.key = key.clone();
        out.insert(key, a);
    }
    fn walk(
        dir: &Path,
        depth: usize,
        max: usize,
        system: &[String],
        norm: &dyn Fn(&Path) -> String,
        out: &mut BTreeMap<String, Addon>,
    ) {
        if depth > max || system.contains(&norm(dir)) {
            return;
        }
        if depth > 0 && is_addon_dir(dir) {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let display = cpp_name(dir).unwrap_or_else(|| name.clone());
            insert(
                out,
                Addon {
                    key: String::new(),
                    name,
                    display_name: display,
                    path: dir.to_path_buf(),
                    source: AddonSource::Local,
                },
            );
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        subs.sort();
        for s in subs {
            walk(&s, depth + 1, max, system, norm, out);
        }
    }
    for d in &kept {
        walk(d, 0, max_depth, &system, &norm, &mut out);
    }
    for w in &opts.workshop_dirs {
        let Ok(rd) = std::fs::read_dir(w) else {
            continue;
        };
        let mut items: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        items.sort();
        for item in items {
            let Some(id) = item
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            if !is_addon_dir(&item) {
                continue;
            }
            let display = cpp_name(&item).unwrap_or_else(|| id.to_string());
            // Skip Workshop items already present through a search folder.
            if out.values().any(|a| norm(&a.path) == norm(&item)) {
                continue;
            }
            let name = format!("@{}", sanitize_name(&display));
            insert(
                &mut out,
                Addon {
                    key: String::new(),
                    name,
                    display_name: display,
                    path: item,
                    source: AddonSource::Workshop { id },
                },
            );
        }
    }
    out
}

/// Folder-safe version of a mod name.
pub fn sanitize_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    t.trim_matches('_').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn discovers_and_dedupes() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("A");
        let b = d.path().join("B");
        mk(&a, "@ace/addons/ace.pbo");
        mk(&a, "group/@cba/Addons/cba.PBO");
        mk(&a, "@notamod/readme.txt");
        mk(&b, "@ace/addons/ace2.pbo");
        mk(&b, "@ace/addons/inner/@nested/addons/x.pbo"); // not descended
        let ws = d.path().join("ws");
        mk(&ws, "463939057/addons/x.pbo");
        std::fs::write(
            ws.join("463939057/meta.cpp"),
            "protocol = 1;\npublishedid = 463939057;\nname = \"ACE3\";\n",
        )
        .unwrap();
        let opts = DiscoverOptions {
            search_dirs: vec![a.clone(), b, a.join("group")],
            workshop_dirs: vec![ws],
            ..Default::default()
        };
        let m = discover(&opts);
        let keys: Vec<&str> = m.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, ["@ace", "@ace*", "@ace3", "@cba"]);
        assert_eq!(m["@ace3"].display_name, "ACE3");
        assert!(matches!(
            m["@ace3"].source,
            AddonSource::Workshop { id: 463939057 }
        ));
    }
}
