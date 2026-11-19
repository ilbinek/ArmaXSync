//! Self-update from GitHub releases (tauri-plugin-updater, minisign-signed).
//!
//! The release workflow publishes `latest.json` next to the installers; the
//! stable channel reads `releases/latest/download/latest.json`, which skips
//! pre-releases. Windows (NSIS) and AppImage installs update in place; deb/rpm,
//! portable and dev builds only get a notification with a download link.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;

/// GitHub `owner/repo` that publishes releases.
pub const RELEASE_REPO: &str = match option_env!("ARMAXSYNC_RELEASE_REPO") {
    Some(r) => r,
    None => "ilbinek/ArmaXSync",
};

#[derive(Default)]
pub struct UpdaterState {
    pending: Mutex<Option<Update>>,
}

/// How this copy of the app can be updated.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    /// The updater replaces the installation (NSIS / AppImage).
    Auto,
    /// Installed by a package manager or portable: download manually.
    Manual,
    /// Development build: check only.
    Dev,
}

pub fn install_kind(portable: bool) -> InstallKind {
    if cfg!(debug_assertions) {
        return InstallKind::Dev;
    }
    if portable {
        return InstallKind::Manual;
    }
    if cfg!(windows) {
        return InstallKind::Auto;
    }
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some() {
        return InstallKind::Auto;
    }
    InstallKind::Manual
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    current: String,
    version: String,
    notes: String,
    date: Option<String>,
    kind: InstallKind,
    /// Release page with the installers.
    url: String,
}

#[derive(Serialize, Clone)]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
    phase: &'static str,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn release_page(version: &str) -> String {
    format!("https://github.com/{RELEASE_REPO}/releases/tag/v{version}")
}

fn updater(app: &AppHandle, proxy: Option<&str>) -> Result<tauri_plugin_updater::Updater, String> {
    let mut b = app.updater_builder();
    // Test hook: point at a local latest.json. The signature check still applies.
    if let Ok(ep) = std::env::var("ARMAXSYNC_UPDATE_ENDPOINT") {
        b = b.endpoints(vec![ep.parse().map_err(err)?]).map_err(err)?;
    }
    if let Some(p) = proxy.filter(|p| !p.is_empty()) {
        b = b.proxy(p.parse().map_err(err)?);
    }
    b.build().map_err(err)
}

/// Look for a newer release. `None` when up to date.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    st: State<'_, AppState>,
    ust: State<'_, UpdaterState>,
) -> Result<Option<UpdateInfo>, String> {
    let c = axs_client::Client::open(st.dirs.clone()).map_err(err)?;
    let kind = install_kind(c.dirs.portable);
    let update = updater(&app, c.config.proxy.as_deref())?
        .check()
        .await
        .map_err(err)?;
    let info = update.as_ref().map(|u| UpdateInfo {
        current: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default(),
        date: u.date.map(|d| d.date().to_string()),
        kind,
        url: release_page(&u.version),
    });
    *ust.pending.lock().unwrap() = update;
    Ok(info)
}

/// Download, verify and install the update found by [`update_check`], then restart.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    st: State<'_, AppState>,
    ust: State<'_, UpdaterState>,
) -> Result<(), String> {
    let c = axs_client::Client::open(st.dirs.clone()).map_err(err)?;
    if install_kind(c.dirs.portable) != InstallKind::Auto {
        return Err("This installation cannot update itself. Download the new version from the release page.".into());
    }
    if !st.syncs.lock().unwrap().is_empty() {
        return Err("Wait for the running downloads to finish before updating.".into());
    }
    if axs_launcher::detect::arma_running() {
        return Err("Close Arma 3 before updating.".into());
    }
    let update = ust
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or("No update found. Check for updates first.")?;

    let mut downloaded = 0u64;
    let h = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = h.emit(
                    "update-progress",
                    UpdateProgress {
                        downloaded,
                        total,
                        phase: "download",
                    },
                );
            },
            || {
                let _ = app.emit(
                    "update-progress",
                    UpdateProgress {
                        downloaded: 0,
                        total: None,
                        phase: "install",
                    },
                );
            },
        )
        .await
        .map_err(err)?;
    // Windows: the installer already closed and relaunches the app.
    app.restart();
}
