//! Local files of a legacy ArmA3Sync client install (read-only, for migration).
//!
//! * `resources/ftp/<name>.a3s.repository` — `GZIP(OOS(SealedObject(Repository)))`
//!   encrypted with AES-128/ECB/PKCS5 and a static key.
//! * `resources/configuration/a3s.cfg` — `Configuration`
//! * `resources/configuration/a3s.prefs` — `Preferences`
//! * `profiles/<name>.a3s.profile` — `Profile`

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyInit};
use axs_javaser::Graph;
use serde::{Deserialize, Serialize};

use crate::a3s::{read_favorite_server, read_protocol, R};
use crate::model::*;
use crate::{Error, Result};

/// Static AES key from `fr.soe.a3s.dao.EncryptionProvider`.
pub const LEGACY_AES_KEY: [u8; 16] = [
    0x01, 0x72, 0x43, 0x3E, 0x1C, 0x7A, 0x55, 0x00, 0x01, 0x72, 0x43, 0x3E, 0x1C, 0x7A, 0x55, 0x4F,
];

type Aes128EcbDec = ecb::Decryptor<aes::Aes128>;

/// Decrypt the `encryptedContent` of a `javax.crypto.SealedObject` and parse
/// the inner serialization stream.
pub fn unseal(g: &Graph) -> Result<Graph> {
    let root = g.root().ok_or(Error::NullRoot)?;
    if g.class_name(root)? != "javax.crypto.SealedObject" {
        return Err(Error::UnexpectedClass {
            expected: "javax.crypto.SealedObject".into(),
            found: g.class_name(root)?.into(),
        });
    }
    let content = g
        .field(root, "encryptedContent")?
        .and_then(|v| v.as_ref())
        .ok_or_else(|| Error::Invalid("SealedObject without encryptedContent".into()))?;
    let mut buf = g.byte_array(content)?.to_vec();
    let plain = Aes128EcbDec::new(&LEGACY_AES_KEY.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(axs_javaser::parse(plain)?)
}

/// Client-side repository settings (`fr.soe.a3s.domain.repository.Repository`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LegacyRepository {
    pub name: String,
    pub protocole: Option<Protocol>,
    pub upload_protocole: Option<Protocol>,
    pub revision: i32,
    pub notify: bool,
    pub auto: bool,
    pub exact_match: bool,
    pub no_auto_discover: bool,
    pub default_download_location: Option<String>,
    pub hidden_folder_paths: Vec<String>,
    pub number_of_client_connections: i32,
    pub maximum_client_download_speed: f64,
    // admin side
    pub path: Option<String>,
    pub number_of_connections: i32,
    pub compressed: bool,
    pub no_partial_file_transfer: bool,
    pub upload_compressed_pbo_files_only: bool,
    pub auto_config_url: Option<String>,
    pub excluded_files_from_build: Vec<String>,
    pub excluded_folders_from_sync: Vec<String>,
    pub favorite_servers_set_to_autoconfig: Vec<FavoriteServer>,
}

/// Read a legacy `*.a3s.repository` file (gzipped, sealed and encrypted).
pub fn read_repository_file(gz: &[u8]) -> Result<LegacyRepository> {
    let outer = crate::parse_gz(gz)?;
    let inner = unseal(&outer)?;
    read_repository_graph(&inner)
}

pub fn read_repository_graph(g: &Graph) -> Result<LegacyRepository> {
    let r = R { g };
    let h = g.root().ok_or(Error::NullRoot)?;
    r.expect_class(h, "fr.soe.a3s.domain.repository.Repository")?;
    let proto = |name: &str| -> Result<Option<Protocol>> {
        match r.obj(h, name)? {
            Some(p) => Ok(Some(read_protocol(&r, p)?)),
            None => Ok(None),
        }
    };
    let mut favs = vec![];
    for s in r
        .items(r.obj(h, "favoriteServersSetToAutoconfig")?)?
        .into_iter()
        .flatten()
    {
        favs.push(read_favorite_server(&r, s)?);
    }
    let set = |name: &str| -> Result<Vec<String>> {
        let mut v = r.strings(r.obj(h, name)?)?;
        v.sort();
        Ok(v)
    };
    Ok(LegacyRepository {
        name: r.string(h, "name")?.unwrap_or_default(),
        protocole: proto("protocole")?,
        upload_protocole: proto("uploadProtocole")?,
        revision: r.i32(h, "revision", 0)?,
        notify: r.bool(h, "notify")?,
        auto: r.bool(h, "auto")?,
        exact_match: r.bool(h, "exactMatch")?,
        no_auto_discover: r.bool(h, "noAutoDiscover")?,
        default_download_location: r.string(h, "defaultDownloadLocation")?,
        hidden_folder_paths: set("hidddenFolderPaths")?,
        number_of_client_connections: r.i32(h, "numberOfClientConnections", 0)?,
        maximum_client_download_speed: r.f64(h, "maximumClientDownloadSpeed")?,
        path: r.string(h, "path")?,
        number_of_connections: r.i32(h, "numberOfConnections", 1)?,
        compressed: r.bool(h, "compressed")?,
        no_partial_file_transfer: r.bool(h, "noPartialFileTransfer")?,
        upload_compressed_pbo_files_only: r.bool(h, "uploadCompressedPboFilesOnly")?,
        auto_config_url: r.string(h, "autoConfigURL")?,
        excluded_files_from_build: set("excludedFilesFromBuild")?,
        excluded_folders_from_sync: set("excludedFoldersFromSync")?,
        favorite_servers_set_to_autoconfig: favs,
    })
}

/// `fr.soe.a3s.domain.TreeDirectory` / `TreeLeaf` (addon groups).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LegacyTreeNode {
    Dir {
        name: String,
        selected: bool,
        /// `REPOSITORY` / `EVENT` for generated modsets.
        modset_type: Option<String>,
        modset_repository: Option<String>,
        children: Vec<LegacyTreeNode>,
    },
    Leaf {
        name: String,
        selected: bool,
        optional: bool,
    },
}

/// `fr.soe.a3s.domain.configration.LauncherOptions`
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LegacyLauncherOptions {
    pub game_profile: Option<String>,
    pub show_script_errors: bool,
    pub no_pause: bool,
    pub file_patching: bool,
    pub window_mode: bool,
    pub check_signatures: bool,
    pub auto_restart: bool,
    pub max_memory: Option<String>,
    pub cpu_count: i32,
    pub ex_threads: Option<String>,
    pub malloc: Option<String>,
    pub enable_ht: bool,
    pub huge_pages: bool,
    pub no_splash: bool,
    pub default_world: bool,
    pub no_logs: bool,
    pub arma3_exe_path: Option<String>,
}

/// `fr.soe.a3s.domain.Profile`
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LegacyProfile {
    pub name: String,
    pub additional_parameters: Option<String>,
    pub addon_names_by_priority: Vec<String>,
    pub addon_search_directories: Vec<String>,
    pub launcher_options: LegacyLauncherOptions,
    /// Children of the root ("racine") group folder.
    pub groups: Vec<LegacyTreeNode>,
}

fn read_tree(r: &R, h: axs_javaser::Handle, depth: usize) -> Result<LegacyTreeNode> {
    if depth > 200 {
        return Err(Error::Invalid("group tree too deep".into()));
    }
    let class = r.g.class_name(h)?.to_owned();
    if class.ends_with("TreeLeaf") {
        return Ok(LegacyTreeNode::Leaf {
            name: r.string(h, "name")?.unwrap_or_default(),
            selected: r.bool(h, "selected")?,
            optional: r.bool(h, "optional")?,
        });
    }
    let mut children = vec![];
    for c in r.items(r.obj(h, "list")?)?.into_iter().flatten() {
        children.push(read_tree(r, c, depth + 1)?);
    }
    Ok(LegacyTreeNode::Dir {
        name: r.string(h, "name")?.unwrap_or_default(),
        selected: r.bool(h, "selected")?,
        modset_type: r.enum_name(r.obj(h, "modsetType")?)?,
        modset_repository: r.string(h, "modsetRepositoryName")?,
        children,
    })
}

/// Read a legacy `*.a3s.profile` file.
pub fn read_profile_file(gz: &[u8]) -> Result<LegacyProfile> {
    let g = crate::parse_gz(gz)?;
    let r = R { g: &g };
    let h = g.root().ok_or(Error::NullRoot)?;
    r.expect_class(h, "fr.soe.a3s.domain.Profile")?;
    let lo = r.obj(h, "launcherOptions")?;
    let launcher_options = match lo {
        Some(o) => LegacyLauncherOptions {
            game_profile: r.string(o, "gameProfile")?,
            show_script_errors: r.bool(o, "showScriptErrors")?,
            no_pause: r.bool(o, "noPause")?,
            file_patching: r.bool(o, "filePatching")?,
            window_mode: r.bool(o, "windowMode")?,
            check_signatures: r.bool(o, "checkSignatures")?,
            auto_restart: r.bool(o, "autoRestart")?,
            max_memory: r.string(o, "maxMemorySelection")?,
            cpu_count: r.i32(o, "cpuCountSelection", 0)?,
            ex_threads: r.string(o, "exThreadsSelection")?,
            malloc: r.string(o, "mallocSelection")?,
            enable_ht: r.bool(o, "enableHT")?,
            huge_pages: r.bool(o, "hugePages")?,
            no_splash: r.bool(o, "noSplashScreen")?,
            default_world: r.bool(o, "defaultWorld")?,
            no_logs: r.bool(o, "nologs")?,
            arma3_exe_path: r.string(o, "arma3ExePath")?,
        },
        None => LegacyLauncherOptions::default(),
    };
    let groups = match r.obj(h, "tree")? {
        Some(t) => match read_tree(&r, t, 0)? {
            LegacyTreeNode::Dir { children, .. } => children,
            leaf => vec![leaf],
        },
        None => vec![],
    };
    Ok(LegacyProfile {
        name: r.string(h, "name")?.unwrap_or_else(|| "Default".into()),
        additional_parameters: r.string(h, "additionalParameters")?,
        addon_names_by_priority: r.string_list(h, "addonNamesByPriority")?,
        addon_search_directories: r.strings(r.obj(h, "addonSearchDirectories")?)?,
        launcher_options,
        groups,
    })
}

/// Parts of `a3s.cfg` worth importing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LegacyConfiguration {
    pub profile_name: Option<String>,
    pub server_name: Option<String>,
    pub favorite_servers: Vec<FavoriteServer>,
    /// (name, executable, parameters, enabled)
    pub external_applications: Vec<(String, String, String, bool)>,
}

pub fn read_configuration_file(gz: &[u8]) -> Result<LegacyConfiguration> {
    let g = crate::parse_gz(gz)?;
    let r = R { g: &g };
    let h = g.root().ok_or(Error::NullRoot)?;
    let mut favorite_servers = vec![];
    for s in r.items(r.obj(h, "favoriteServers")?)?.into_iter().flatten() {
        favorite_servers.push(read_favorite_server(&r, s)?);
    }
    let mut apps = vec![];
    for a in r
        .items(r.obj(h, "externalApplications")?)?
        .into_iter()
        .flatten()
    {
        apps.push((
            r.string(a, "name")?.unwrap_or_default(),
            r.string(a, "executablePath")?.unwrap_or_default(),
            r.string(a, "parameters")?.unwrap_or_default(),
            r.bool(a, "enable")?,
        ));
    }
    Ok(LegacyConfiguration {
        profile_name: r.string(h, "profileName")?,
        server_name: r.string(h, "serverName")?,
        favorite_servers,
        external_applications: apps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shipped_sample_profiles() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../arma3-sync/ArmA3Sync/profiles");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut n = 0;
        for e in rd.flatten() {
            let p = read_profile_file(&std::fs::read(e.path()).unwrap()).unwrap();
            assert!(!p.name.is_empty());
            n += 1;
        }
        assert!(n >= 1);
    }
}
