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
        let x = checked_grid_coordinate(
            world_to_grid(position.x, self.height_tile_scale),
            self.height_axis,
            clamp,
        )?;
        let z = checked_grid_coordinate(
            world_to_grid(position.z, self.height_tile_scale),
            self.height_axis,
            clamp,
        )?;
        let x = i32::try_from(x).ok()?;
        let z = i32::try_from(z).ok()?;
        let world_x = x.to_f32()? * self.height_tile_scale;
        let world_z = z.to_f32()? * self.height_tile_scale;
        let x_fraction = (position.x - world_x) / self.height_tile_scale;
        let z_fraction = (position.z - world_z) / self.height_tile_scale;
        let bottom = interpolate(
            self.height_sample(x, z, true)?,
            self.height_sample(x + 1, z, true)?,
            x_fraction,
        );
        let top = interpolate(
            self.height_sample(x, z + 1, true)?,
            self.height_sample(x + 1, z + 1, true)?,
            x_fraction,
        );
        Some(interpolate(bottom, top, z_fraction))
    }

    fn height_sample(&self, x: i32, z: i32, clamp: bool) -> Option<f32> {
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

    fn projectile_segment_intersection(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        const EARLY_OUT_DISTANCE_SQUARED: f32 = 8.0 * 8.0;
        const ABOVE_TERRAIN_HEIGHT: f32 = 0.25;

        if !start.is_finite() || !end.is_finite() {
            return None;
        }
        if start.distance_squared(end) < EARLY_OUT_DISTANCE_SQUARED {
            let start_height = self.height(start, true)?;
            let end_height = self.height(end, true)?;
            let start_above = start.y - start_height > ABOVE_TERRAIN_HEIGHT;
            let end_above = end.y - end_height > ABOVE_TERRAIN_HEIGHT;
            if start_above && end_above {
                return None;
            }
            if !start_above && !end_above {
                return Some(Vec3::new(end.x, end_height, end.z));
            }
        }
        self.segment_intersection(start, end)
    }

    fn segment_intersection(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        let direction = end - start;
        if !start.is_finite()
            || !end.is_finite()
            || direction.length_squared() <= f32::EPSILON
            || self.height_axis < 2
        {
            return None;
        }
        let maximum_world = (self.height_axis - 1).to_f32()? * self.height_tile_scale;
        let (entry, exit) = clip_segment_xz(start, direction, maximum_world)?;
        let clipped_start = start + direction * entry;
        if clipped_start.y < self.height(clipped_start, false)? {
            return None;
        }
        let clipped_end = start + direction * exit;
        let maximum_cell = i32::try_from(self.height_axis - 2).ok()?;
        let (minimum_x, maximum_x) = cell_span(
            clipped_start.x,
            clipped_end.x,
            self.height_tile_scale,
            maximum_cell,
        )?;
        let (minimum_z, maximum_z) = cell_span(
            clipped_start.z,
            clipped_end.z,
            self.height_tile_scale,
            maximum_cell,
        )?;
        let mut nearest = None;
        for z in minimum_z..=maximum_z {
            for x in minimum_x..=maximum_x {
                nearest = nearest_triangle_hit(
                    nearest,
                    self.tile_triangles(x, z)?,
                    start,
                    direction,
                    entry,
                    exit,
                );
            }
        }
        nearest.map(|fraction| start + direction * fraction)
    }

    fn tile_triangles(&self, x: i32, z: i32) -> Option<[[Vec3; 3]; 2]> {
        let x1 = x.to_f32()? * self.height_tile_scale;
        let z1 = z.to_f32()? * self.height_tile_scale;
        let x2 = x1 + self.height_tile_scale;
        let z2 = z1 + self.height_tile_scale;
        let y0 = self.height_sample(x, z, true)?;
        let y1 = self.height_sample(x, z + 1, true)?;
        let y2 = self.height_sample(x + 1, z + 1, true)?;
        let y3 = self.height_sample(x + 1, z, true)?;
        let lower_left = Vec3::new(x1, y0, z1);
        let upper_right = Vec3::new(x2, y2, z2);
        Some([
            [lower_left, Vec3::new(x1, y1, z2), upper_right],
            [lower_left, upper_right, Vec3::new(x2, y3, z1)],
        ])
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.fingerprint);
        checksum.hash_u32(u32::try_from(self.height_axis).unwrap_or(u32::MAX));
        checksum.hash_f32(self.height_tile_scale);
    }
}

fn clip_segment_xz(start: Vec3, direction: Vec3, maximum: f32) -> Option<(f32, f32)> {
    let mut entry = 0.0_f32;
    let mut exit = 1.0_f32;
    for (origin, delta) in [(start.x, direction.x), (start.z, direction.z)] {
        if delta.abs() <= f32::EPSILON {
            if origin < 0.0 || origin > maximum {
                return None;
            }
            continue;
        }
        let first = -origin / delta;
        let second = (maximum - origin) / delta;
        entry = entry.max(first.min(second));
        exit = exit.min(first.max(second));
        if entry > exit {
            return None;
        }
    }
    Some((entry.clamp(0.0, 1.0), exit.clamp(0.0, 1.0)))
}

fn interpolate(start: f32, end: f32, fraction: f32) -> f32 {
    (end - start).mul_add(fraction, start)
}

fn cell_span(first: f32, second: f32, scale: f32, maximum: i32) -> Option<(i32, i32)> {
    let minimum = (first.min(second) / scale)
        .floor()
        .to_i32()?
        .clamp(0, maximum);
    let maximum_value = (first.max(second) / scale)
        .floor()
        .to_i32()?
        .clamp(0, maximum);
    Some((minimum, maximum_value))
}

fn nearest_triangle_hit(
    mut nearest: Option<f32>,
    triangles: [[Vec3; 3]; 2],
    start: Vec3,
    direction: Vec3,
    entry: f32,
    exit: f32,
) -> Option<f32> {
    for triangle in triangles {
        let Some(fraction) = segment_triangle_fraction(start, direction, triangle) else {
            continue;
        };
        if fraction + f32::EPSILON < entry || fraction - f32::EPSILON > exit {
            continue;
        }
        nearest = Some(nearest.map_or(fraction, |current| current.min(fraction)));
    }
    nearest
}

fn segment_triangle_fraction(start: Vec3, direction: Vec3, triangle: [Vec3; 3]) -> Option<f32> {
    const INTERSECTION_EPSILON: f32 = 0.000_01;
    let first_edge = triangle[1] - triangle[0];
    let second_edge = triangle[2] - triangle[0];
    let cross = direction.cross(second_edge);
    let determinant = first_edge.dot(cross);
    if determinant.abs() <= INTERSECTION_EPSILON {
        return None;
    }
    let inverse = determinant.recip();
    let from_vertex = start - triangle[0];
    let first_weight = from_vertex.dot(cross) * inverse;
    if !(-INTERSECTION_EPSILON..=1.0 + INTERSECTION_EPSILON).contains(&first_weight) {
        return None;
    }
    let second_cross = from_vertex.cross(first_edge);
    let second_weight = direction.dot(second_cross) * inverse;
    if second_weight < -INTERSECTION_EPSILON
        || first_weight + second_weight > 1.0 + INTERSECTION_EPSILON
    {
        return None;
    }
    let fraction = second_edge.dot(second_cross) * inverse;
    (-INTERSECTION_EPSILON..=1.0 + INTERSECTION_EPSILON)
        .contains(&fraction)
        .then(|| fraction.clamp(0.0, 1.0))
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

    pub(super) fn projectile_terrain_intersection(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        self.terrain_simulation
            .as_ref()
            .and_then(|terrain| terrain.projectile_segment_intersection(start, end))
    }
}

#[cfg(test)]
mod tests;
