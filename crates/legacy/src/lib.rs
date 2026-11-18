//! Legacy Arma3Sync file formats.
//!
//! * `.a3s/{sync,serverinfo,changelogs,events,autoconfig}` — repository metadata
//!   (GZIP + Java serialization), read and written byte-compatibly.
//! * Local client files of an old install (`a3s.cfg`, `a3s.prefs`,
//!   `*.a3s.profile`, encrypted `*.a3s.repository`) — read-only, for migration.

pub mod a3s;
pub mod canonical;
pub mod client;
pub mod model;
pub mod urls;

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

pub use model::*;

/// Maximum decompressed size accepted for a legacy metadata file.
pub const MAX_DECOMPRESSED: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("serialization stream: {0}")]
    Stream(#[from] axs_javaser::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("expected {expected}, found {found}")]
    UnexpectedClass { expected: String, found: String },
    #[error("stream root is null")]
    NullRoot,
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("decryption failed: {0}")]
    Crypto(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Names of the files inside a repository's `.a3s` folder.
pub mod files {
    pub const A3S_FOLDER: &str = ".a3s";
    pub const SYNC: &str = "sync";
    pub const SERVERINFO: &str = "serverinfo";
    pub const CHANGELOGS: &str = "changelogs";
    pub const EVENTS: &str = "events";
    pub const AUTOCONFIG: &str = "autoconfig";
    pub const ZSYNC_EXT: &str = ".zsync";
    pub const PBO_ZIP_EXT: &str = ".pbo.zip";
    pub const PART_EXT: &str = ".part";
}

/// Gunzip a legacy file (bounded).
pub fn gunzip(gz: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    GzDecoder::new(gz)
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > MAX_DECOMPRESSED {
        return Err(Error::Invalid("decompressed file too large".into()));
    }
    Ok(out)
}

/// Gzip bytes the way `GZIPOutputStream` would (default level).
pub fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(
        Vec::with_capacity(raw.len() / 3 + 64),
        Compression::default(),
    );
    enc.write_all(raw).expect("in-memory write");
    enc.finish().expect("in-memory write")
}

/// Parse a gzipped serialization stream into a graph.
pub fn parse_gz(gz: &[u8]) -> Result<axs_javaser::Graph> {
    Ok(axs_javaser::parse(&gunzip(gz)?)?)
}

fn encode(f: impl FnOnce(&mut axs_javaser::Writer)) -> Vec<u8> {
    let mut w = axs_javaser::Writer::new();
    f(&mut w);
    w.finish()
}

macro_rules! file_fns {
    ($ty:ty, $read:ident, $read_graph:path, $write:ident, $write_raw:ident, $write_fn:path) => {
        /// Read from gzipped bytes.
        pub fn $read(gz: &[u8]) -> Result<$ty> {
            $read_graph(&parse_gz(gz)?)
        }
        /// Encode as an uncompressed serialization stream.
        pub fn $write_raw(v: &$ty) -> Vec<u8> {
            encode(|w| $write_fn(w, v))
        }
        /// Encode as a gzipped file body.
        pub fn $write(v: &$ty) -> Vec<u8> {
            gzip(&$write_raw(v))
        }
    };
}

file_fns!(
    SyncDir,
    read_sync,
    a3s::read_sync_graph,
    write_sync,
    write_sync_raw,
    a3s::write_sync
);
file_fns!(
    ServerInfo,
    read_server_info,
    a3s::read_server_info_graph,
    write_server_info,
    write_server_info_raw,
    a3s::write_server_info
);
file_fns!(
    Changelogs,
    read_changelogs,
    a3s::read_changelogs_graph,
    write_changelogs,
    write_changelogs_raw,
    a3s::write_changelogs
);
file_fns!(
    Events,
    read_events,
    a3s::read_events_graph,
    write_events,
    write_events_raw,
    a3s::write_events
);
file_fns!(
    AutoConfig,
    read_autoconfig,
    a3s::read_autoconfig_graph,
    write_autoconfig,
    write_autoconfig_raw,
    a3s::write_autoconfig
);

/// Any of the five repository metadata files, detected by root class.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum A3sFile {
    Sync(SyncDir),
    ServerInfo(ServerInfo),
    Changelogs(Changelogs),
    Events(Events),
    AutoConfig(AutoConfig),
}

impl A3sFile {
    pub fn from_graph(g: &axs_javaser::Graph) -> Result<Self> {
        let root = g.root().ok_or(Error::NullRoot)?;
        Ok(match g.class_name(root)? {
            a3s::CLS_SYNC_DIR => Self::Sync(a3s::read_sync_graph(g)?),
            a3s::CLS_SERVER_INFO => Self::ServerInfo(a3s::read_server_info_graph(g)?),
            a3s::CLS_CHANGELOGS => Self::Changelogs(a3s::read_changelogs_graph(g)?),
            a3s::CLS_EVENTS => Self::Events(a3s::read_events_graph(g)?),
            a3s::CLS_AUTOCONFIG => Self::AutoConfig(a3s::read_autoconfig_graph(g)?),
            other => {
                return Err(Error::UnexpectedClass {
                    expected: "repository metadata".into(),
                    found: other.into(),
                })
            }
        })
    }

    pub fn to_raw(&self) -> Vec<u8> {
        match self {
            Self::Sync(v) => write_sync_raw(v),
            Self::ServerInfo(v) => write_server_info_raw(v),
            Self::Changelogs(v) => write_changelogs_raw(v),
            Self::Events(v) => write_events_raw(v),
            Self::AutoConfig(v) => write_autoconfig_raw(v),
        }
    }
}
