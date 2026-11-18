//! Local snapshot + pure planner.

use std::collections::{BTreeMap, HashMap, HashSet};

use std::path::Path;

use serde::Serialize;

use axs_build::{CachedHash, HashCache};
use axs_model::paths::{is_under, join_rel, parent};
use axs_model::{FileEntry, Manifest};

/// What the user wants synced.
#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    /// Only these repository sub-paths (event / modset selection); `None` = all.
    pub selection: Option<Vec<String>>,
    /// Remove extra files at the destination root as well (legacy "exact match").
    pub exact_match: bool,
    /// Folders whose extra local content is kept (user "hide extra content").
    pub keep_extras: Vec<String>,
    /// Compare names case-insensitively (Windows).
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Mkdir {
        path: String,
    },
    Fetch {
        path: String,
        size: u64,
        sha1: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blake3: Option<String>,
        zsync: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        zip_size: Option<u64>,
        /// Size of the existing local file (delta seed), if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        local_size: Option<u64>,
    },
    Delete {
        path: String,
        dir: bool,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncPlan {
    pub revision: u32,
    pub ops: Vec<Op>,
    pub fetch_count: usize,
    pub fetch_bytes: u64,
    pub delete_count: usize,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.fetch_count == 0
            && self.delete_count == 0
            && !self.ops.iter().any(|o| matches!(o, Op::Mkdir { .. }))
    }
}

/// Local state of the entries relevant to a plan.
#[derive(Debug, Clone, Default)]
pub struct LocalSnapshot {
    /// Relative path (as on disk) → entry.
    pub entries: BTreeMap<String, LocalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub dir: bool,
    pub size: u64,
    /// Present for files whose size matches the remote file.
    pub sha1: Option<String>,
}

fn fold(s: &str, ci: bool) -> String {
    if ci {
        s.to_lowercase()
    } else {
        s.to_owned()
    }
}

/// Repository files and directories targeted by a selection.
fn targets<'a>(m: &'a Manifest, opts: &PlanOptions) -> (Vec<&'a FileEntry>, Vec<String>) {
    let sel = |p: &str| match &opts.selection {
        None => true,
        Some(s) => s.iter().any(|pre| is_under(p, pre, opts.case_insensitive)),
    };
    let files: Vec<&FileEntry> = m.files.iter().filter(|f| sel(&f.path)).collect();
    let mut dirs: HashSet<String> = HashSet::new();
    for d in &m.dirs {
        if sel(d) {
            dirs.insert(d.clone());
        }
    }
    for f in &files {
        let mut p = parent(&f.path);
        while !p.is_empty() {
            dirs.insert(p.to_owned());
            p = parent(p);
        }
    }
    let mut dirs: Vec<String> = dirs.into_iter().collect();
    dirs.sort();
    (files, dirs)
}

/// Gather the local state needed to plan (I/O). Files whose size equals the
/// remote size are hashed (with the cache keyed by absolute path).
pub fn snapshot_local(
    root: &Path,
    m: &Manifest,
    opts: &PlanOptions,
    cache: &mut dyn HashCache,
) -> std::io::Result<LocalSnapshot> {
    let (files, dirs) = targets(m, opts);
    let ci = opts.case_insensitive;
    let remote_sizes: HashMap<String, u64> =
        files.iter().map(|f| (fold(&f.path, ci), f.size)).collect();
    let mut snap = LocalSnapshot::default();
    let mut list_dirs: Vec<String> = vec![String::new()];
    list_dirs.extend(dirs.iter().cloned());
    for d in list_dirs {
        let abs = if d.is_empty() {
            root.to_path_buf()
        } else {
            join_rel(root, &d)
        };
        let Ok(rd) = std::fs::read_dir(&abs) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if d.is_empty() {
                name.clone()
            } else {
                format!("{d}/{name}")
            };
            let Ok(md) = std::fs::metadata(e.path()) else {
                continue;
            };
            if md.is_dir() {
                snap.entries.insert(
                    rel,
                    LocalEntry {
                        dir: true,
                        size: 0,
                        sha1: None,
                    },
                );
                continue;
            }
            let size = md.len();
            let sha1 = match remote_sizes.get(&fold(&rel, ci)) {
                Some(&rs) if rs == size => {
                    let key = e.path().to_string_lossy().into_owned();
                    let mtime = axs_build_mtime(&md);
                    match cache
                        .get(&key)
                        .filter(|c| c.size == size && c.mtime_ns == mtime)
                    {
                        Some(c) => Some(c.sha1),
                        None => {
                            let h = axs_build::hash_file(&e.path(), None)?;
                            cache.put(
                                &key,
                                CachedHash {
                                    size,
                                    mtime_ns: mtime,
                                    sha1: h.sha1.clone(),
                                    blake3: h.blake3,
                                    zsync_sha1: None,
                                    zip_sha1: None,
                                },
                            );
                            Some(h.sha1)
                        }
                    }
                }
                _ => None,
            };
            snap.entries.insert(
                rel,
                LocalEntry {
                    dir: false,
                    size,
                    sha1,
                },
            );
        }
    }
    Ok(snap)
}

fn axs_build_mtime(md: &std::fs::Metadata) -> i128 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128)
        .unwrap_or(0)
}

/// Names never treated as "extra" local content.
fn ignorable(name: &str) -> bool {
    name.ends_with(".axs-part")
        || name.contains(".axs-part.")
        || name.ends_with(".part")
        || name.contains(".pbo.zip")
        || name.ends_with(".axs-tmp")
}

/// Compute the operations turning `local` into the selected remote content.
pub fn plan(m: &Manifest, local: &LocalSnapshot, opts: &PlanOptions) -> SyncPlan {
    let ci = opts.case_insensitive;
    let (files, dirs) = targets(m, opts);
    let by_fold: HashMap<String, (&String, &LocalEntry)> = local
        .entries
        .iter()
        .map(|(k, v)| (fold(k, ci), (k, v)))
        .collect();
    let mut ops = vec![];
    let mut plan = SyncPlan {
        revision: m.revision,
        ..Default::default()
    };

    // Directories.
    for d in &dirs {
        match by_fold.get(&fold(d, ci)) {
            Some((_, e)) if e.dir => {}
            Some((k, _)) => {
                ops.push(Op::Delete {
                    path: (*k).clone(),
                    dir: false,
                });
                plan.delete_count += 1;
                ops.push(Op::Mkdir { path: d.clone() });
            }
            None => ops.push(Op::Mkdir { path: d.clone() }),
        }
    }

    // Files.
    for f in &files {
        let local_e = by_fold.get(&fold(&f.path, ci));
        let up_to_date = matches!(local_e, Some((_, e)) if !e.dir && e.size == f.size && e.sha1.as_deref() == Some(f.sha1.as_str()));
        if up_to_date {
            continue;
        }
        let mut local_size = None;
        if let Some((k, e)) = local_e {
            if e.dir {
                ops.push(Op::Delete {
                    path: (*k).clone(),
                    dir: true,
                });
                plan.delete_count += 1;
            } else {
                local_size = Some(e.size);
            }
        }
        plan.fetch_count += 1;
        plan.fetch_bytes += f.size;
        ops.push(Op::Fetch {
            path: f.path.clone(),
            size: f.size,
            sha1: f.sha1.clone(),
            blake3: Some(f.blake3.clone()).filter(|b| !b.is_empty()),
            zsync: f.zsync,
            zip_size: f.zip_size,
            local_size,
        });
    }

    // Extra local content.
    let remote_names: HashSet<String> = files
        .iter()
        .map(|f| fold(&f.path, ci))
        .chain(dirs.iter().map(|d| fold(d, ci)))
        .collect();
    let mut keep: Vec<&str> = m.hidden_paths.iter().map(String::as_str).collect();
    keep.extend(opts.keep_extras.iter().map(String::as_str));
    let mut scan_dirs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    if opts.exact_match && opts.selection.is_none() {
        scan_dirs.insert(0, "");
    }
    let deletable_dir = |d: &str| !keep.iter().any(|k| is_under(d, k, ci));
    let mut children: HashMap<&str, Vec<(&String, &LocalEntry)>> = HashMap::new();
    for (k, e) in &local.entries {
        children.entry(parent(k)).or_default().push((k, e));
    }
    let mut deleted_dirs: Vec<String> = vec![];
    for d in scan_dirs {
        if !d.is_empty() && !deletable_dir(d) {
            continue;
        }
        // Local folder names may differ in case from the remote ones.
        let local_dir = if ci {
            by_fold
                .get(&fold(d, ci))
                .map(|(k, _)| k.as_str())
                .unwrap_or(d)
        } else {
            d
        };
        for (k, e) in children.get(local_dir).cloned().unwrap_or_default() {
            let name = axs_model::paths::file_name(k);
            if ignorable(name) || remote_names.contains(&fold(k, ci)) {
                continue;
            }
            if !deletable_dir(k) {
                continue;
            }
            if deleted_dirs.iter().any(|dd| is_under(k, dd, ci)) {
                continue;
            }
            if e.dir {
                deleted_dirs.push(k.clone());
            }
            ops.push(Op::Delete {
                path: k.clone(),
                dir: e.dir,
            });
            plan.delete_count += 1;
        }
    }
    plan.ops = ops;
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(path: &str, size: u64, sha1: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            size,
            sha1: sha1.into(),
            blake3: String::new(),
            zsync: true,
            zip_size: None,
        }
    }

    fn manifest() -> Manifest {
        Manifest {
            format: axs_model::MANIFEST_FORMAT.into(),
            game: axs_model::GameId::Arma3,
            name: "t".into(),
            revision: 5,
            build_date: 0,
            content_updated: true,
            connections: 2,
            partial_transfer: true,
            compressed_only: false,
            block_size: 8192,
            hidden_paths: vec!["userconfig".into()],
            dirs: vec![
                "@a".into(),
                "@a/addons".into(),
                "@b".into(),
                "@b/addons".into(),
                "userconfig".into(),
            ],
            files: vec![
                fe("@a/addons/a.pbo", 10, "aa"),
                fe("@b/addons/b.pbo", 20, "bb"),
                fe("readme.txt", 1, "rr"),
            ],
            addons: vec!["@a".into(), "@b".into()],
            events: vec![],
            changelogs: vec![],
            servers: vec![],
            mods: vec![],
            signature: None,
        }
    }

    fn le(dir: bool, size: u64, sha1: Option<&str>) -> LocalEntry {
        LocalEntry {
            dir,
            size,
            sha1: sha1.map(str::to_owned),
        }
    }

    #[test]
    fn fresh_destination() {
        let p = plan(
            &manifest(),
            &LocalSnapshot::default(),
            &PlanOptions::default(),
        );
        assert_eq!(p.fetch_count, 3);
        assert_eq!(p.fetch_bytes, 31);
        assert_eq!(p.delete_count, 0);
        assert_eq!(
            p.ops
                .iter()
                .filter(|o| matches!(o, Op::Mkdir { .. }))
                .count(),
            5
        );
    }

    #[test]
    fn update_delete_and_hidden() {
        let mut l = LocalSnapshot::default();
        for d in [
            "@a",
            "@a/addons",
            "@b",
            "@b/addons",
            "userconfig",
            "@old",
            "@b/keys",
        ] {
            l.entries.insert(d.into(), le(true, 0, None));
        }
        l.entries
            .insert("@a/addons/a.pbo".into(), le(false, 10, Some("aa")));
        l.entries
            .insert("@b/addons/b.pbo".into(), le(false, 20, Some("XX")));
        l.entries
            .insert("@b/addons/extra.pbo".into(), le(false, 5, None));
        l.entries
            .insert("@b/addons/x.pbo.axs-part".into(), le(false, 5, None));
        l.entries
            .insert("userconfig/mine.hpp".into(), le(false, 5, None));
        l.entries.insert("rootextra.txt".into(), le(false, 5, None));
        let p = plan(&manifest(), &l, &PlanOptions::default());
        let fetched: Vec<_> = p
            .ops
            .iter()
            .filter_map(|o| {
                if let Op::Fetch {
                    path, local_size, ..
                } = o
                {
                    Some((path.as_str(), *local_size))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            fetched,
            [("@b/addons/b.pbo", Some(20)), ("readme.txt", None)]
        );
        let deleted: Vec<_> = p
            .ops
            .iter()
            .filter_map(|o| {
                if let Op::Delete { path, .. } = o {
                    Some(path.as_str())
                } else {
                    None
                }
            })
            .collect();
        // Extras inside synced folders are removed; root extras and hidden
        // folders are kept without exact match.
        assert_eq!(deleted, ["@b/keys", "@b/addons/extra.pbo"]);

        let p = plan(
            &manifest(),
            &l,
            &PlanOptions {
                exact_match: true,
                ..Default::default()
            },
        );
        let deleted: Vec<_> = p
            .ops
            .iter()
            .filter_map(|o| {
                if let Op::Delete { path, .. } = o {
                    Some(path.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            deleted,
            ["@old", "rootextra.txt", "@b/keys", "@b/addons/extra.pbo"]
        );
    }

    #[test]
    fn selection_and_case() {
        let mut l = LocalSnapshot::default();
        l.entries.insert("@A".into(), le(true, 0, None));
        l.entries.insert("@A/Addons".into(), le(true, 0, None));
        l.entries
            .insert("@A/Addons/A.PBO".into(), le(false, 10, Some("aa")));
        let opts = PlanOptions {
            selection: Some(vec!["@a".into()]),
            case_insensitive: true,
            ..Default::default()
        };
        let p = plan(&manifest(), &l, &opts);
        assert!(p.is_empty(), "{:?}", p.ops);
        let opts = PlanOptions {
            selection: Some(vec!["@a".into()]),
            case_insensitive: false,
            ..Default::default()
        };
        let p = plan(&manifest(), &l, &opts);
        assert_eq!(p.fetch_count, 1);
    }
}
