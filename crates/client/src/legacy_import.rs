//! One-shot import of a legacy ArmA3Sync installation.

use std::path::{Path, PathBuf};

use serde::Serialize;

use axs_legacy::client::LegacyRepository;

use crate::config::{AdminSettings, RepoConfig, RepoState};
use crate::{Client, Result};

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub repositories: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

/// Locate the legacy `resources/ftp` folder inside an install directory.
pub fn legacy_repo_files(install: &Path) -> Vec<PathBuf> {
    let dir = install.join("resources").join("ftp");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().ends_with(".a3s.repository"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Convert a legacy repository to the new configuration.
pub fn convert_repository(l: &LegacyRepository) -> Option<RepoConfig> {
    let p = l.protocole.as_ref()?;
    let url = axs_legacy::urls::base_url(p);
    let ms_to_s = |v: &Option<String>| -> Option<u64> {
        v.as_deref()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|ms| ms / 1000)
    };
    let admin = l.path.as_ref().map(|path| AdminSettings {
        path: Some(PathBuf::from(path)),
        public_url: None,
        compress: l.compressed,
        compressed_only: l.upload_compressed_pbo_files_only,
        connections: l.number_of_connections.clamp(1, 10) as u32,
        partial_transfer: !l.no_partial_file_transfer,
        embed_credentials: true,
        exclude: l
            .excluded_files_from_build
            .iter()
            .filter_map(|f| relative_to(f, path))
            .collect(),
        hidden: l
            .excluded_folders_from_sync
            .iter()
            .filter_map(|f| relative_to(f, path))
            .collect(),
        servers: l
            .favorite_servers_set_to_autoconfig
            .iter()
            .map(axs_model::ServerDef::from)
            .collect(),
        ..Default::default()
    });
    Some(RepoConfig {
        name: l.name.clone(),
        url,
        login: p
            .login
            .clone()
            .filter(|x| !x.eq_ignore_ascii_case("anonymous") && !x.is_empty()),
        password: p.password.clone().filter(|x| !x.is_empty()),
        destination: l.default_download_location.as_ref().map(PathBuf::from),
        exact_match: l.exact_match,
        auto_discover: !l.no_auto_discover,
        notify: l.notify,
        auto: l.auto,
        connections: l.number_of_client_connections.max(0) as u32,
        // Legacy stored MB/s as a double in the UI layer; stored value is bytes/s.
        max_speed: if l.maximum_client_download_speed > 0.0 {
            l.maximum_client_download_speed as u64
        } else {
            0
        },
        connect_timeout_secs: ms_to_s(&p.connection_timeout).filter(|&s| s > 0),
        read_timeout_secs: ms_to_s(&p.read_timeout).filter(|&s| s > 0),
        keep_extras: l.hidden_folder_paths.clone(),
        admin,
        state: RepoState {
            revision: Some(l.revision.max(0) as u32).filter(|&r| r > 0),
            ..Default::default()
        },
        ..Default::default()
    })
}

fn relative_to(abs: &str, root: &str) -> Option<String> {
    let a = abs.replace('\\', "/");
    let r = root.replace('\\', "/");
    let r = r.trim_end_matches('/');
    if a.len() > r.len() && a[..r.len()].eq_ignore_ascii_case(r) {
        Some(a[r.len()..].trim_start_matches('/').to_owned()).filter(|s| !s.is_empty())
    } else {
        None
    }
}

/// Import all legacy repositories of an install directory.
pub fn import_repositories(
    client: &mut Client,
    install: &Path,
    overwrite: bool,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for f in legacy_repo_files(install) {
        let fname = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let data = std::fs::read(&f)?;
        let legacy = match axs_legacy::client::read_repository_file(&data) {
            Ok(l) => l,
            Err(e) => {
                report.skipped.push((fname, e.to_string()));
                continue;
            }
        };
        let Some(r) = convert_repository(&legacy) else {
            report.skipped.push((fname, "no protocol".into()));
            continue;
        };
        if !overwrite && client.repo(&r.name).is_ok() {
            report
                .skipped
                .push((r.name.clone(), "already exists".into()));
            continue;
        }
        report.repositories.push(r.name.clone());
        client.save_repo(r)?;
    }
    Ok(report)
}

/// Convert a legacy launcher profile.
pub fn convert_profile(l: &axs_legacy::client::LegacyProfile) -> axs_launcher::Profile {
    use axs_launcher::{GroupNode, LauncherOptions, ModsetRef, Profile};
    use axs_legacy::client::LegacyTreeNode;
    fn node(n: &LegacyTreeNode) -> GroupNode {
        match n {
            LegacyTreeNode::Leaf {
                name,
                selected,
                optional,
            } => GroupNode::Addon {
                name: name.trim_end_matches('*').to_owned(),
                selected: *selected,
                optional: *optional,
            },
            LegacyTreeNode::Dir {
                name,
                children,
                modset_type,
                modset_repository,
                ..
            } => GroupNode::Group {
                name: name.clone(),
                children: children.iter().map(node).collect(),
                modset: match (modset_type.as_deref(), modset_repository) {
                    (Some("REPOSITORY"), Some(r)) => Some(ModsetRef::Repository {
                        repository: r.clone(),
                    }),
                    (Some("EVENT"), Some(r)) => Some(ModsetRef::Event {
                        repository: r.clone(),
                        event: name.clone(),
                    }),
                    _ => None,
                },
            },
        }
    }
    let o = &l.launcher_options;
    let num = |s: &Option<String>| s.as_deref().and_then(|v| v.trim().parse::<u32>().ok());
    Profile {
        name: l.name.clone(),
        executable: o.arma3_exe_path.as_ref().map(PathBuf::from),
        options: LauncherOptions {
            game_profile: o.game_profile.clone(),
            show_script_errors: o.show_script_errors,
            no_pause: o.no_pause,
            window_mode: o.window_mode,
            file_patching: o.file_patching,
            check_signatures: o.check_signatures,
            max_mem: num(&o.max_memory),
            cpu_count: (o.cpu_count > 0).then_some(o.cpu_count as u32),
            ex_threads: num(&o.ex_threads),
            malloc: o.malloc.clone().filter(|m| !m.is_empty()),
            enable_ht: o.enable_ht,
            huge_pages: o.huge_pages,
            no_splash: o.no_splash,
            default_world_empty: o.default_world,
            no_logs: o.no_logs,
            auto_restart: o.auto_restart,
        },
        additional_parameters: l.additional_parameters.clone().unwrap_or_default(),
        addon_search_dirs: l
            .addon_search_directories
            .iter()
            .map(PathBuf::from)
            .collect(),
        addon_priority: l.addon_names_by_priority.clone(),
        groups: l.groups.iter().map(node).collect(),
        ..Default::default()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct FullImportReport {
    pub repositories: ImportReport,
    pub profiles: Vec<String>,
    pub favorite_servers: usize,
    pub external_apps: usize,
    pub errors: Vec<(String, String)>,
}

/// Import repositories, launcher profiles and configuration of a legacy install.
pub fn import_all(
    client: &mut Client,
    install: &Path,
    overwrite: bool,
) -> Result<FullImportReport> {
    let mut rep = FullImportReport {
        repositories: import_repositories(client, install, overwrite)?,
        ..Default::default()
    };
    let store = client.profiles();
    for e in std::fs::read_dir(install.join("profiles"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let n = e.file_name().to_string_lossy().into_owned();
        if !n.ends_with(".a3s.profile") {
            continue;
        }
        let parsed = std::fs::read(e.path())
            .map_err(|e| e.to_string())
            .and_then(|d| axs_legacy::client::read_profile_file(&d).map_err(|e| e.to_string()));
        match parsed {
            Ok(lp) => {
                let exists = store.list().iter().any(|p| p.name == lp.name);
                if exists && !overwrite && lp.name != "Default" {
                    continue;
                }
                let p = convert_profile(&lp);
                store.save(&p)?;
                rep.profiles.push(p.name);
            }
            Err(err) => rep.errors.push((n, err)),
        }
    }
    let cfg = install
        .join("resources")
        .join("configuration")
        .join("a3s.cfg");
    if let Ok(data) = std::fs::read(&cfg) {
        match axs_legacy::client::read_configuration_file(&data) {
            Ok(c) => {
                for s in &c.favorite_servers {
                    let fav = axs_launcher::FavoriteServer {
                        name: s.name.clone().unwrap_or_default(),
                        host: s.ip_address.clone().unwrap_or_default(),
                        port: s.port.clamp(0, 65535) as u16,
                        password: s.password.clone().filter(|p| !p.is_empty()),
                        modset: s.modset_name.clone().filter(|p| !p.is_empty()),
                        repository: s.repository_name.clone(),
                    };
                    if !client
                        .config
                        .favorite_servers
                        .iter()
                        .any(|f| f.name == fav.name)
                    {
                        client.config.favorite_servers.push(fav);
                        rep.favorite_servers += 1;
                    }
                }
                for (name, exe, params, enabled) in &c.external_applications {
                    if !client.config.external_apps.iter().any(|a| &a.name == name) {
                        client.config.external_apps.push(axs_launcher::ExternalApp {
                            name: name.clone(),
                            path: PathBuf::from(exe),
                            parameters: params.clone(),
                            enabled: *enabled,
                        });
                        rep.external_apps += 1;
                    }
                }
                if client.config.profile.is_none() {
                    client.config.profile = c.profile_name.clone();
                }
                client.save_config()?;
            }
            Err(e) => rep.errors.push(("a3s.cfg".into(), e.to_string())),
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_shipped_legacy_profiles() {
        let install = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../arma3-sync/ArmA3Sync");
        if !install.join("profiles").is_dir() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut c = Client::open(crate::DataDirs::at(dir.path())).unwrap();
        let rep = import_all(&mut c, &install, true).unwrap();
        assert!(rep.profiles.iter().any(|p| p == "Default"), "{rep:?}");
        let p = c.profiles().get("Default").unwrap();
        assert_eq!(p.addon_priority, ["@ace", "@CBA_A3"]);
        assert!(p
            .executable
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .ends_with("arma3.exe"));
    }

    #[test]
    fn imports_real_legacy_repository_file() {
        let install = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/legacy-client");
        let dir = tempfile::tempdir().unwrap();
        let mut c = Client::open(crate::DataDirs::at(dir.path())).unwrap();
        let rep = import_repositories(&mut c, &install, false).unwrap();
        assert_eq!(rep.repositories, vec!["Fixture Repo".to_string()]);
        let r = c.repo("Fixture Repo").unwrap();
        assert_eq!(r.url, "https://mods.example.com:8443/r/main");
        assert_eq!(r.login.as_deref(), Some("player"));
        assert_eq!(r.password.as_deref(), Some("s3cret"));
        assert!(r.admin.as_ref().unwrap().path.is_some());
    }
}
