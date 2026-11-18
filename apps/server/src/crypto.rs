//! Password hashing, random tokens and at-rest encryption of repository
//! credentials (which must be recoverable: legacy autoconfig embeds them).

use std::path::Path;

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("argon2")
        .to_string()
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// URL-safe random token with `bytes` of entropy.
pub fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

pub fn sha256_hex(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time string comparison.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Symmetric box for secrets stored in the database.
#[derive(Clone)]
pub struct SecretBox {
    cipher: XChaCha20Poly1305,
}

impl SecretBox {
    /// Load `<data>/secret.key`, creating it on first start.
    pub fn load_or_create(path: &Path) -> anyhow::Result<Self> {
        let key = match std::fs::read(path) {
            Ok(k) if k.len() == 32 => k,
            Ok(_) => anyhow::bail!("{} is corrupt (expected 32 bytes)", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut k = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut k);
                if let Some(d) = path.parent() {
                    std::fs::create_dir_all(d)?;
                }
                std::fs::write(path, &k)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                }
                k
            }
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            cipher: XChaCha20Poly1305::new_from_slice(&key).expect("32-byte key"),
        })
    }

    pub fn ephemeral() -> Self {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        Self {
            cipher: XChaCha20Poly1305::new_from_slice(&k).expect("key"),
        }
    }

    pub fn seal(&self, plain: &str) -> String {
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher
            .encrypt(XNonce::from_slice(&nonce), plain.as_bytes())
            .expect("encrypt");
        let mut out = nonce.to_vec();
        out.extend(ct);
        STANDARD.encode(out)
    }

    pub fn open(&self, sealed: &str) -> Option<String> {
        let raw = STANDARD.decode(sealed).ok()?;
        if raw.len() < 24 {
            return None;
        }
        let pt = self
            .cipher
            .decrypt(XNonce::from_slice(&raw[..24]), &raw[24..])
            .ok()?;
        String::from_utf8(pt).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_box() {
        let h = hash_password("hunter2");
        assert!(verify_password("hunter2", &h));
        assert!(!verify_password("hunter3", &h));
        let b = SecretBox::ephemeral();
        let s = b.seal("p@ss€");
        assert_eq!(b.open(&s).as_deref(), Some("p@ss€"));
        assert!(ct_eq("abc", "abc") && !ct_eq("abc", "abd"));
    }
}
