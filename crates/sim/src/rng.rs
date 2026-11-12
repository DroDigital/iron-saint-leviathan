//! Seeded RNG used everywhere in the simulation (PCG32 via `rand_pcg`).

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SimRng(Pcg32);

impl SimRng {
    pub fn new(seed: u64) -> Self {
        SimRng(Pcg32::seed_from_u64(seed))
    }
    pub fn u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    pub fn u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        // Lemire-free modulo is fine at our scale; bias < 2^-24 for small n.
        self.u32() % n
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len() as u32) as usize])
        }
    }
    /// Weighted pick; returns index.
    pub fn weighted(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u32 = weights.iter().sum();
        if total == 0 {
            return None;
        }
        let mut r = self.below(total);
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return Some(i);
            }
            r -= w;
        }
        None
    }
    /// Derive an independent stream (for sub-systems) without disturbing determinism elsewhere.
    pub fn fork(&mut self, salt: u64) -> SimRng {
        SimRng::new(self.u64() ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }
}

/// Stable 64-bit hash (FNV-1a) for seeds derived from strings (daily tide, build hashes).
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_seed_same_stream() {
        let mut a = SimRng::new(42);
        let mut b = SimRng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.u32(), b.u32());
        }
        let mut c = SimRng::new(43);
        assert_ne!(SimRng::new(42).u32(), c.u32());
    }
    #[test]
    fn f32_in_range_and_weighted() {
        let mut r = SimRng::new(7);
        for _ in 0..10_000 {
            let x = r.f32();
            assert!((0.0..1.0).contains(&x));
        }
        let mut hits = [0u32; 3];
        for _ in 0..30_000 {
            hits[r.weighted(&[1, 0, 2]).unwrap()] += 1;
        }
        assert_eq!(hits[1], 0);
        assert!(hits[2] > hits[0] * 3 / 2);
    }
    #[test]
    fn serde_roundtrip_preserves_stream() {
        let mut a = SimRng::new(9);
        a.u32();
        let s = ron::to_string(&a).unwrap();
        let mut b: SimRng = ron::from_str(&s).unwrap();
        assert_eq!(a.u32(), b.u32());
    }
}
