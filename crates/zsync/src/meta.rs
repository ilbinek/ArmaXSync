//! `.zsync` metafile header + checksum table.

use crate::{Error, FullBlockSum, Result};

/// zsync Hash-Lengths `[seq_matches, rsum_bytes, checksum_bytes]`, computed
/// exactly like `jazsyncmake.MetaFileMaker.analyzeFile` (including its Java
/// double→int conversions and integer division).
pub fn hash_lengths(file_len: u64, block_size: usize) -> [i32; 3] {
    let len = file_len as i64;
    let bs = block_size as i64;
    let lf = len as f64;
    let ln2 = 2f64.ln();
    let seq = if len > bs { 2 } else { 1 };
    let mut rsum = ((lf.ln() + (bs as f64).ln()) / ln2 - 8.6) / 8.0;
    rsum = rsum.ceil();
    let mut rsum = rsum as i32; // Java (int) cast: saturating, NaN -> 0
    rsum = rsum.clamp(2, 4);
    let blocks = (1 + len / bs) as f64;
    let mut strong = ((20.0 + (lf.ln() + blocks.ln()) / ln2) / seq as f64 / 8.0).ceil() as i32;
    let strong2 = ((7.9 + (20.0 + blocks.ln() / ln2)) / 8.0) as i32;
    if strong < strong2 {
        strong = strong2;
    }
    [seq, rsum, strong]
}

/// Java `SimpleDateFormat("EEE, dd MMM yyyy HH:mm:ss Z", Locale.US)` in the
/// local time zone.
pub fn format_mtime(mtime_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(mtime_ms).single() {
        Some(t) => t.format("%a, %d %b %Y %H:%M:%S %z").to_string(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZsyncHeader {
    pub version: String,
    pub filename: String,
    pub mtime: String,
    pub block_size: usize,
    pub length: u64,
    /// `[seq_matches, rsum_bytes, checksum_bytes]`
    pub hash_lengths: [i32; 3],
    pub url: String,
    pub sha1: String,
}

/// Truncated checksums of one block as stored in the metafile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSum {
    /// Last `rsum_bytes` of `a_hi a_lo b_hi b_lo`, as a big-endian integer.
    pub weak: u32,
    pub strong: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZsyncMeta {
    pub header: ZsyncHeader,
    pub blocks: Vec<BlockSum>,
}

impl ZsyncMeta {
    /// Build a metafile from full block sums.
    pub fn new(
        filename: &str,
        mtime_ms: i64,
        length: u64,
        url: &str,
        sha1: &str,
        block_size: usize,
        sums: &[FullBlockSum],
    ) -> Self {
        let hl = hash_lengths(length, block_size);
        let rb = hl[1] as usize;
        let sb = hl[2] as usize;
        let blocks = sums
            .iter()
            .map(|s| {
                let mut strong = [0u8; 16];
                strong[..sb].copy_from_slice(&s.md4[..sb]);
                BlockSum {
                    weak: s.rsum.key(rb),
                    strong,
                }
            })
            .collect();
        Self {
            header: ZsyncHeader {
                version: "jazsync".into(),
                filename: filename.into(),
                mtime: format_mtime(mtime_ms),
                block_size,
                length,
                hash_lengths: hl,
                url: url.into(),
                sha1: sha1.into(),
            },
            blocks,
        }
    }

    pub fn rsum_bytes(&self) -> usize {
        self.header.hash_lengths[1] as usize
    }
    pub fn strong_bytes(&self) -> usize {
        self.header.hash_lengths[2] as usize
    }
    pub fn seq_matches(&self) -> usize {
        self.header.hash_lengths[0] as usize
    }

    /// Serialize exactly like jazsync (`\n` line endings, blank line, binary sums).
    pub fn to_bytes(&self) -> Vec<u8> {
        let h = &self.header;
        let mut out = format!(
            "zsync: {}\nFilename: {}\nMTime: {}\nBlocksize: {}\nLength: {}\nHash-Lengths: {},{},{}\nURL: {}\nSHA-1: {}\n\n",
            h.version,
            h.filename,
            h.mtime,
            h.block_size,
            h.length,
            h.hash_lengths[0],
            h.hash_lengths[1],
            h.hash_lengths[2],
            h.url,
            h.sha1
        )
        .into_bytes();
        let rb = self.rsum_bytes();
        let sb = self.strong_bytes();
        out.reserve(self.blocks.len() * (rb + sb));
        for b in &self.blocks {
            out.extend_from_slice(&b.weak.to_be_bytes()[4 - rb..]);
            out.extend_from_slice(&b.strong[..sb]);
        }
        out
    }

    /// Parse a metafile (any zsync-compatible header; unknown keys ignored).
    pub fn parse(data: &[u8]) -> Result<Self> {
        let split = data
            .windows(2)
            .position(|w| w == b"\n\n")
            .ok_or_else(|| Error::Header("missing header terminator".into()))?;
        let head = String::from_utf8_lossy(&data[..split]);
        let body = &data[split + 2..];
        let mut h = ZsyncHeader {
            version: String::new(),
            filename: String::new(),
            mtime: String::new(),
            block_size: 0,
            length: 0,
            hash_lengths: [1, 4, 16],
            url: String::new(),
            sha1: String::new(),
        };
        for line in head.lines() {
            let line = line.trim_end_matches('\r');
            let Some((k, v)) = line.split_once(':') else {
                return Err(Error::Header(format!("bad line {line:?}")));
            };
            let v = v.strip_prefix(' ').unwrap_or(v);
            let bad = |what: &str| Error::Header(format!("bad {what}: {v:?}"));
            match k.to_ascii_lowercase().as_str() {
                "zsync" => h.version = v.into(),
                "filename" => h.filename = v.into(),
                "mtime" => h.mtime = v.into(),
                "blocksize" => h.block_size = v.trim().parse().map_err(|_| bad("blocksize"))?,
                "length" => h.length = v.trim().parse().map_err(|_| bad("length"))?,
                "hash-lengths" => {
                    let parts: Vec<i32> = v
                        .split(',')
                        .map(|p| p.trim().parse())
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|_| bad("hash-lengths"))?;
                    if parts.len() != 3 {
                        return Err(bad("hash-lengths"));
                    }
                    h.hash_lengths = [parts[0], parts[1], parts[2]];
                }
                "url" => h.url = v.into(),
                "sha-1" => h.sha1 = v.into(),
                _ => {}
            }
        }
        let [seq, rb, sb] = h.hash_lengths;
        if !(1..=2).contains(&seq) || !(1..=4).contains(&rb) || !(3..=16).contains(&sb) {
            return Err(Error::Header(format!(
                "nonsensical hash lengths {seq},{rb},{sb}"
            )));
        }
        if h.block_size == 0 || !h.block_size.is_power_of_two() || h.block_size > (1 << 24) {
            return Err(Error::Header(format!("bad block size {}", h.block_size)));
        }
        let (rb, sb) = (rb as usize, sb as usize);
        let count = h.length.div_ceil(h.block_size as u64) as usize;
        let rec = rb + sb;
        if body.len() < count.checked_mul(rec).ok_or(Error::Truncated)? {
            return Err(Error::Truncated);
        }
        let mut blocks = Vec::with_capacity(count);
        for i in 0..count {
            let r = &body[i * rec..(i + 1) * rec];
            let mut w = [0u8; 4];
            w[4 - rb..].copy_from_slice(&r[..rb]);
            let mut strong = [0u8; 16];
            strong[..sb].copy_from_slice(&r[rb..]);
            blocks.push(BlockSum {
                weak: u32::from_be_bytes(w),
                strong,
            });
        }
        Ok(Self { header: h, blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockSummer, Rsum};

    #[test]
    fn hash_lengths_known_values() {
        // Empty file: ln(0) = -inf saturates like Java's (int) cast.
        assert_eq!(hash_lengths(0, 8192), [1, 2, 3]);
        // Full parity with Java is checked in tests/java_zsync.rs.
    }

    #[test]
    fn roundtrip() {
        let data: Vec<u8> = (0..30000u32).map(|i| (i % 253) as u8).collect();
        let mut s = BlockSummer::new(8192);
        s.update(&data);
        let sums = s.finish();
        let m = ZsyncMeta::new(
            "f.pbo",
            1_700_000_000_000,
            data.len() as u64,
            "http://x/f.pbo",
            "abc",
            8192,
            &sums,
        );
        let bytes = m.to_bytes();
        let p = ZsyncMeta::parse(&bytes).unwrap();
        assert_eq!(p, m);
        assert_eq!(
            p.blocks[0].weak,
            Rsum::of(&data[..8192]).key(p.rsum_bytes())
        );
    }
}
