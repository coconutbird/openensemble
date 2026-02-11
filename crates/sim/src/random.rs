//! Deterministic random number generator matching vanilla Halo Wars.
//!
//! This implements the KISS+SWB generator from the original source.
//! For network sync, every client must produce identical random sequences.

use std::num::Wrapping;

/// Deterministic RNG matching vanilla `Random` class.
///
/// Uses KISS (Keep It Simple Stupid) + SWB (Subtract With Borrow) generators.
/// Period is approximately 2^7700.
#[derive(Clone)]
pub struct Random {
    // MWC state
    z: Wrapping<u32>,
    w: Wrapping<u32>,
    // SHR3 state
    jsr: Wrapping<u32>,
    // CONG state
    jcong: Wrapping<u32>,
    // FIB state
    a: Wrapping<u32>,
    b: Wrapping<u32>,
    // SWB state
    table: [Wrapping<u32>; 256],
    x: Wrapping<u32>,
    y: Wrapping<u32>,
    bro: Wrapping<u32>,
    c: Wrapping<u8>,
    // Gaussian state
    prev_gaussian: f32,
    use_prev_gaussian: bool,
}

impl Default for Random {
    fn default() -> Self {
        let mut rng = Self {
            z: Wrapping(362436069),
            w: Wrapping(521288629),
            jsr: Wrapping(123456789),
            jcong: Wrapping(380116160),
            a: Wrapping(224466889),
            b: Wrapping(7584631),
            table: [Wrapping(0); 256],
            x: Wrapping(0),
            y: Wrapping(0),
            bro: Wrapping(0),
            c: Wrapping(0),
            prev_gaussian: 0.0,
            use_prev_gaussian: false,
        };
        rng.set_seed_full(12345, 65435, 34221, 12345, 9983651, 95746118);
        rng
    }
}

impl Random {
    pub const U_RAND_MAX: u32 = u32::MAX;

    /// Create a new RNG with the default seed.
    pub fn new() -> Self {
        Self::default()
    }

    /// MWC generator: z = 36969 * (z & 65535) + (z >> 16)
    #[inline]
    fn znew(&mut self) -> Wrapping<u32> {
        self.z = Wrapping(36969) * (self.z & Wrapping(65535)) + (self.z >> 16);
        self.z
    }

    /// MWC generator: w = 18000 * (w & 65535) + (w >> 16)
    #[inline]
    fn wnew(&mut self) -> Wrapping<u32> {
        self.w = Wrapping(18000) * (self.w & Wrapping(65535)) + (self.w >> 16);
        self.w
    }

    /// MWC combined: (znew << 16) + wnew
    #[inline]
    fn mwc(&mut self) -> Wrapping<u32> {
        (self.znew() << 16) + self.wnew()
    }

    /// SHR3: 3-shift register generator
    #[inline]
    fn shr3(&mut self) -> Wrapping<u32> {
        self.jsr ^= self.jsr << 17;
        self.jsr ^= self.jsr >> 13;
        self.jsr ^= self.jsr << 5;
        self.jsr
    }

    /// CONG: Congruential generator
    #[inline]
    fn cong(&mut self) -> Wrapping<u32> {
        self.jcong = Wrapping(69069) * self.jcong + Wrapping(1234567);
        self.jcong
    }

    /// FIB: Fibonacci generator
    #[inline]
    fn fib(&mut self) -> Wrapping<u32> {
        self.b = self.a + self.b;
        self.a = self.b - self.a;
        self.b
    }

    /// KISS: Combined generator (MWC ^ CONG) + SHR3
    #[inline]
    fn kiss(&mut self) -> Wrapping<u32> {
        (self.mwc() ^ self.cong()) + self.shr3()
    }

    /// SWB: Subtract with borrow generator
    #[inline]
    fn swb(&mut self) -> Wrapping<u32> {
        self.c += Wrapping(1);
        let idx = self.c.0 as usize;
        let idx34 = self.c.0.wrapping_add(34) as usize;
        let idx19 = self.c.0.wrapping_add(19) as usize;

        self.x = self.table[idx34];
        self.y = self.table[idx19] + self.bro;
        self.bro = if self.x < self.y {
            Wrapping(1)
        } else {
            Wrapping(0)
        };
        self.table[idx] = self.x - self.y;
        self.table[idx]
    }

    /// Generate a random u32 using KISS + SWB.
    pub fn u_rand(&mut self) -> u32 {
        (self.kiss() + self.swb()).0
    }

    /// Fast random using FIB ^ SHR3 + SWB (no multiplies).
    pub fn u_rand_fast(&mut self) -> u32 {
        ((self.fib() ^ self.shr3()) + self.swb()).0
    }

    /// Random f64 in [l, h).
    pub fn d_rand(&mut self, l: f64, h: f64) -> f64 {
        if (l - h).abs() < f64::EPSILON {
            return l;
        }
        loop {
            let r = self.u_rand();
            let d = l + (h - l) * (r as f64 / 4294967296.0);
            if d >= l && d < h {
                return d;
            }
        }
    }

    /// Random f32 in [l, h).
    pub fn f_rand(&mut self, l: f32, h: f32) -> f32 {
        if (l - h).abs() < f32::EPSILON {
            return l;
        }
        loop {
            let r = self.u_rand();
            let f = l + (h - l) * (r as f64 / 4294967296.0) as f32;
            if f >= l && f < h {
                return f;
            }
        }
    }

    /// Random i32 in [l, h).
    pub fn i_rand(&mut self, l: i32, h: i32) -> i32 {
        debug_assert!(l < h);
        loop {
            let r = l + ((h - l) as f64 * self.d_rand(0.0, 1.0)) as i32;
            if r >= l && r < h {
                return r;
            }
        }
    }

    /// Set seed with 6 values (full initialization).
    pub fn set_seed_full(&mut self, i1: u32, i2: u32, i3: u32, i4: u32, i5: u32, i6: u32) {
        self.z = Wrapping(i1);
        self.w = Wrapping(i2);
        self.jsr = Wrapping(if i3 == 0 { 1 } else { i3 });
        self.jcong = Wrapping(i4);
        self.a = Wrapping(i5);
        self.b = Wrapping(i6);
        self.x = Wrapping(0);
        self.y = Wrapping(0);
        self.bro = Wrapping(0);
        self.c = Wrapping(0);
        self.use_prev_gaussian = false;
        self.prev_gaussian = 0.0;

        // Initialize table with KISS values
        for i in 0..256 {
            self.table[i] = self.kiss();
        }
    }

    /// Set seed from a single u32.
    pub fn set_seed(&mut self, seed: u32) {
        self.jcong = Wrapping(seed);

        let mut s = [0u32; 6];
        for item in &mut s {
            let r0 = (self.cong().0 >> 24) & 0xFF;
            let r1 = (self.cong().0 >> 24) & 0xFF;
            let r2 = (self.cong().0 >> 24) & 0xFF;
            let r3 = (self.cong().0 >> 24) & 0xFF;
            *item = r0 | (r1 << 8) | (r2 << 16) | (r3 << 24);
        }
        self.set_seed_full(s[0], s[1], s[2], s[3], s[4], s[5]);
    }

    /// Set seed from a u64.
    pub fn set_seed64(&mut self, seed: u64) {
        self.jcong = Wrapping(seed as u32);
        self.jsr = Wrapping(((seed >> 32) as u32) ^ self.jcong.0);
        if self.jsr.0 == 0 {
            self.jsr = Wrapping(1);
        }

        let mut s = [0u32; 6];
        for item in &mut s {
            let r0 = (self.cong().0 >> 24) & 0xFF;
            let r1 = (self.cong().0 >> 24) & 0xFF;
            let r2 = (self.cong().0 >> 24) & 0xFF;
            let r3 = (self.cong().0 >> 24) & 0xFF;
            *item = r0 | (r1 << 8) | (r2 << 16) | (r3 << 24);
            *item ^= self.shr3().0;
        }
        self.set_seed_full(s[0], s[1], s[2], s[3], s[4], s[5]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_sequence() {
        let mut rng1 = Random::new();
        let mut rng2 = Random::new();

        // Same seed should produce same sequence
        for _ in 0..1000 {
            assert_eq!(rng1.u_rand(), rng2.u_rand());
        }
    }

    #[test]
    fn test_seed_changes_sequence() {
        let mut rng1 = Random::new();
        let mut rng2 = Random::new();
        rng2.set_seed(42);

        // Different seeds should produce different sequences
        let mut same = true;
        for _ in 0..100 {
            if rng1.u_rand() != rng2.u_rand() {
                same = false;
                break;
            }
        }
        assert!(!same);
    }

    #[test]
    fn test_i_rand_range() {
        let mut rng = Random::new();
        for _ in 0..1000 {
            let v = rng.i_rand(0, 10);
            assert!(v >= 0 && v < 10);
        }
    }

    #[test]
    fn test_f_rand_range() {
        let mut rng = Random::new();
        for _ in 0..1000 {
            let v = rng.f_rand(0.0, 1.0);
            assert!(v >= 0.0 && v < 1.0);
        }
    }
}
