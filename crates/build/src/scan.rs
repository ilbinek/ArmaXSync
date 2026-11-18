//! Repository folder scanning (legacy inclusion rules).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use axs_legacy::{SyncDir, SyncLeaf, SyncNode};

use crate::{BuildError, Result};

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub rel: String,
    pub abs: PathBuf,
    pub size: u64,
    pub mtime_ns: i128,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub files: Vec<ScannedFile>,
    /// All directories, `/`-separated relative paths.
    pub dirs: Vec<String>,
    /// Directories with an `addons` child (legacy `markAsAddon`).
    pub addon_dirs: Vec<String>,
}

/// Legacy rule: names containing these substrings are never part of a repo.
pub fn is_excluded_name(name: &str, is_dir: bool) -> bool {
    if is_dir {
        name.contains(".a3s") || name == ".axs"
    } else {
        name.contains(".zsync")
            || name.contains(".pbo.zip")
            || name.contains(".a3s")
            || name.ends_with(".axs-tmp")
            || name.ends_with(".axs-part")
    }
}

pub fn mtime_ns(md: &std::fs::Metadata) -> i128 {
    md.modified()
        .ok()
        .map(|t| match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as i128,
            Err(e) => -(e.duration().as_nanos() as i128),
        })
        .unwrap_or(0)
}

impl Scan {
    /// Assemble a scan from explicit entries (virtual trees). Parent folders
    /// are added automatically and addon folders detected from the names.
    pub fn from_entries(
        mut files: Vec<ScannedFile>,
        dirs: impl IntoIterator<Item = String>,
    ) -> Scan {
        use std::collections::BTreeSet;
        let mut all: BTreeSet<String> = dirs.into_iter().collect();
        for f in &files {
            let mut p = axs_model::paths::parent(&f.rel);
            while !p.is_empty() {
                all.insert(p.to_owned());
                p = axs_model::paths::parent(p);
            }
        }
        let mut addon: BTreeSet<String> = BTreeSet::new();
        let child_names = files
            .iter()
            .map(|f| f.rel.as_str())
            .chain(all.iter().map(|d| d.as_str()));
        for c in child_names {
            if axs_model::paths::file_name(c).eq_ignore_ascii_case("addons") {
                let p = axs_model::paths::parent(c);
                if !p.is_empty() {
                    addon.insert(p.to_owned());
                }
            }
        }
        files.sort_by(|a, b| a.rel.cmp(&b.rel));
        Scan {
            files,
            dirs: all.into_iter().collect(),
            addon_dirs: addon.into_iter().collect(),
        }
    }
}

pub fn scan(root: &Path, excluded_files: &[String]) -> Result<Scan> {
    let mut scan = Scan::default();
    let excluded: std::collections::HashSet<&str> =
        excluded_files.iter().map(|s| s.trim_matches('/')).collect();
    fn rec(
        dir: &Path,
        rel: &str,
        excluded: &std::collections::HashSet<&str>,
        scan: &mut Scan,
    ) -> Result<()> {
        let rd = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
        let mut has_addons_child = false;
        let mut entries = vec![];
        for e in rd {
            let e = e.map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
            let name = e.file_name().to_string_lossy().into_owned();
            if name.to_lowercase() == "addons" {
                has_addons_child = true;
            }
            entries.push((name, e));
        }
        if has_addons_child && !rel.is_empty() {
            scan.addon_dirs.push(rel.to_owned());
        }
        for (name, e) in entries {
            let ft = e.file_type().map_err(|err| BuildError::Io(e.path(), err))?;
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            // Follow symlinks so admins can link mods into a repository.
            let (is_dir, md) = if ft.is_symlink() {
                match std::fs::metadata(e.path()) {
                    Ok(md) => (md.is_dir(), md),
                    Err(_) => continue, // dangling link
                }
            } else {
                let md = e.metadata().map_err(|err| BuildError::Io(e.path(), err))?;
                (ft.is_dir(), md)
            };
            if is_excluded_name(&name, is_dir) {
                continue;
            }
            // Excluded entries can be files or whole folders (e.g. `.sync`).
            if excluded.contains(child_rel.as_str()) {
                continue;
            }
            if is_dir {
                scan.dirs.push(child_rel.clone());
                rec(&e.path(), &child_rel, excluded, scan)?;
            } else {
                scan.files.push(ScannedFile {
                    rel: child_rel,
                    abs: e.path(),
                    size: md.len(),
                    mtime_ns: mtime_ns(&md),
                });
            }
        }
        Ok(())
    }
    if !root.is_dir() {
        return Err(BuildError::NotADirectory(root.to_path_buf()));
    }
    rec(root, "", &excluded, &mut scan)?;
    scan.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    scan.dirs.sort();
    scan.addon_dirs.sort();
    Ok(scan)
}

/// Build the legacy sync tree from scanned entries and per-file hashes.
pub fn build_tree(scan: &Scan, leaf: impl Fn(&ScannedFile) -> SyncLeaf) -> SyncDir {
    use std::collections::HashSet;
    let addon: HashSet<&str> = scan.addon_dirs.iter().map(|s| s.as_str()).collect();
    let mut root = SyncDir::root();
    fn get_dir<'a>(root: &'a mut SyncDir, rel: &str) -> &'a mut SyncDir {
        let mut cur = root;
        if rel.is_empty() {
            return cur;
        }
        for seg in rel.split('/') {
            let idx = cur
                .children
                .iter()
                .position(|c| matches!(c, SyncNode::Dir(d) if d.name == seg));
            let idx = match idx {
                Some(i) => i,
                None => {
                    cur.children.push(SyncNode::Dir(SyncDir::new(seg)));
                    cur.children.len() - 1
                }
            };
            cur = match &mut cur.children[idx] {
                SyncNode::Dir(d) => d,
                _ => unreachable!(),
            };
        }
        cur
    }
    for d in &scan.dirs {
        let node = get_dir(&mut root, d);
        node.mark_as_addon = addon.contains(d.as_str());
    }
    for f in &scan.files {
        let parent = axs_model::paths::parent(&f.rel);
        get_dir(&mut root, parent)
            .children
            .push(SyncNode::Leaf(leaf(f)));
    }
    root.sort_legacy();
    root
}
