//! Starting the game and external applications.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use serde::Serialize;

use crate::addons::Addon;
use crate::params::{native_path, proton_path, run_parameters, PathMapper};
use crate::profile::{ExternalApp, FavoriteServer, LaunchMode, Profile};
use crate::LauncherError;

/// The resolved command that will be executed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Game parameters only (for previews / shortcuts).
    pub game_args: Vec<String>,
}

fn is_battleye(exe: &Path) -> bool {
    exe.file_name()
        .map(|n| {
            n.to_string_lossy()
                .eq_ignore_ascii_case("arma3battleye.exe")
        })
        .unwrap_or(false)
}

/// Steam client executable.
pub fn steam_executable() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::detect::steam_root()
            .map(|r| r.join("steam.exe"))
            .filter(|p| p.is_file())
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("PATH")
            .and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join("steam"))
                    .find(|p| p.is_file())
            })
            .or_else(|| Some(PathBuf::from("steam")))
    }
}

/// Build the command for a profile (no side effects).
pub fn command(
    p: &Profile,
    addons: &BTreeMap<String, Addon>,
    server: Option<&FavoriteServer>,
) -> Result<LaunchCommand, LauncherError> {
    let proton = p.proton && !cfg!(windows);
    let map: &PathMapper = if proton { &proton_path } else { &native_path };
    let game_args = run_parameters(p, addons, server, map);
    let via_steam = p.launch_mode == LaunchMode::Steam || proton;
    if via_steam {
        let steam = steam_executable().ok_or(LauncherError::SteamNotFound)?;
        let mut args = vec![
            "-applaunch".to_owned(),
            crate::detect::ARMA3_APP_ID.to_string(),
        ];
        args.extend(game_args.iter().cloned());
        return Ok(LaunchCommand {
            program: steam,
            args,
            game_args,
        });
    }
    let exe = p
        .executable
        .clone()
        .filter(|e| e.is_file())
        .ok_or(LauncherError::ExecutableMissing)?;
    let mut args = vec![];
    if is_battleye(&exe) {
        // Legacy ArmA3Sync: arma3battleye.exe 2 1 <params>
        args.push("2".to_owned());
        args.push("1".to_owned());
    }
    args.extend(game_args.iter().cloned());
    Ok(LaunchCommand {
        program: exe,
        args,
        game_args,
    })
}

fn spawn(program: &Path, args: &[String], cwd: Option<&Path>) -> std::io::Result<Child> {
    let ext = program
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut cmd = match ext.as_str() {
        "bat" | "cmd" if cfg!(windows) => {
            let mut c = Command::new("cmd.exe");
            c.arg("/C").arg(program);
            c
        }
        "sh" if !cfg!(windows) => {
            let mut c = Command::new("/bin/bash");
            c.arg(program);
            c
        }
        _ => Command::new(program),
    };
    cmd.args(args);
    if let Some(d) = cwd.or_else(|| program.parent()) {
        if d.is_dir() {
            cmd.current_dir(d);
        }
    }
    cmd.spawn()
}

/// Start enabled external applications that are not running yet.
pub fn start_external_apps(apps: &[ExternalApp]) -> Vec<(String, String)> {
    let mut errors = vec![];
    for a in apps.iter().filter(|a| a.enabled) {
        if !a.path.is_file() {
            errors.push((a.name.clone(), format!("{} not found", a.path.display())));
            continue;
        }
        let exe = a
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if crate::detect::is_running(&[&exe]) {
            continue;
        }
        let args: Vec<String> = a.parameters.split_whitespace().map(str::to_owned).collect();
        if let Err(e) = spawn(&a.path, &args, None) {
            errors.push((a.name.clone(), e.to_string()));
        }
    }
    errors
}

/// Launch the game. With `auto_restart`, a background thread relaunches it
/// whenever it exits (until the returned handle is dropped... or forever, like
/// the legacy launcher).
pub fn launch(cmd: &LaunchCommand, auto_restart: bool, proton: bool) -> Result<u32, LauncherError> {
    let _ = crate::detect::clean_arma3_cfg(proton);
    let child =
        spawn(&cmd.program, &cmd.args, None).map_err(|e| LauncherError::Spawn(e.to_string()))?;
    let pid = child.id();
    if auto_restart && cmd.args.first().map(|a| a != "-applaunch").unwrap_or(true) {
        let cmd = cmd.clone();
        std::thread::spawn(move || {
            let mut child = child;
            loop {
                let _ = child.wait();
                tracing::info!("game exited; auto-restart");
                std::thread::sleep(std::time::Duration::from_secs(3));
                match spawn(&cmd.program, &cmd.args, None) {
                    Ok(c) => child = c,
                    Err(e) => {
                        tracing::error!("auto-restart failed: {e}");
                        break;
                    }
                }
            }
        });
    }
    Ok(pid)
}
