//! Keys, server mod folders and the RPT viewer.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::addons::Addon;

/// Copy every `*.bikey` below `src` into `dst` (flat). Returns file names.
pub fn extract_bikeys(src: &Path, dst: &Path) -> std::io::Result<Vec<String>> {
    if !src.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} does not exist", src.display()),
        ));
    }
    std::fs::create_dir_all(dst)?;
    let mut out = vec![];
    let mut stack = vec![src.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .map(|x| x.eq_ignore_ascii_case("bikey"))
                .unwrap_or(false)
            {
                std::fs::copy(&p, dst.join(e.file_name()))?;
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkMode {
    /// Symbolic links (junctions on Windows): no extra space.
    #[default]
    Link,
    /// Full copies.
    Copy,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerSetup {
    pub mods: Vec<PathBuf>,
    pub server_mods: Vec<PathBuf>,
    pub keys: Vec<String>,
    pub parameters: Vec<String>,
}

fn copy_dir(src: &Path, dst: &Path, lowercase: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let mut name = e.file_name().to_string_lossy().into_owned();
        if lowercase {
            name = name.to_lowercase();
        }
        let to = dst.join(name);
        if e.path().is_dir() {
            copy_dir(&e.path(), &to, lowercase)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

fn link_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)
    }
    #[cfg(windows)]
    {
        // Junctions need no privileges; fall back to symlink_dir.
        let st = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(dst)
            .arg(src)
            .stdout(std::process::Stdio::null())
            .status()?;
        if st.success() {
            Ok(())
        } else {
            std::os::windows::fs::symlink_dir(src, dst)
        }
    }
}

/// Prepare a dedicated-server folder: link/copy the selected mods into
/// `target`, copy their keys into `<server>/keys`, and build `-mod` /
/// `-serverMod` parameters.
pub fn prepare_server(
    addons: &BTreeMap<String, Addon>,
    mods: &[String],
    server_mods: &[String],
    server_dir: &Path,
    target: &Path,
    mode: LinkMode,
    lowercase: bool,
) -> std::io::Result<ServerSetup> {
    std::fs::create_dir_all(target)?;
    let place = |names: &[String]| -> std::io::Result<Vec<PathBuf>> {
        let mut v = vec![];
        for n in names {
            let a = addons.get(&n.to_lowercase()).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("addon {n} not found"))
            })?;
            let folder = if lowercase {
                a.name.to_lowercase()
            } else {
                a.name.clone()
            };
            let dst = target.join(&folder);
            if dst.exists() || dst.symlink_metadata().is_ok() {
                if dst
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false)
                    || mode == LinkMode::Link
                {
                    let _ = std::fs::remove_file(&dst).or_else(|_| std::fs::remove_dir(&dst));
                } else {
                    std::fs::remove_dir_all(&dst)?;
                }
            }
            match mode {
                LinkMode::Link if !lowercase => link_dir(&a.path, &dst)?,
                _ => copy_dir(&a.path, &dst, lowercase)?,
            }
            v.push(dst);
        }
        Ok(v)
    };
    let m = place(mods)?;
    let s = place(server_mods)?;
    let keys_dir = server_dir.join("keys");
    let mut keys = vec![];
    for p in m.iter().chain(s.iter()) {
        if let Ok(k) = extract_bikeys(p, &keys_dir) {
            keys.extend(k);
        }
    }
    keys.sort();
    keys.dedup();
    let rel = |p: &PathBuf| -> PathBuf {
        p.strip_prefix(server_dir)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| p.clone())
    };
    let rm: Vec<PathBuf> = m.iter().map(rel).collect();
    let rs: Vec<PathBuf> = s.iter().map(rel).collect();
    let parameters =
        crate::params::server_mod_line(&rm, &rs, &|p| p.to_string_lossy().replace('\\', "/"));
    Ok(ServerSetup {
        mods: m,
        server_mods: s,
        keys,
        parameters,
    })
}

/// Follow a growing log file (RPT viewer). Calls `on_text` with new content
/// until it returns false. Restarts from the beginning if the file shrinks.
pub fn tail(
    path: &Path,
    from_start: bool,
    mut on_text: impl FnMut(&str) -> bool,
) -> std::io::Result<()> {
    let mut f = std::fs::File::open(path)?;
    let mut pos = if from_start { 0 } else { f.metadata()?.len() };
    let mut buf = Vec::new();
    loop {
        let len = f.metadata()?.len();
        if len < pos {
            pos = 0;
        }
        if len > pos {
            f.seek(SeekFrom::Start(pos))?;
            buf.clear();
            (&mut f).take(len - pos).read_to_end(&mut buf)?;
            pos = len;
            if !on_text(&String::from_utf8_lossy(&buf)) {
                return Ok(());
            }
        } else if !on_text("") {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
