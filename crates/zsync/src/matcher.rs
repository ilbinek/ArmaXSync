//! Find blocks of a target file (described by a metafile) inside a local seed.

use std::collections::HashMap;
use std::path::Path;

use md4::{Digest, Md4};

use crate::{Rsum, ZsyncMeta};

/// For each target block, the seed offset where identical content was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockMap {
    pub block_size: usize,
    pub blocks: Vec<Option<u64>>,
}

impl BlockMap {
    pub fn matched(&self) -> usize {
        self.blocks.iter().filter(|b| b.is_some()).count()
    }
    /// Fraction of blocks available locally (1.0 for empty files).
    pub fn ratio(&self) -> f64 {
        if self.blocks.is_empty() {
            1.0
        } else {
            self.matched() as f64 / self.blocks.len() as f64
        }
    }
}

/// Scan a seed file on disk.
pub fn match_blocks(meta: &ZsyncMeta, seed: &Path) -> std::io::Result<BlockMap> {
    let f = std::fs::File::open(seed)?;
    let len = f.metadata()?.len();
    if len == 0 {
        return Ok(match_blocks_in(meta, &[], || false));
    }
    // SAFETY: the map is read-only and dropped before returning. Concurrent
    // modification of the seed can only produce wrong matches, which the
    // caller detects through the whole-file hash check.
    let map = unsafe { memmap2::Mmap::map(&f)? };
    Ok(match_blocks_in(meta, &map, || false))
}

/// Scan an in-memory seed. `cancelled` is polled periodically.
pub fn match_blocks_in(meta: &ZsyncMeta, seed: &[u8], cancelled: impl Fn() -> bool) -> BlockMap {
    let bs = meta.header.block_size;
    let n = meta.blocks.len();
    let mut result = BlockMap {
        block_size: bs,
        blocks: vec![None; n],
    };
    if n == 0 || seed.is_empty() {
        return result;
    }
    let rb = meta.rsum_bytes();
    let sb = meta.strong_bytes();
    let seq = meta.seq_matches();

    let mut table: HashMap<u32, Vec<u32>> = HashMap::with_capacity(n);
    let mut filter = vec![0u64; 1 << 10]; // 65536-bit prefilter on low 16 bits
    for (i, b) in meta.blocks.iter().enumerate() {
        table.entry(b.weak).or_default().push(i as u32);
        let k = (b.weak & 0xFFFF) as usize;
        filter[k >> 6] |= 1 << (k & 63);
    }

    // Seed is virtually zero-padded by one block so a trailing partial block
    // (zero padded in the metafile) can match.
    let padded_len = seed.len() + bs;
    let byte_at = |p: usize| -> u8 {
        if p < seed.len() {
            seed[p]
        } else {
            0
        }
    };
    let window = |p: usize, buf: &mut Vec<u8>| -> () {
        buf.clear();
        if p + bs <= seed.len() {
            buf.extend_from_slice(&seed[p..p + bs]);
        } else {
            let end = seed.len().min(p + bs);
            if p < end {
                buf.extend_from_slice(&seed[p..end]);
            }
            buf.resize(bs, 0);
        }
    };
    let strong_eq = |p: usize, block: usize, buf: &mut Vec<u8>| -> bool {
        window(p, buf);
        let d = Md4::digest(&buf[..]);
        d[..sb] == meta.blocks[block].strong[..sb]
    };
    let weak_eq = |p: usize, block: usize, buf: &mut Vec<u8>| -> bool {
        window(p, buf);
        Rsum::of(buf).key(rb) == meta.blocks[block].weak
    };

    let mut buf = Vec::with_capacity(bs);
    let mut tmp = Vec::with_capacity(bs);
    let mut pos = 0usize;
    window(0, &mut buf);
    let mut r = Rsum::of(&buf);
    let mut remaining = n;
    let mut steps = 0u64;

    while pos < seed.len() && remaining > 0 {
        steps += 1;
        if steps & 0xF_FFFF == 0 && cancelled() {
            break;
        }
        let key = r.key(rb);
        let mut advanced = false;
        let lo = (key & 0xFFFF) as usize;
        if filter[lo >> 6] & (1 << (lo & 63)) != 0 {
            if let Some(cands) = table.get(&key) {
                let mut strong_ok: Option<bool> = None;
                let mut matched_block = None;
                for &c in cands {
                    let c = c as usize;
                    if result.blocks[c].is_some() {
                        continue;
                    }
                    // Strong hash of this window is the same for all candidates.
                    let ok = *strong_ok.get_or_insert_with(|| strong_eq(pos, c, &mut tmp));
                    let ok = ok && {
                        // Equal weak keys with different strong prefixes: recheck per candidate.
                        window(pos, &mut tmp);
                        Md4::digest(&tmp[..])[..sb] == meta.blocks[c].strong[..sb]
                    };
                    if !ok {
                        continue;
                    }
                    // Require the following block to match as well when the
                    // metafile asks for sequential matches (unless last block).
                    if seq >= 2 && c + 1 < n {
                        let np = pos + bs;
                        if !(np < padded_len
                            && weak_eq(np, c + 1, &mut tmp)
                            && strong_eq(np, c + 1, &mut tmp))
                        {
                            continue;
                        }
                        if result.blocks[c + 1].is_none() {
                            result.blocks[c + 1] = Some(np as u64);
                            remaining -= 1;
                        }
                    }
                    result.blocks[c] = Some(pos as u64);
                    remaining -= 1;
                    matched_block = Some(c);
                    break;
                }
                // The same content may appear in several target blocks.
                if let Some(c) = matched_block {
                    for &o in cands {
                        let o = o as usize;
                        if o != c
                            && result.blocks[o].is_none()
                            && meta.blocks[o].strong[..sb] == meta.blocks[c].strong[..sb]
                        {
                            result.blocks[o] = Some(pos as u64);
                            remaining -= 1;
                        }
                    }
                    // Jump a whole block ahead.
                    pos += bs;
                    if pos < seed.len() {
                        window(pos, &mut buf);
                        r = Rsum::of(&buf);
                    }
                    advanced = true;
                }
            }
        }
        if !advanced {
            let old = byte_at(pos);
            let new = byte_at(pos + bs);
            r.roll(old, new, bs);
            pos += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plan_ranges, BlockSummer};

    fn meta_for(data: &[u8], bs: usize) -> ZsyncMeta {
        let mut s = BlockSummer::new(bs);
        s.update(data);
        ZsyncMeta::new("t", 0, data.len() as u64, "", "", bs, &s.finish())
    }

    fn pseudo(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    fn reconstruct(meta: &ZsyncMeta, map: &BlockMap, seed: &[u8], target: &[u8]) -> Vec<u8> {
        let bs = map.block_size;
        let len = meta.header.length as usize;
        let mut out = vec![0u8; len];
        for (i, m) in map.blocks.iter().enumerate() {
            let start = i * bs;
            let end = (start + bs).min(len);
            match m {
                Some(off) => {
                    let off = *off as usize;
                    for k in 0..(end - start) {
                        out[start + k] = if off + k < seed.len() {
                            seed[off + k]
                        } else {
                            0
                        };
                    }
                }
                None => out[start..end].copy_from_slice(&target[start..end]),
            }
        }
        out
    }

    #[test]
    fn identical_file_fully_matches() {
        let data = pseudo(100_000, 1);
        let meta = meta_for(&data, 4096);
        let map = match_blocks_in(&meta, &data, || false);
        assert_eq!(map.matched(), map.blocks.len());
        assert_eq!(reconstruct(&meta, &map, &data, &data), data);
    }

    #[test]
    fn insertion_and_modification() {
        let old = pseudo(200_000, 7);
        let mut new = old.clone();
        new.splice(50_000..50_000, pseudo(123, 9)); // insertion shifts everything
        new[150_000] ^= 0xFF; // modification
        new.truncate(190_000);
        let bs = 2048;
        let meta = meta_for(&new, bs);
        let map = match_blocks_in(&meta, &old, || false);
        let missing = map.blocks.len() - map.matched();
        assert!(missing <= 4, "missing {missing} blocks");
        assert_eq!(reconstruct(&meta, &map, &old, &new), new);
        let ranges = plan_ranges(&map, new.len() as u64, 0, 0);
        let fetched: u64 = ranges.iter().map(|r| r.len()).sum();
        assert!(fetched <= 4 * bs as u64);
    }

    #[test]
    fn trailing_partial_block_matches() {
        let data = pseudo(10_000, 3);
        let meta = meta_for(&data, 4096);
        let map = match_blocks_in(&meta, &data, || false);
        assert_eq!(map.blocks.len(), 3);
        assert!(map.blocks.iter().all(Option::is_some));
    }

    #[test]
    fn unrelated_seed_matches_nothing() {
        let meta = meta_for(&pseudo(50_000, 11), 2048);
        let map = match_blocks_in(&meta, &pseudo(50_000, 12), || false);
        assert_eq!(map.matched(), 0);
    }
}
