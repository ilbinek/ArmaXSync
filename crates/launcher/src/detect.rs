//! Locating Steam, the Arma 3 installation, Workshop content, game profiles
//! and RPT logs (Windows and Linux/Proton).

use std::path::{Path, PathBuf};

pub const ARMA3_APP_ID: u32 = 107410;

/// Steam installation root.
pub fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            if let Ok(p) = k.get_value::<String, _>("SteamPath") {
                return Some(PathBuf::from(p));
            }
        }
        if let Ok(k) =
            RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Wow6432Node\\Valve\\Steam")
        {
            if let Ok(p) = k.get_value::<String, _>("InstallPath") {
                return Some(PathBuf::from(p));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        [
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/data/Steam",
        ]
        .iter()
        .map(|p| home.join(p))
        .find(|p| p.join("steamapps").is_dir())
    }
}

/// Library folders from `steamapps/libraryfolders.vdf`.
pub fn steam_libraries() -> Vec<PathBuf> {
    let Some(root) = steam_root() else {
        return vec![];
    };
    let mut libs = vec![root.clone()];
    if let Ok(s) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for line in s.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("\"path\"") {
                let v = rest.trim().trim_matches('"').replace("\\\\", "\\");
                let p = PathBuf::from(v);
                if !libs.contains(&p) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

/// Arma 3 installation folder.
pub fn arma3_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Wow6432Node\\Bohemia Interactive\\Arma 3")
        {
            if let Ok(p) = k.get_value::<String, _>("MAIN") {
                let p = PathBuf::from(p);
                if p.is_dir() {
                    return Some(p);
                }
            }
        }
    }
    steam_libraries()
        .into_iter()
        .map(|l| l.join("steamapps").join("common").join("Arma 3"))
        .find(|p| p.is_dir())
}

/// Default game executable inside an installation folder.
pub fn default_executable(dir: &Path) -> Option<PathBuf> {
    [
        "arma3_x64.exe",
        "arma3.exe",
        "arma3server_x64",
        "arma3server",
    ]
    .iter()
    .map(|e| dir.join(e))
    .find(|p| p.is_file())
}

/// `steamapps/workshop/content/107410` folders.
pub fn workshop_dirs() -> Vec<PathBuf> {
    steam_libraries()
        .into_iter()
        .map(|l| {
            l.join("steamapps")
                .join("workshop")
                .join("content")
                .join(ARMA3_APP_ID.to_string())
        })
        .filter(|p| p.is_dir())
        .collect()
}

/// The Windows-side "Documents" folder Arma uses (Proton prefix on Linux).
pub fn documents_dir(proton: bool) -> Option<PathBuf> {
    if proton {
        steam_libraries()
            .into_iter()
            .map(|l| {
                l.join(format!(
                    "steamapps/compatdata/{ARMA3_APP_ID}/pfx/drive_c/users/steamuser/Documents"
                ))
            })
            .find(|p| p.is_dir())
    } else {
        directories::UserDirs::new().and_then(|u| u.document_dir().map(Path::to_path_buf))
    }
}

/// `%LOCALAPPDATA%\Arma 3` (RPT logs).
pub fn local_appdata_arma(proton: bool) -> Option<PathBuf> {
    if proton {
        return steam_libraries()
            .into_iter()
            .map(|l| l.join(format!("steamapps/compatdata/{ARMA3_APP_ID}/pfx/drive_c/users/steamuser/AppData/Local/Arma 3")))
            .find(|p| p.is_dir());
    }
    directories::BaseDirs::new()
        .map(|b| b.data_local_dir().join("Arma 3"))
        .filter(|p| p.is_dir())
}

/// Game profile names (`-name=`): "Default" plus `Arma 3 - Other Profiles/*`.
pub fn game_profiles(proton: bool) -> Vec<String> {
    let mut v = vec!["Default".to_owned()];
    if let Some(d) = documents_dir(proton) {
        if let Ok(rd) = std::fs::read_dir(d.join("Arma 3 - Other Profiles")) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    let n = e
                        .file_name()
                        .to_string_lossy()
                        .replace("%20", " ")
                        .replace("%2e", ".");
                    v.push(n);
                }
            }
        }
    }
    v
}

/// Allocators available in `<game>/Dll`.
pub fn mallocs(game_dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(game_dir.join("Dll"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.to_lowercase()
                .strip_suffix(".dll")
                .map(|_| n[..n.len() - 4].to_owned())
        })
        .collect();
    v.sort();
    v
}

/// Newest `*.rpt` log.
pub fn latest_rpt(proton: bool) -> Option<PathBuf> {
    let dir = local_appdata_arma(proton)?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("rpt"))
                .unwrap_or(false)
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// Running processes whose executable name matches (case-insensitive).
pub fn is_running(names: &[&str]) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        let n = p.name().to_string_lossy().to_lowercase();
        names.iter().any(|x| n == x.to_lowercase())
    })
}

pub fn arma_running() -> bool {
    is_running(&[
        "arma3.exe",
        "arma3_x64.exe",
        "arma3battleye.exe",
        "arma3server.exe",
        "arma3server_x64.exe",
        "arma3server",
        "arma3server_x64",
    ])
}

/// Remove the `class ModLauncherList { … };` block the official launcher
/// leaves in Arma3.cfg (it overrides `-mod`).
pub fn clean_arma3_cfg(proton: bool) -> std::io::Result<bool> {
    let Some(docs) = documents_dir(proton) else {
        return Ok(false);
    };
    let cfg = docs.join("Arma 3").join("Arma3.cfg");
    let Ok(s) = std::fs::read_to_string(&cfg) else {
        return Ok(false);
    };
    let Some(start) = s.find("class ModLauncherList") else {
        return Ok(false);
    };
    let mut depth = 0i32;
    let mut end = None;
    let bytes = s.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let mut j = i + 1;
                    while j < bytes.len() && bytes[j] != b';' && bytes[j] != b'\n' {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] == b';' {
                        j += 1;
                    }
                    end = Some(j);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let Some(end) = end else { return Ok(false) };
    let cleaned = format!(
        "{}{}",
        &s[..start],
        s[end..].trim_start_matches(['\r', '\n'])
    );
    std::fs::write(&cfg, cleaned)?;
    Ok(true)
}
