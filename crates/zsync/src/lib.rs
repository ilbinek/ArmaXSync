//! zsync metafiles compatible with Arma3Sync's embedded `jazsync`.
//!
//! * [`BlockSummer`] computes per-block rsum + MD4 in a single streaming pass.
//! * [`ZsyncMeta`] renders/parses the `.zsync` format (header + checksums).
//! * [`match_blocks`] scans a local seed file for blocks of the target.
//! * [`plan_ranges`] turns missing blocks into coalesced byte ranges.

mod matcher;
mod meta;
mod rsum;

pub use matcher::{match_blocks, match_blocks_in, BlockMap};
pub use meta::{format_mtime, hash_lengths, BlockSum, ZsyncHeader, ZsyncMeta};
pub use rsum::Rsum;

use md4::{Digest, Md4};

/// Block size used by Arma3Sync (fixed).
pub const LEGACY_BLOCK_SIZE: usize = 8192;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid zsync header: {0}")]
    Header(String),
    #[error("zsync checksum data truncated")]
    Truncated,
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Full-precision checksums of one block (zero padded to the block size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullBlockSum {
    pub rsum: Rsum,
    pub md4: [u8; 16],
}

/// Streaming computation of block checksums.
pub struct BlockSummer {
    block_size: usize,
    buf: Vec<u8>,
    sums: Vec<FullBlockSum>,
}

impl BlockSummer {
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            buf: Vec::with_capacity(block_size),
            sums: Vec::new(),
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            if self.buf.is_empty() && data.len() >= self.block_size {
                let (blk, rest) = data.split_at(self.block_size);
                self.sums.push(sum_block(blk));
                data = rest;
                continue;
            }
            let take = (self.block_size - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == self.block_size {
                self.sums.push(sum_block(&self.buf));
                self.buf.clear();
            }
        }
    }

    pub fn finish(mut self) -> Vec<FullBlockSum> {
        if !self.buf.is_empty() {
            self.buf.resize(self.block_size, 0);
            self.sums.push(sum_block(&self.buf));
        }
        self.sums
    }
}

/// Checksums of a block of exactly the block size.
pub fn sum_block(block: &[u8]) -> FullBlockSum {
    let md4: [u8; 16] = Md4::digest(block).into();
    FullBlockSum {
        rsum: Rsum::of(block),
        md4,
    }
}

/// A byte range `[start, end)` to fetch from the remote file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn len(&self) -> u64 {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }
}

/// Coalesce missing blocks into ranges. Gaps of up to `merge_gap_blocks`
/// matched blocks between missing runs are fetched too (fewer requests);
/// ranges are capped at `max_range_bytes` (0 = unlimited).
pub fn plan_ranges(
    map: &BlockMap,
    file_len: u64,
    merge_gap_blocks: usize,
    max_range_bytes: u64,
) -> Vec<ByteRange> {
    let bs = map.block_size as u64;
    let mut out: Vec<ByteRange> = Vec::new();
    for (i, m) in map.blocks.iter().enumerate() {
        if m.is_some() {
            continue;
        }
        let start = i as u64 * bs;
        let end = (start + bs).min(file_len);
        if start >= end {
            continue;
        }
        if let Some(last) = out.last_mut() {
            let gap_blocks = (start.saturating_sub(last.end)) / bs;
            let merged_len = end - last.start;
            if gap_blocks as usize <= merge_gap_blocks
                && (max_range_bytes == 0 || merged_len <= max_range_bytes)
            {
                last.end = end;
                continue;
            }
        }
        out.push(ByteRange { start, end });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summer_matches_direct() {
        let data: Vec<u8> = (0..20000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut s = BlockSummer::new(8192);
        for chunk in data.chunks(1000) {
            s.update(chunk);
        }
        let sums = s.finish();
        assert_eq!(sums.len(), 3);
        assert_eq!(sums[0], sum_block(&data[..8192]));
        let mut last = data[16384..].to_vec();
        last.resize(8192, 0);
        assert_eq!(sums[2], sum_block(&last));
    }

    #[test]
    fn ranges_coalesce() {
        let map = BlockMap {
            block_size: 10,
            blocks: vec![None, None, Some(0), None, Some(1), Some(2), Some(3), None],
        };
        let r = plan_ranges(&map, 75, 0, 0);
        assert_eq!(
            r,
            vec![
                ByteRange { start: 0, end: 20 },
                ByteRange { start: 30, end: 40 },
                ByteRange { start: 70, end: 75 }
            ]
        );
        let r = plan_ranges(&map, 75, 1, 0);
        assert_eq!(
            r,
            vec![
                ByteRange { start: 0, end: 40 },
                ByteRange { start: 70, end: 75 }
            ]
        );
        let r = plan_ranges(&map, 75, 1, 25);
        assert_eq!(
            r,
            vec![
                ByteRange { start: 0, end: 20 },
                ByteRange { start: 30, end: 40 },
                ByteRange { start: 70, end: 75 }
            ]
        );
    }
}
