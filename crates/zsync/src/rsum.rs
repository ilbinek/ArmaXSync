//! zsync rolling checksum (Adler-like, two 16-bit halves).

/// `a = Σ x_i`, `b = Σ (n - i) · x_i`, both modulo 2^16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rsum {
    pub a: u16,
    pub b: u16,
}

impl Rsum {
    pub fn of(block: &[u8]) -> Self {
        let n = block.len();
        let mut a: u16 = 0;
        let mut b: u16 = 0;
        for (i, &x) in block.iter().enumerate() {
            a = a.wrapping_add(x as u16);
            b = b.wrapping_add(((n - i) as u16).wrapping_mul(x as u16));
        }
        Self { a, b }
    }

    /// Slide the window by one byte (`old` leaves, `new` enters).
    #[inline]
    pub fn roll(&mut self, old: u8, new: u8, block_len: usize) {
        self.a = self.a.wrapping_sub(old as u16).wrapping_add(new as u16);
        self.b = self
            .b
            .wrapping_sub((block_len as u16).wrapping_mul(old as u16))
            .wrapping_add(self.a);
    }

    /// Big-endian `a_hi a_lo b_hi b_lo`, the zsync on-disk order.
    pub fn to_be_bytes(self) -> [u8; 4] {
        let [ah, al] = self.a.to_be_bytes();
        let [bh, bl] = self.b.to_be_bytes();
        [ah, al, bh, bl]
    }

    /// Key comparable with the truncated value stored in a metafile
    /// (the last `bytes` bytes of [`Rsum::to_be_bytes`]).
    #[inline]
    pub fn key(self, bytes: usize) -> u32 {
        let full = ((self.a as u32) << 16) | self.b as u32;
        if bytes >= 4 {
            full
        } else {
            full & ((1u32 << (bytes * 8)) - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_equals_direct() {
        let data: Vec<u8> = (0..5000u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let n = 1024;
        let mut r = Rsum::of(&data[..n]);
        for p in 1..(data.len() - n) {
            r.roll(data[p - 1], data[p + n - 1], n);
            assert_eq!(r, Rsum::of(&data[p..p + n]), "pos {p}");
        }
    }
}
