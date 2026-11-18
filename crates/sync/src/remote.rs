//! Loading a remote repository description (modern manifest or legacy .a3s).

use axs_legacy::{files, AutoConfig, Changelogs, Events, ServerInfo, SyncDir, SyncNode};
use axs_model::{
    ChangelogEntry, EventDef, FileEntry, GameId, Manifest, RepoIndex, ServerDef, AXS_FOLDER,
    INDEX_FILE, MANIFEST_FORMAT,
};
use axs_transport::{Transport, TransportError};

use crate::{Result, SyncError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// `.axs/index.json` + manifest (ArmaXSync).
    Modern,
    /// `.a3s/*` (Arma3Sync).
    Legacy,
}

/// Everything known about a remote repository at one point in time.
#[derive(Debug, Clone)]
pub struct RemoteSnapshot {
    pub source: SourceKind,
    /// Normalized view used by the planner (built from legacy files if needed).
    pub manifest: Manifest,
    pub index: Option<RepoIndex>,
    pub server_info: Option<ServerInfo>,
    pub autoconfig: Option<AutoConfig>,
    /// Range requests work (probed for legacy servers).
    pub ranges_supported: bool,
}

impl RemoteSnapshot {
    pub fn revision(&self) -> u32 {
        self.manifest.revision
    }
}

async fn get_optional(t: &Transport, rel: &str) -> Result<Option<Vec<u8>>> {
    match t.get_bytes(rel).await {
        Ok(b) => Ok(Some(b)),
        Err(TransportError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Fetch only the cheap revision information (for status checks).
pub async fn fetch_snapshot(t: &Transport, prefer_modern: bool) -> Result<RemoteSnapshot> {
    if prefer_modern {
        if let Some(idx_bytes) = get_optional(t, &format!("{AXS_FOLDER}/{INDEX_FILE}")).await? {
            let index = RepoIndex::parse(&idx_bytes)
                .map_err(|e| SyncError::Remote(format!("index.json: {e}")))?;
            let mbytes = t
                .get_bytes(&format!("{AXS_FOLDER}/{}", index.manifest))
                .await?;
            if blake3::hash(&mbytes).to_hex().as_str() != index.manifest_blake3 {
                return Err(SyncError::Remote(
                    "manifest checksum mismatch (publish in progress?)".into(),
                ));
            }
            let manifest = Manifest::from_zst(&mbytes)
                .map_err(|e| SyncError::Remote(format!("manifest: {e}")))?;
            let ranges = t.is_http() && manifest.partial_transfer;
            return Ok(RemoteSnapshot {
                source: SourceKind::Modern,
                manifest,
                index: Some(index),
                server_info: None,
                autoconfig: None,
                ranges_supported: ranges,
            });
        }
    }
    fetch_legacy(t).await
}

/// Load the legacy `.a3s` metadata (sync, serverinfo, changelogs, events, autoconfig).
pub async fn fetch_legacy(t: &Transport) -> Result<RemoteSnapshot> {
    let a3s = |n: &str| format!("{}/{n}", files::A3S_FOLDER);
    let need = |r: Option<Vec<u8>>, what: &str| {
        r.ok_or_else(|| SyncError::Remote(format!("remote file not found: /.a3s/{what}")))
    };
    let sync_b = need(get_optional(t, &a3s(files::SYNC)).await?, files::SYNC)?;
    let si_b = need(
        get_optional(t, &a3s(files::SERVERINFO)).await?,
        files::SERVERINFO,
    )?;
    let cl_b = get_optional(t, &a3s(files::CHANGELOGS)).await?;
    let ev_b = get_optional(t, &a3s(files::EVENTS)).await?;
    let ac_b = get_optional(t, &a3s(files::AUTOCONFIG)).await?;
    fn legacy<T>(r: axs_legacy::Result<T>, what: &str) -> Result<T> {
        r.map_err(|e| SyncError::Remote(format!("/.a3s/{what}: {e}")))
    }
    let sync = legacy(axs_legacy::read_sync(&sync_b), files::SYNC)?;
    let si = legacy(axs_legacy::read_server_info(&si_b), files::SERVERINFO)?;
    let cl = cl_b
        .map(|b| legacy(axs_legacy::read_changelogs(&b), files::CHANGELOGS))
        .transpose()?;
    let ev = ev_b
        .map(|b| legacy(axs_legacy::read_events(&b), files::EVENTS))
        .transpose()?;
    let ac = ac_b
        .map(|b| legacy(axs_legacy::read_autoconfig(&b), files::AUTOCONFIG))
        .transpose()?;
    let ranges =
        t.is_http() && !si.no_partial_file_transfer && t.supports_ranges(&a3s(files::SYNC)).await;
    let manifest = legacy_to_manifest(
        &sync,
        &si,
        cl.as_ref(),
        ev.as_ref(),
        ac.as_ref(),
        t.is_http(),
        ranges,
    );
    Ok(RemoteSnapshot {
        source: SourceKind::Legacy,
        manifest,
        index: None,
        server_info: Some(si),
        autoconfig: ac,
        ranges_supported: ranges,
    })
}

/// Normalize legacy metadata into a manifest.
pub fn legacy_to_manifest(
    sync: &SyncDir,
    si: &ServerInfo,
    changelogs: Option<&Changelogs>,
    events: Option<&Events>,
    autoconfig: Option<&AutoConfig>,
    zsync: bool,
    partial: bool,
) -> Manifest {
    let mut files = vec![];
    let mut dirs = vec![];
    let mut addons = vec![];
    fn rec(
        d: &SyncDir,
        prefix: &str,
        in_addon: bool,
        zsync: bool,
        files: &mut Vec<FileEntry>,
        dirs: &mut Vec<String>,
        addons: &mut Vec<String>,
    ) {
        for c in &d.children {
            let p = if prefix.is_empty() {
                c.name().to_owned()
            } else {
                format!("{prefix}/{}", c.name())
            };
            match c {
                SyncNode::Dir(sub) => {
                    dirs.push(p.clone());
                    let is_addon = sub.mark_as_addon && !in_addon;
                    if is_addon {
                        addons.push(p.clone());
                    }
                    rec(sub, &p, in_addon || is_addon, zsync, files, dirs, addons);
                }
                SyncNode::Leaf(l) => files.push(FileEntry {
                    path: p,
                    size: l.size.max(0) as u64,
                    sha1: l.sha1.clone().unwrap_or_default(),
                    blake3: String::new(),
                    zsync,
                    zip_size: l.compressed.then_some(l.compressed_size.max(0) as u64),
                }),
            }
        }
    }
    rec(sync, "", false, zsync, &mut files, &mut dirs, &mut addons);
    Manifest {
        format: MANIFEST_FORMAT.into(),
        game: GameId::Arma3,
        name: autoconfig
            .and_then(|a| a.repository_name.clone())
            .unwrap_or_default(),
        revision: si.revision.max(0) as u32,
        build_date: si.build_date.unwrap_or(0),
        content_updated: si.repository_content_updated,
        connections: si.number_of_connections.max(1) as u32,
        partial_transfer: partial,
        compressed_only: si.compressed_pbo_files_only,
        block_size: axs_zsync::LEGACY_BLOCK_SIZE as u32,
        hidden_paths: si.hidden_folder_paths.clone(),
        dirs,
        files,
        addons,
        events: events
            .map(|e| e.list.iter().map(EventDef::from).collect())
            .unwrap_or_default(),
        changelogs: changelogs
            .map(|c| c.list.iter().map(ChangelogEntry::from).collect())
            .unwrap_or_default(),
        servers: autoconfig
            .map(|a| a.favorite_servers.iter().map(ServerDef::from).collect())
            .unwrap_or_default(),
        mods: vec![],
        signature: None,
    }
}

/// Download and parse a legacy `.a3s/autoconfig` from a full autoconfig URL.
pub async fn fetch_autoconfig(url: &str) -> Result<(AutoConfig, axs_transport::Endpoint)> {
    let ep = axs_transport::Endpoint::parse(url, None, None)?;
    let t = Transport::new(ep.clone())?;
    let data = t
        .get_bytes(&format!("{}/{}", files::A3S_FOLDER, files::AUTOCONFIG))
        .await?;
    let ac = axs_legacy::read_autoconfig(&data)
        .map_err(|e| SyncError::Remote(format!("autoconfig: {e}")))?;
    Ok((ac, ep))
}
