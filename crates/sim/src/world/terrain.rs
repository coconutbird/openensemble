//! Retail XSD simulation-terrain loading and height queries.

use super::World;
use crate::sync::SyncChecksum;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use glam::Vec3;
use half::f16;
use num_traits::ToPrimitive;

const XSD_VERSION: i32 = 4;
const XSD_HEADER_CHUNK: u64 = 0x1111;
const XSD_HEIGHTS_CHUNK: u64 = 0x2222;
const XSD_HEADER_SIZE: usize = 32;
const HEIGHT_BLOCK_AXIS: usize = 8;
const HEIGHT_BLOCK_SIZE: usize = HEIGHT_BLOCK_AXIS * HEIGHT_BLOCK_AXIS;

/// Failure while parsing the scenario's retail XSD simulation terrain.
#[derive(Debug, thiserror::Error)]
pub enum TerrainLoadError {
    /// The outer Ensemble Common Format container was invalid.
    #[error("invalid XSD container: {0}")]
    Container(#[from] ecf::Error),
    /// A required retail XSD chunk was absent.
    #[error("XSD is missing required chunk 0x{0:04X}")]
    MissingChunk(u64),
    /// The XSD header was shorter than the retail version-4 layout.
    #[error("XSD header is truncated: expected at least {expected} bytes, found {actual}")]
    TruncatedHeader { expected: usize, actual: usize },
    /// The XSD header used an unsupported version or byte order.
    #[error("unsupported XSD version bytes {0:02X?}")]
    UnsupportedVersion([u8; 4]),
    /// Terrain grid dimensions or scale were not usable.
    #[error("invalid XSD terrain dimensions: {0}")]
    InvalidDimensions(&'static str),
    /// The height chunk did not contain the cache-aligned retail grid.
    #[error("XSD height grid is truncated: expected {expected} bytes, found {actual}")]
    TruncatedHeights { expected: usize, actual: usize },
}

#[derive(Debug, Clone, Copy)]
enum XsdEndian {
    Big,
    Little,
}

impl XsdEndian {
    fn from_version(bytes: &[u8]) -> Result<Self, TerrainLoadError> {
        let version_bytes: [u8; 4] = bytes
            .get(..4)
            .ok_or(TerrainLoadError::TruncatedHeader {
                expected: XSD_HEADER_SIZE,
                actual: bytes.len(),
            })?
            .try_into()
            .expect("the checked XSD version slice has four bytes");
        if BigEndian::read_i32(&version_bytes) == XSD_VERSION {
            Ok(Self::Big)
        } else if LittleEndian::read_i32(&version_bytes) == XSD_VERSION {
            Ok(Self::Little)
        } else {
            Err(TerrainLoadError::UnsupportedVersion(version_bytes))
        }
    }

    fn read_i32(self, bytes: &[u8]) -> i32 {
        match self {
            Self::Big => BigEndian::read_i32(bytes),
            Self::Little => LittleEndian::read_i32(bytes),
        }
    }

    fn read_f32(self, bytes: &[u8]) -> f32 {
        match self {
            Self::Big => BigEndian::read_f32(bytes),
            Self::Little => LittleEndian::read_f32(bytes),
        }
    }

    fn read_u16(self, bytes: &[u8]) -> u16 {
        match self {
            Self::Big => BigEndian::read_u16(bytes),
            Self::Little => LittleEndian::read_u16(bytes),
        }
    }
}

/// Immutable simulation height grid loaded from a scenario XSD.
#[derive(Debug)]
pub(super) struct TerrainSimulation {
    height_axis: usize,
    cache_axis: usize,
    height_tile_scale: f32,
    heights: Vec<f32>,
    fingerprint: u32,
}

impl TerrainSimulation {
    fn from_xsd(bytes: &[u8]) -> Result<Self, TerrainLoadError> {
        let container = ecf::Reader::new(bytes)?;
        let header = container
            .chunk_data_by_id(XSD_HEADER_CHUNK)
            .map_err(|error| missing_chunk(error, XSD_HEADER_CHUNK))?;
        let heights = container
            .chunk_data_by_id(XSD_HEIGHTS_CHUNK)
            .map_err(|error| missing_chunk(error, XSD_HEIGHTS_CHUNK))?;
        Self::from_chunks(&header, &heights)
    }

    fn from_chunks(header: &[u8], height_bytes: &[u8]) -> Result<Self, TerrainLoadError> {
        if header.len() < XSD_HEADER_SIZE {
            return Err(TerrainLoadError::TruncatedHeader {
                expected: XSD_HEADER_SIZE,
                actual: header.len(),
            });
        }
        let endian = XsdEndian::from_version(header)?;
        let height_axis = positive_usize(endian.read_i32(&header[16..20]), "height axis")?;
        let cache_axis = positive_usize(endian.read_i32(&header[20..24]), "cache axis")?;
        let height_tile_scale = endian.read_f32(&header[24..28]);
        if cache_axis < height_axis || !cache_axis.is_multiple_of(HEIGHT_BLOCK_AXIS) {
            return Err(TerrainLoadError::InvalidDimensions(
                "cache axis must contain the height grid and be divisible by eight",
            ));
        }
        if !height_tile_scale.is_finite() || height_tile_scale <= 0.0 {
            return Err(TerrainLoadError::InvalidDimensions(
                "height tile scale must be finite and positive",
            ));
        }
        let sample_count =
            cache_axis
                .checked_mul(cache_axis)
                .ok_or(TerrainLoadError::InvalidDimensions(
                    "cache-aligned height grid overflows memory",
                ))?;
        let expected_bytes = sample_count.checked_mul(size_of::<u16>()).ok_or(
            TerrainLoadError::InvalidDimensions("height byte count overflows memory"),
        )?;
        if height_bytes.len() < expected_bytes {
            return Err(TerrainLoadError::TruncatedHeights {
                expected: expected_bytes,
                actual: height_bytes.len(),
            });
        }
        let heights: Vec<f32> = height_bytes[..expected_bytes]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|sample| f16::from_bits(endian.read_u16(sample)).to_f32())
            .collect();
        if heights.iter().any(|height| !height.is_finite()) {
            return Err(TerrainLoadError::InvalidDimensions(
                "height samples must be finite",
            ));
        }
        let fingerprint = terrain_fingerprint(
            height_axis,
            cache_axis,
            height_tile_scale,
            heights.as_slice(),
        );
        Ok(Self {
            height_axis,
            cache_axis,
            height_tile_scale,
            heights,
            fingerprint,
        })
    }

    fn height(&self, position: Vec3, clamp: bool) -> Option<f32> {
        if !position.is_finite() {
            return None;
        }
        let x = world_to_grid(position.x, self.height_tile_scale);
        let z = world_to_grid(position.z, self.height_tile_scale);
        let x = checked_grid_coordinate(x, self.height_axis, clamp)?;
        let z = checked_grid_coordinate(z, self.height_axis, clamp)?;
        let blocks_per_axis = self.cache_axis / HEIGHT_BLOCK_AXIS;
        let block_x = (x / HEIGHT_BLOCK_AXIS) * HEIGHT_BLOCK_SIZE;
        let block_z = (z / HEIGHT_BLOCK_AXIS) * blocks_per_axis * HEIGHT_BLOCK_SIZE;
        let local_x = x % HEIGHT_BLOCK_AXIS;
        let local_z = (z % HEIGHT_BLOCK_AXIS) * HEIGHT_BLOCK_AXIS;
        self.heights
            .get(block_z + block_x + local_z + local_x)
            .copied()
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.fingerprint);
        checksum.hash_u32(u32::try_from(self.height_axis).unwrap_or(u32::MAX));
        checksum.hash_f32(self.height_tile_scale);
    }
}

fn missing_chunk(error: ecf::Error, chunk_id: u64) -> TerrainLoadError {
    if matches!(error, ecf::Error::ChunkNotFound(_)) {
        TerrainLoadError::MissingChunk(chunk_id)
    } else {
        TerrainLoadError::Container(error)
    }
}

fn positive_usize(value: i32, field: &'static str) -> Result<usize, TerrainLoadError> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(TerrainLoadError::InvalidDimensions(field))
}

fn world_to_grid(world: f32, tile_scale: f32) -> i32 {
    let coordinate = world / tile_scale;
    coordinate.to_i32().unwrap_or_else(|| {
        if coordinate.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

fn checked_grid_coordinate(value: i32, axis: usize, clamp: bool) -> Option<usize> {
    let maximum = i32::try_from(axis.checked_sub(1)?).ok()?;
    if clamp {
        usize::try_from(value.clamp(0, maximum)).ok()
    } else {
        (0..=maximum)
            .contains(&value)
            .then(|| usize::try_from(value).ok())
            .flatten()
    }
}

fn terrain_fingerprint(
    height_axis: usize,
    cache_axis: usize,
    height_tile_scale: f32,
    heights: &[f32],
) -> u32 {
    let mut checksum = SyncChecksum::new();
    checksum.hash_u32(u32::try_from(height_axis).unwrap_or(u32::MAX));
    checksum.hash_u32(u32::try_from(cache_axis).unwrap_or(u32::MAX));
    checksum.hash_f32(height_tile_scale);
    for &height in heights {
        checksum.hash_f32(height);
    }
    checksum.value()
}

impl World {
    /// Load the retail scenario XSD used by synchronized terrain queries.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainLoadError`] when the ECF container, version-4 header,
    /// or cache-blocked height grid is malformed.
    pub fn configure_terrain_simulation(&mut self, bytes: &[u8]) -> Result<(), TerrainLoadError> {
        self.terrain_simulation = Some(TerrainSimulation::from_xsd(bytes)?);
        Ok(())
    }

    /// Return whether retail XSD simulation terrain is available.
    #[must_use]
    pub const fn has_terrain_simulation(&self) -> bool {
        self.terrain_simulation.is_some()
    }

    /// Sample the retail simulation height grid at a world-space point.
    #[must_use]
    pub fn terrain_height(&self, position: Vec3, clamp: bool) -> Option<f32> {
        self.terrain_simulation
            .as_ref()
            .and_then(|terrain| terrain.height(position, clamp))
    }
}

#[cfg(test)]
mod tests;
