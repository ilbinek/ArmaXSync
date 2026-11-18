//! Small utilities: bikey extraction, remote completeness check.

use std::path::Path;

use serde::Serialize;

use crate::{Client, Result};

/// Copy every `*.bikey` below `src` (recursively) into `dst` (flat).
/// Returns the copied file names.
pub fn extract_bikeys(src: &Path, dst: &Path) -> std::io::Result<Vec<String>> {
    if !src.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("source directory {} does not exist", src.display()),
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
                let name = e.file_name();
                std::fs::copy(&p, dst.join(&name))?;
                out.push(name.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteCheck {
    pub checked: usize,
    pub missing: Vec<String>,
}

impl Client {
    /// Verify that every file referenced by the remote metadata exists on the
    /// server, including `.zsync` / `.pbo.zip` sidecars (legacy `-check`).
    pub async fn check_remote(&self, name: &str) -> Result<RemoteCheck> {
        use futures::StreamExt as _;
        let r = self.repo(name)?.clone();
        let t = self.transport(&r)?;
        let snap = axs_sync::fetch_snapshot(&t, r.prefer_modern).await?;
        let mut paths = vec![];
        for f in &snap.manifest.files {
            if !(snap.manifest.compressed_only && f.zip_size.is_some()) {
                paths.push(f.path.clone());
            }
            if f.zip_size.is_some() {
                paths.push(format!("{}.zip", f.path));
            }
            if f.zsync && t.is_http() {
                paths.push(format!("{}.zsync", f.path));
            }
        }
        let checked = paths.len();
        let results: Vec<(String, bool)> = futures::stream::iter(paths)
            .map(|p| {
                let t = t.clone();
                async move {
                    let ok = t.exists(&p).await.unwrap_or(false);
                    (p, ok)
                }
            })
            .buffer_unordered(8)
            .collect()
            .await;
        let mut missing: Vec<String> = results
            .into_iter()
            .filter(|(_, ok)| !ok)
            .map(|(p, _)| p)
            .collect();
        missing.sort();
        Ok(RemoteCheck { checked, missing })
    }
}
