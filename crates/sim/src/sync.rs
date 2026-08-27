//! Sync checksum for detecting out-of-sync (OOS) conditions.
//!
//! In deterministic lockstep, all clients must produce identical simulation
//! results. This module provides checksumming to detect when clients diverge.

use std::num::Wrapping;

/// CRC-32 polynomial (IEEE 802.3).
const CRC32_POLYNOMIAL: u32 = 0xEDB8_8320;

fn update_crc32(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte);
    for _ in 0..8 {
        crc = if crc & 1 == 0 {
            crc >> 1
        } else {
            (crc >> 1) ^ CRC32_POLYNOMIAL
        };
    }
    crc
}

/// Sync checksum state for detecting OOS.
#[derive(Debug, Clone, Default)]
pub struct SyncChecksum {
    /// Current CRC value.
    crc: u32,
    /// Number of values hashed.
    count: u64,
    /// Last sync update number.
    last_update: u32,
}

impl SyncChecksum {
    /// Create a new sync checksum.
    #[must_use]
    pub fn new() -> Self {
        Self {
            crc: 0xFFFF_FFFF,
            count: 0,
            last_update: 0,
        }
    }

    /// Reset the checksum state.
    pub fn reset(&mut self) {
        self.crc = 0xFFFF_FFFF;
        self.count = 0;
    }

    /// Get the current checksum value.
    #[must_use]
    pub fn value(&self) -> u32 {
        self.crc ^ 0xFFFF_FFFF
    }

    /// Get the number of values hashed.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Hash a u32 value.
    pub fn hash_u32(&mut self, value: u32) {
        let bytes = value.to_le_bytes();
        for byte in bytes {
            self.crc = update_crc32(self.crc, byte);
        }
        self.count += 1;
    }

    /// Hash an i32 value.
    pub fn hash_i32(&mut self, value: i32) {
        self.hash_u32(value.cast_unsigned());
    }

    /// Hash a f32 value.
    pub fn hash_f32(&mut self, value: f32) {
        self.hash_u32(value.to_bits());
    }

    /// Hash a slice of bytes.
    pub fn hash_bytes(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc = update_crc32(self.crc, byte);
        }
        self.count += 1;
    }

    /// Hash a Vec3 (x, y, z).
    pub fn hash_vec3(&mut self, x: f32, y: f32, z: f32) {
        self.hash_f32(x);
        self.hash_f32(y);
        self.hash_f32(z);
    }

    /// Set the last update number.
    pub fn set_update(&mut self, update: u32) {
        self.last_update = update;
    }

    /// Get the last update number.
    #[must_use]
    pub fn last_update(&self) -> u32 {
        self.last_update
    }
}

/// Simple additive checksum (faster, less collision-resistant).
#[derive(Debug, Clone, Default)]
pub struct SimpleChecksum {
    sum: Wrapping<u32>,
    count: u64,
}

impl SimpleChecksum {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.sum = Wrapping(0);
        self.count = 0;
    }

    #[must_use]
    pub fn value(&self) -> u32 {
        self.sum.0
    }

    pub fn add_u32(&mut self, value: u32) {
        self.sum += Wrapping(value);
        self.count += 1;
    }

    pub fn add_f32(&mut self, value: f32) {
        self.add_u32(value.to_bits());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_deterministic() {
        let mut c1 = SyncChecksum::new();
        let mut c2 = SyncChecksum::new();

        for i in 0..100 {
            c1.hash_u32(i);
            c2.hash_u32(i);
        }

        assert_eq!(c1.value(), c2.value());
    }

    #[test]
    fn test_crc32_different_input() {
        let mut c1 = SyncChecksum::new();
        let mut c2 = SyncChecksum::new();

        c1.hash_u32(1);
        c2.hash_u32(2);

        assert_ne!(c1.value(), c2.value());
    }
}
