//! Java "modified UTF-8" (as used by `DataOutput.writeUTF`).
//!
//! Differences from standard UTF-8: U+0000 is encoded as `C0 80`, and
//! supplementary characters are encoded as two 3-byte surrogate sequences.

use crate::{Error, Result};

/// Decode modified UTF-8 bytes into a Rust string. Unpaired surrogates are
/// replaced with U+FFFD rather than failing, since Java permits them.
pub fn decode_modified_utf8(bytes: &[u8]) -> Result<String> {
    // Fast path: pure ASCII without NUL.
    if bytes.iter().all(|&b| b != 0 && b < 0x80) {
        // SAFETY-free: ASCII is valid UTF-8.
        return Ok(String::from_utf8(bytes.to_vec()).expect("ascii"));
    }
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        if b0 & 0x80 == 0 {
            units.push(b0 as u16);
            i += 1;
        } else if b0 & 0xE0 == 0xC0 {
            let b1 = *bytes.get(i + 1).ok_or(Error::BadUtf8)?;
            if b1 & 0xC0 != 0x80 {
                return Err(Error::BadUtf8);
            }
            units.push((((b0 & 0x1F) as u16) << 6) | (b1 & 0x3F) as u16);
            i += 2;
        } else if b0 & 0xF0 == 0xE0 {
            let b1 = *bytes.get(i + 1).ok_or(Error::BadUtf8)?;
            let b2 = *bytes.get(i + 2).ok_or(Error::BadUtf8)?;
            if b1 & 0xC0 != 0x80 || b2 & 0xC0 != 0x80 {
                return Err(Error::BadUtf8);
            }
            units.push(
                (((b0 & 0x0F) as u16) << 12) | (((b1 & 0x3F) as u16) << 6) | (b2 & 0x3F) as u16,
            );
            i += 3;
        } else {
            return Err(Error::BadUtf8);
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

/// Encode a string as modified UTF-8.
pub fn encode_modified_utf8(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for unit in s.encode_utf16() {
        match unit {
            0x0001..=0x007F => out.push(unit as u8),
            0x0000 | 0x0080..=0x07FF => {
                out.push(0xC0 | ((unit >> 6) & 0x1F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => {
                out.push(0xE0 | ((unit >> 12) & 0x0F) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in ["", "abc", "a\0b", "é", "€uro", "@ACE 🙂 mod", "日本語"] {
            let enc = encode_modified_utf8(s);
            assert_eq!(decode_modified_utf8(&enc).unwrap(), s);
        }
    }

    #[test]
    fn nul_is_two_bytes() {
        assert_eq!(encode_modified_utf8("\0"), vec![0xC0, 0x80]);
    }

    #[test]
    fn supplementary_is_surrogate_pair() {
        let enc = encode_modified_utf8("🙂");
        assert_eq!(enc.len(), 6);
        assert_eq!(enc[0], 0xED);
    }
}
