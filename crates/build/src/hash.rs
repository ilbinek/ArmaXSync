//! Single-pass file hashing (SHA-1, BLAKE3, zsync block sums) and the
//! build hash cache.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use axs_zsync::{BlockSummer, FullBlockSum};

#[derive(Debug, Clone)]
pub struct FileHashes {
    /// Lowercase hex, `"0"` for empty files (legacy convention).
    pub sha1: String,
    pub blake3: String,
    pub block_sums: Option<Vec<FullBlockSum>>,
}

pub fn hex(b: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(HEX[(x >> 4) as usize] as char);
        s.push(HEX[(x & 15) as usize] as char);
    }
    s
}

/// Hash a file in one pass. `block_size` enables zsync block sums.
pub fn hash_file(path: &Path, block_size: Option<usize>) -> std::io::Result<FileHashes> {
    let mut f = std::fs::File::open(path)?;
    let mut sha = Sha1::new();
    let mut b3 = blake3::Hasher::new();
    let mut summer = block_size.map(BlockSummer::new);
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        sha.update(&buf[..n]);
        b3.update(&buf[..n]);
        if let Some(s) = summer.as_mut() {
            s.update(&buf[..n]);
        }
    }
    Ok(FileHashes {
        sha1: if total == 0 {
            "0".into()
        } else {
            hex(&sha.finalize())
        },
        blake3: b3.finalize().to_hex().to_string(),
        block_sums: summer.map(|s| s.finish()),
    })
}

/// Cached hashes of one file, valid while size and mtime are unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedHash {
    pub size: u64,
    pub mtime_ns: i128,
    pub sha1: String,
    pub blake3: String,
    /// SHA-1 recorded in the current `.zsync` sidecar (if we wrote it).
    #[serde(default)]
    pub zsync_sha1: Option<String>,
    /// SHA-1 of the content the current `.pbo.zip` was made from.
    #[serde(default)]
    pub zip_sha1: Option<String>,
}

/// Storage for build hashes, keyed by repository-relative path.
pub trait HashCache: Send {
    fn get(&self, rel: &str) -> Option<CachedHash>;
    fn put(&mut self, rel: &str, v: CachedHash);
    /// Drop entries whose path is not in `keep`; returns the number removed.
    fn retain(&mut self, keep: &dyn Fn(&str) -> bool) -> usize;
    fn flush(&mut self) -> std::io::Result<()>;
}

/// In-memory cache persisted as zstd-compressed JSON.
#[derive(Debug, Default)]
pub struct FileHashCache {
    path: Option<PathBuf>,
    map: HashMap<String, CachedHash>,
    dirty: bool,
}

impl FileHashCache {
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Load from `path` (missing or corrupt files give an empty cache).
    pub fn load(path: PathBuf) -> Self {
        let map = std::fs::read(&path)
            .ok()
            .and_then(|z| zstd::decode_all(&z[..]).ok())
            .and_then(|j| serde_json::from_slice(&j).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            map,
            dirty: false,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl HashCache for FileHashCache {
    fn get(&self, rel: &str) -> Option<CachedHash> {
        self.map.get(rel).cloned()
    }
    fn put(&mut self, rel: &str, v: CachedHash) {
        if self.map.get(rel) != Some(&v) {
            self.map.insert(rel.to_owned(), v);
            self.dirty = true;
        }
    }
    fn retain(&mut self, keep: &dyn Fn(&str) -> bool) -> usize {
        let before = self.map.len();
        self.map.retain(|k, _| keep(k));
        let removed = before - self.map.len();
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec(&self.map).map_err(std::io::Error::other)?;
        let z = zstd::encode_all(&json[..], 3)?;
        crate::write_atomic(path, &z)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_sha1_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("e");
        std::fs::write(&p, b"").unwrap();
        let h = hash_file(&p, Some(8192)).unwrap();
        assert_eq!(h.sha1, "0");
        assert_eq!(h.block_sums.unwrap().len(), 0);
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            hash_file(&p, None).unwrap().sha1,
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }
}
