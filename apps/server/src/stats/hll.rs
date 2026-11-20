//! HyperLogLog cardinality sketch (p = 12: 4096 one-byte registers, ~1.6 %
//! standard error). Used for unique-player counts: only the registers are
//! stored, never the hashed addresses, and sketches merge across days and
//! repositories.

pub const P: u32 = 12;
pub const M: usize = 1 << P;

#[derive(Clone, PartialEq, Eq)]
pub struct Hll {
    regs: Box<[u8; M]>,
}

impl Default for Hll {
    fn default() -> Self {
        Self {
            regs: Box::new([0u8; M]),
        }
    }
}

impl std::fmt::Debug for Hll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hll(~{})", self.count())
    }
}

impl Hll {
    /// Add a 64-bit hash (must already be uniformly distributed).
    pub fn insert(&mut self, hash: u64) {
        let idx = (hash >> (64 - P)) as usize;
        let rest = hash << P;
        // Position of the first set bit in the remaining 52 bits (1-based).
        let rho = (rest.leading_zeros() + 1).min(64 - P + 1) as u8;
        if rho > self.regs[idx] {
            self.regs[idx] = rho;
        }
    }

    pub fn merge(&mut self, other: &Hll) {
        for (a, b) in self.regs.iter_mut().zip(other.regs.iter()) {
            if *b > *a {
                *a = *b;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.regs.iter().all(|r| *r == 0)
    }

    /// Estimated number of distinct items.
    pub fn count(&self) -> u64 {
        let m = M as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let mut sum = 0.0f64;
        let mut zeros = 0usize;
        for &r in self.regs.iter() {
            sum += 1.0 / (1u64 << r) as f64;
            if r == 0 {
                zeros += 1;
            }
        }
        let est = alpha * m * m / sum;
        // Small-range correction (linear counting).
        if est <= 2.5 * m && zeros > 0 {
            return (m * (m / zeros as f64).ln()).round() as u64;
        }
        est.round() as u64
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != M {
            return None;
        }
        let mut h = Hll::default();
        h.regs.copy_from_slice(b);
        Some(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(i: u64) -> u64 {
        let b = blake3::hash(&i.to_le_bytes());
        u64::from_le_bytes(b.as_bytes()[..8].try_into().unwrap())
    }

    #[test]
    fn small_counts_are_exact_enough() {
        let mut s = Hll::default();
        assert_eq!(s.count(), 0);
        for i in 0..10 {
            s.insert(h(i));
            s.insert(h(i)); // duplicates don't count
        }
        assert_eq!(s.count(), 10);
    }

    #[test]
    fn accuracy_and_merge() {
        let (mut a, mut b) = (Hll::default(), Hll::default());
        for i in 0..10_000 {
            a.insert(h(i));
        }
        for i in 5_000..15_000 {
            b.insert(h(i));
        }
        let err = |est: u64, real: f64| ((est as f64 - real) / real).abs();
        assert!(err(a.count(), 10_000.0) < 0.03, "{}", a.count());
        a.merge(&b);
        assert!(err(a.count(), 15_000.0) < 0.03, "{}", a.count());
        let round = Hll::from_bytes(&a.to_bytes()).unwrap();
        assert_eq!(round, a);
        assert!(Hll::from_bytes(&[1, 2, 3]).is_none());
    }
}
