//! Metadata rendering (legacy + modern) and publication.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;

use axs_legacy::{
    files, java_compare_ignore_case, AutoConfig, Changelog, Changelogs, Events, ServerInfo,
    SyncDir, SyncNode,
};
use axs_model::{
    ChangelogEntry, EventDef, Manifest, RepoIndex, AXS_FOLDER, INDEX_FILE, INDEX_FORMAT,
    MANIFEST_FORMAT,
};

use crate::scan::Scan;
use crate::{write_atomic, BuildOptions};

/// Metadata of the build being replaced.
#[derive(Debug, Default, Clone)]
pub struct PreviousBuild {
    pub sync: Option<SyncDir>,
    pub server_info: Option<ServerInfo>,
    pub changelogs: Option<Changelogs>,
    pub events: Option<Events>,
    pub index: Option<RepoIndex>,
    pub manifest: Option<Manifest>,
}

impl PreviousBuild {
    pub fn revision(&self) -> u32 {
        let legacy = self
            .server_info
            .as_ref()
            .map(|s| s.revision.max(0) as u32)
            .unwrap_or(0);
        let modern = self.index.as_ref().map(|i| i.revision).unwrap_or(0);
        legacy.max(modern)
    }

    /// Events of the previous build in the modern representation.
    pub fn events(&self) -> Option<Vec<EventDef>> {
        if let Some(m) = &self.manifest {
            return Some(m.events.clone());
        }
        self.events
            .as_ref()
            .map(|e| e.list.iter().map(EventDef::from).collect())
    }

    /// `path -> sha1` of the previous content.
    fn content(&self) -> Option<HashMap<String, String>> {
        if let Some(s) = &self.sync {
            let mut m = HashMap::new();
            s.walk_leaves(&mut |p, l| {
                m.insert(p.to_owned(), l.sha1.clone().unwrap_or_default());
            });
            return Some(m);
        }
        self.manifest.as_ref().map(|m| {
            m.files
                .iter()
                .map(|f| (f.path.clone(), f.sha1.clone()))
                .collect()
        })
    }
}

fn read_legacy<T>(root: &Path, name: &str, f: fn(&[u8]) -> axs_legacy::Result<T>) -> Option<T> {
    let p = root.join(files::A3S_FOLDER).join(name);
    let data = std::fs::read(&p).ok()?;
    match f(&data) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("ignoring unreadable {}: {e}", p.display());
            None
        }
    }
}

/// Read whatever metadata exists from a previous build (errors are ignored).
pub fn read_previous(root: &Path) -> PreviousBuild {
    let index = std::fs::read(root.join(AXS_FOLDER).join(INDEX_FILE))
        .ok()
        .and_then(|d| RepoIndex::parse(&d).ok());
    let manifest = index.as_ref().and_then(|i| {
        let data = std::fs::read(root.join(AXS_FOLDER).join(&i.manifest)).ok()?;
        Manifest::from_zst(&data).ok()
    });
    PreviousBuild {
        sync: read_legacy(root, files::SYNC, axs_legacy::read_sync),
        server_info: read_legacy(root, files::SERVERINFO, axs_legacy::read_server_info),
        changelogs: read_legacy(root, files::CHANGELOGS, axs_legacy::read_changelogs),
        events: read_legacy(root, files::EVENTS, axs_legacy::read_events),
        index,
        manifest,
    }
}

/// Everything to publish for one build.
#[derive(Debug, Clone)]
pub struct RenderedMetadata {
    pub revision: u32,
    pub build_date: i64,
    pub content_updated: bool,
    pub changelog: Changelog,
    pub sync: SyncDir,
    pub server_info: ServerInfo,
    pub changelogs: Changelogs,
    pub events: Events,
    pub autoconfig: AutoConfig,
    pub manifest: Manifest,
    pub emit_legacy: bool,
    pub emit_modern: bool,
}

impl RenderedMetadata {
    /// Encoded legacy files in publication order (serverinfo last).
    pub fn legacy_files(&self) -> Vec<(&'static str, Vec<u8>)> {
        vec![
            (files::SYNC, axs_legacy::write_sync(&self.sync)),
            (
                files::CHANGELOGS,
                axs_legacy::write_changelogs(&self.changelogs),
            ),
            (files::EVENTS, axs_legacy::write_events(&self.events)),
            (
                files::AUTOCONFIG,
                axs_legacy::write_autoconfig(&self.autoconfig),
            ),
            (
                files::SERVERINFO,
                axs_legacy::write_server_info(&self.server_info),
            ),
        ]
    }

    /// Encoded manifest (immutable) and index (mutable pointer).
    pub fn modern_files(&self) -> (String, Vec<u8>, Vec<u8>) {
        let zst = self.manifest.to_zst();
        let name = Manifest::file_name(self.revision);
        let index = RepoIndex {
            format: INDEX_FORMAT.into(),
            game: self.manifest.game,
            revision: self.revision,
            build_date: self.build_date,
            manifest: name.clone(),
            manifest_size: zst.len() as u64,
            manifest_blake3: blake3::hash(&zst).to_hex().to_string(),
        };
        let mut idx = serde_json::to_vec_pretty(&index).expect("index serializes");
        idx.push(b'\n');
        (name, zst, idx)
    }
}

/// Names of addon folders (legacy `getAddonsByName`: not descending into addons).
pub fn addon_names(tree: &SyncDir) -> Vec<String> {
    let mut out = vec![];
    fn rec(d: &SyncDir, out: &mut Vec<String>) {
        for c in &d.children {
            if let SyncNode::Dir(sub) = c {
                if sub.mark_as_addon {
                    out.push(sub.name.clone());
                } else {
                    rec(sub, out);
                }
            }
        }
    }
    rec(tree, &mut out);
    out
}

/// Addon name → `(leaf name, sha1)` sorted like Java's `Collections.sort`.
fn addon_leaves(tree: &SyncDir) -> HashMap<String, Vec<(String, String)>> {
    let mut out = HashMap::new();
    fn leaves(d: &SyncDir, v: &mut Vec<(String, String)>) {
        for c in &d.children {
            match c {
                SyncNode::Dir(s) => leaves(s, v),
                SyncNode::Leaf(l) => v.push((l.name.clone(), l.sha1.clone().unwrap_or_default())),
            }
        }
    }
    fn rec(d: &SyncDir, out: &mut HashMap<String, Vec<(String, String)>>) {
        for c in &d.children {
            if let SyncNode::Dir(sub) = c {
                if sub.mark_as_addon {
                    let mut v = vec![];
                    leaves(sub, &mut v);
                    v.sort_by(|a, b| java_compare_ignore_case(&a.0, &b.0));
                    out.insert(sub.name.clone(), v);
                } else {
                    rec(sub, out);
                }
            }
        }
    }
    rec(tree, &mut out);
    out
}

/// Compute the changelog entry for a new build (legacy rules).
pub fn diff_changelog(
    previous: Option<&Changelogs>,
    previous_sync: Option<&SyncDir>,
    tree: &SyncDir,
    revision: u32,
    build_date: i64,
    content_updated: bool,
) -> (Changelogs, Changelog) {
    let mut cl = Changelog {
        revision: revision as i32,
        build_date: Some(build_date),
        content_updated,
        addons: addon_names(tree),
        ..Default::default()
    };
    let mut list = previous.map(|c| c.list.clone()).unwrap_or_default();
    match list.last() {
        None => {
            if revision == 1 || previous_sync.is_none() {
                cl.new_addons = cl.addons.clone();
            }
        }
        Some(prev) => {
            cl.new_addons = cl
                .addons
                .iter()
                .filter(|a| !prev.addons.contains(a))
                .cloned()
                .collect();
            cl.deleted_addons = prev
                .addons
                .iter()
                .filter(|a| !cl.addons.contains(a))
                .cloned()
                .collect();
            if let Some(old) = previous_sync {
                let old_map = addon_leaves(old);
                let new_map = addon_leaves(tree);
                let mut updated: Vec<String> = new_map
                    .iter()
                    .filter(|(name, leaves)| match old_map.get(*name) {
                        Some(o) => {
                            o.len() != leaves.len()
                                || o.iter().zip(leaves.iter()).any(|(a, b)| a.1 != b.1)
                        }
                        None => false,
                    })
                    .map(|(n, _)| n.clone())
                    .collect();
                updated.sort_by(|a, b| java_compare_ignore_case(a, b));
                cl.updated_addons = updated;
            }
        }
    }
    list.push(cl.clone());
    while list.len() > 10 {
        list.remove(0);
    }
    (Changelogs { list }, cl)
}

pub(crate) fn render(
    opts: &BuildOptions,
    prev: &PreviousBuild,
    tree: SyncDir,
    files: Vec<axs_model::FileEntry>,
    scan: &Scan,
    total_size: u64,
    now: i64,
) -> RenderedMetadata {
    let revision = prev.revision() + 1;
    let content_updated = match prev.content() {
        Some(old) => {
            old.len() != files.len() || files.iter().any(|f| old.get(&f.path) != Some(&f.sha1))
        }
        None => true,
    };
    let (changelogs, changelog) = diff_changelog(
        prev.changelogs.as_ref(),
        prev.sync.as_ref(),
        &tree,
        revision,
        now,
        content_updated,
    );

    // Events: explicit, else carried forward; prune vanished addons (legacy).
    let names: HashSet<String> = addon_names(&tree).into_iter().collect();
    let mut events: Vec<EventDef> = opts
        .events
        .clone()
        .or_else(|| prev.events())
        .unwrap_or_default();
    for e in &mut events {
        e.addons.retain(|a| names.contains(&a.name));
    }

    let mut hidden: Vec<String> = opts
        .hidden_paths
        .iter()
        .map(|p| p.replace('\\', "/").trim_matches('/').to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    hidden.sort();
    hidden.dedup();

    let server_info = ServerInfo {
        revision: revision as i32,
        build_date: Some(now),
        number_of_files: files.len() as i64,
        total_files_size: total_size as i64,
        hidden_folder_paths: hidden.clone(),
        number_of_connections: opts.connections.max(1) as i32,
        no_partial_file_transfer: !opts.partial_transfer,
        repository_content_updated: content_updated,
        compressed_pbo_files_only: opts.compress_pbo && opts.compressed_pbo_only,
    };

    let protocole = opts.protocol.clone().map(|mut p| {
        if !opts.embed_credentials {
            p.login = Some("anonymous".into());
            p.password = Some(String::new());
        }
        p
    });
    let autoconfig = AutoConfig {
        repository_name: Some(opts.repo_name.clone()),
        protocole,
        favorite_servers: opts
            .favorite_servers
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s.repository_name = Some(opts.repo_name.clone());
                s
            })
            .collect(),
    };

    // Modern manifest: addon folder paths (top-most, like the legacy walk).
    let mut addon_paths = vec![];
    for a in &scan.addon_dirs {
        if !addon_paths
            .iter()
            .any(|p: &String| axs_model::paths::is_under(a, p, false))
        {
            addon_paths.push(a.clone());
        }
    }
    let manifest = Manifest {
        format: MANIFEST_FORMAT.into(),
        game: opts.game,
        name: opts.repo_name.clone(),
        revision,
        build_date: now,
        content_updated,
        connections: opts.connections.max(1),
        partial_transfer: opts.partial_transfer,
        compressed_only: server_info.compressed_pbo_files_only,
        block_size: axs_zsync::LEGACY_BLOCK_SIZE as u32,
        hidden_paths: hidden,
        dirs: scan.dirs.clone(),
        files,
        addons: addon_paths,
        events: events.clone(),
        changelogs: changelogs.list.iter().map(ChangelogEntry::from).collect(),
        servers: opts
            .favorite_servers
            .iter()
            .map(axs_model::ServerDef::from)
            .collect(),
        mods: vec![],
        signature: None,
    };

    RenderedMetadata {
        revision,
        build_date: now,
        content_updated,
        changelog,
        sync: tree,
        server_info,
        changelogs,
        events: Events {
            list: events.iter().map(axs_legacy::Event::from).collect(),
        },
        autoconfig,
        manifest,
        emit_legacy: opts.emit_legacy,
        emit_modern: opts.emit_modern,
    }
}

/// Publish rendered metadata into `root` atomically per file: the immutable
/// manifest first, then the legacy files (serverinfo last), then the index.
pub fn write_metadata(root: &Path, m: &RenderedMetadata) -> std::io::Result<()> {
    let axs = root.join(AXS_FOLDER);
    let modern = m.emit_modern.then(|| m.modern_files());
    if let Some((name, zst, _)) = &modern {
        std::fs::create_dir_all(&axs)?;
        write_atomic(&axs.join(name), zst)?;
    }
    if m.emit_legacy {
        let a3s = root.join(files::A3S_FOLDER);
        std::fs::create_dir_all(&a3s)?;
        for (name, bytes) in m.legacy_files() {
            write_atomic(&a3s.join(name), &bytes)?;
        }
    }
    if let Some((name, _, index)) = &modern {
        write_atomic(&axs.join(INDEX_FILE), index)?;
        prune_manifests(&axs, name, 3);
    }
    Ok(())
}

/// Keep the newest `keep` manifests (clients mid-sync may still read them).
fn prune_manifests(axs: &Path, current: &str, keep: usize) {
    let Ok(rd) = std::fs::read_dir(axs) else {
        return;
    };
    let mut revs: Vec<(u32, std::path::PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let rev = n
                .strip_prefix("manifest.")?
                .strip_suffix(".json.zst")?
                .parse()
                .ok()?;
            Some((rev, e.path()))
        })
        .collect();
    revs.sort_by_key(|r| std::cmp::Reverse(r.0));
    for (_, p) in revs.into_iter().skip(keep) {
        if p.file_name().map(|n| n != current).unwrap_or(true) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Create a single-entry deflate zip (`<file>.pbo.zip`); returns its size.
pub(crate) fn zip_single(src: &Path, dst: &Path) -> std::io::Result<u64> {
    let tmp = crate::tmp_path(dst);
    {
        let out = std::fs::File::create(&tmp)?;
        let mut zw = zip::ZipWriter::new(std::io::BufWriter::new(out));
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(9))
            .large_file(std::fs::metadata(src)?.len() >= 0xFFFF_FFFF);
        zw.start_file(name, opts).map_err(std::io::Error::other)?;
        let mut f = std::fs::File::open(src)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            zw.write_all(&buf[..n])?;
        }
        let mut w = zw.finish().map_err(std::io::Error::other)?;
        w.flush()?;
        w.get_ref().sync_all()?;
    }
    crate::rename_replace(&tmp, dst)?;
    Ok(std::fs::metadata(dst)?.len())
}

/// Delete `.zsync` / `.pbo.zip` sidecars whose content file is gone or whose
/// kind is disabled. Returns the number removed.
pub fn remove_stale_sidecars(
    root: &Path,
    live: &HashSet<&str>,
    zsync_on: bool,
    zip_on: bool,
    dirs: &[String],
) -> usize {
    let mut removed = 0;
    let mut check_dir = |dir: &Path, rel_dir: &str| {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let (base, enabled) = if let Some(b) = name.strip_suffix(".zsync") {
                (b.to_owned(), zsync_on)
            } else if let Some(b) = name.strip_suffix(".pbo.zip") {
                (format!("{b}.pbo"), zip_on)
            } else if name.ends_with(".axs-tmp") {
                // Leftover from an interrupted build.
                let _ = std::fs::remove_file(e.path());
                continue;
            } else {
                continue;
            };
            let base_rel = if rel_dir.is_empty() {
                base
            } else {
                format!("{rel_dir}/{base}")
            };
            if (!enabled || !live.contains(base_rel.as_str()))
                && std::fs::remove_file(e.path()).is_ok()
            {
                removed += 1;
            }
        }
    };
    check_dir(root, "");
    for d in dirs {
        check_dir(&axs_model::paths::join_rel(root, d), d);
    }
    removed
}
