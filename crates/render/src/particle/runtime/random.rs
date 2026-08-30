//! Deterministic Marsaglia generator used by the retail particle manager.

use num_traits::ToPrimitive;

const TABLE_SIZE: usize = 256;
const U32_RANGE: f64 = 4_294_967_296.0;

#[derive(Clone, Debug)]
pub(super) struct RetailRandom {
    z: u32,
    w: u32,
    jsr: u32,
    jcong: u32,
    a: u32,
    b: u32,
    table: [u32; TABLE_SIZE],
    x: u32,
    y: u32,
    borrow: u32,
    cursor: u8,
}

impl RetailRandom {
    pub(super) fn from_reference_seed() -> Self {
        let mut random = Self::empty();
        random.set_seed([12_345, 65_435, 34_221, 12_345, 9_983_651, 95_746_118]);
        random
    }

    pub(super) fn from_seed(seed: u32) -> Self {
        let mut random = Self::empty();
        random.jcong = seed;
        let seeds = std::array::from_fn(|_| {
            let bytes = std::array::from_fn(|_| (random.cong() >> 24) as u8);
            u32::from_le_bytes(bytes)
        });
        random.set_seed(seeds);
        random
    }

    pub(super) fn unit_f32(&mut self) -> f32 {
        (f64::from(self.next_u32()) / U32_RANGE)
            .to_f32()
            .unwrap_or(0.0)
    }

    pub(super) fn range_f32(&mut self, low: f32, high: f32) -> f32 {
        if !low.is_finite() || !high.is_finite() {
            return if low.is_finite() { low } else { 0.0 };
        }
        let low_magnitude = low.to_bits() & 0x7fff_ffff;
        let high_magnitude = high.to_bits() & 0x7fff_ffff;
        if low.to_bits() == high.to_bits() || (low_magnitude == 0 && high_magnitude == 0) {
            return low;
        }
        let (low, high) = if low.total_cmp(&high).is_lt() {
            (low, high)
        } else {
            (high, low)
        };
        loop {
            let span = f64::from(high) - f64::from(low);
            let value = (f64::from(low) + span * (f64::from(self.next_u32()) / U32_RANGE))
                .to_f32()
                .unwrap_or(low);
            if value >= low && value < high {
                return value;
            }
        }
    }

    pub(super) fn index(&mut self, upper_exclusive: usize) -> usize {
        debug_assert!(upper_exclusive > 0);
        let upper = u128::try_from(upper_exclusive).unwrap_or(u128::MAX);
        let value = (u128::from(self.next_u32()) * upper) >> 32;
        usize::try_from(value).unwrap_or(upper_exclusive - 1)
    }

    fn empty() -> Self {
        Self {
            z: 362_436_069,
            w: 521_288_629,
            jsr: 123_456_789,
            jcong: 380_116_160,
            a: 224_466_889,
            b: 7_584_631,
            table: [0; TABLE_SIZE],
            x: 0,
            y: 0,
            borrow: 0,
            cursor: 0,
        }
    }

    fn set_seed(&mut self, seeds: [u32; 6]) {
        self.z = seeds[0];
        self.w = seeds[1];
        self.jsr = seeds[2].max(1);
        self.jcong = seeds[3];
        self.a = seeds[4];
        self.b = seeds[5];
        self.x = 0;
        self.y = 0;
        self.borrow = 0;
        self.cursor = 0;
        for index in 0..TABLE_SIZE {
            self.table[index] = self.kiss();
        }
    }

    fn next_u32(&mut self) -> u32 {
        self.kiss().wrapping_add(self.subtract_with_borrow())
    }

    fn mwc(&mut self) -> u32 {
        self.z = 36_969_u32
            .wrapping_mul(self.z & 65_535)
            .wrapping_add(self.z >> 16);
        self.w = 18_000_u32
            .wrapping_mul(self.w & 65_535)
            .wrapping_add(self.w >> 16);
        (self.z << 16).wrapping_add(self.w)
    }

    fn shr3(&mut self) -> u32 {
        self.jsr ^= self.jsr << 17;
        self.jsr ^= self.jsr >> 13;
        self.jsr ^= self.jsr << 5;
        self.jsr
    }

    fn cong(&mut self) -> u32 {
        self.jcong = 69_069_u32.wrapping_mul(self.jcong).wrapping_add(1_234_567);
        self.jcong
    }

    fn kiss(&mut self) -> u32 {
        let mwc = self.mwc();
        let cong = self.cong();
        let shr3 = self.shr3();
        (mwc ^ cong).wrapping_add(shr3)
    }

    fn subtract_with_borrow(&mut self) -> u32 {
        self.cursor = self.cursor.wrapping_add(1);
        self.x = self.table[self.cursor.wrapping_add(34) as usize];
        self.y = self.table[self.cursor.wrapping_add(19) as usize].wrapping_add(self.borrow);
        self.borrow = u32::from(self.x < self.y);
        let value = self.x.wrapping_sub(self.y);
        self.table[self.cursor as usize] = value;
        value
    }

    #[cfg(test)]
    fn lagged_fibonacci(&mut self) -> u32 {
        self.cursor = self.cursor.wrapping_add(1);
        let index = self.cursor as usize;
        let value = self.table[index]
            .wrapping_add(self.table[self.cursor.wrapping_add(58) as usize])
            .wrapping_add(self.table[self.cursor.wrapping_add(119) as usize])
            .wrapping_add(self.table[self.cursor.wrapping_add(178) as usize]);
        self.table[index] = value;
        value
    }

    #[cfg(test)]
    fn fibonacci(&mut self) -> u32 {
        self.b = self.a.wrapping_add(self.b);
        self.a = self.b.wrapping_sub(self.a);
        self.a
    }
}

#[cfg(test)]
mod tests {
    use super::RetailRandom;

    #[test]
    fn marsaglia_reference_sequences_match_the_retail_source() {
        let mut random = RetailRandom::empty();
        random.set_seed([12_345, 65_435, 34_221, 12_345, 9_983_651, 95_746_118]);
        let million = 1_000_000;
        assert_eq!(last(million, || random.lagged_fibonacci()), 1_064_612_766);
        assert_eq!(last(million, || random.subtract_with_borrow()), 627_749_721);
        assert_eq!(last(million, || random.kiss()), 1_372_460_312);
        assert_eq!(last(million, || random.cong()), 1_529_210_297);
        assert_eq!(last(million, || random.shr3()), 2_642_725_982);
        assert_eq!(last(million, || random.mwc()), 904_977_562);
        assert_eq!(last(million, || random.fibonacci()), 3_519_793_928);
    }

    #[test]
    fn signed_zero_range_matches_the_retail_equality_check() {
        let mut random = RetailRandom::from_reference_seed();
        assert_eq!(random.range_f32(-0.0, 0.0).to_bits(), (-0.0_f32).to_bits());
    }

    fn last(count: usize, mut next: impl FnMut() -> u32) -> u32 {
        let mut value = 0;
        for _ in 0..count {
            value = next();
        }
        value
    }
}
