//! Retail XSD simulation-terrain height, surface and collision queries.

mod ground_sample;
mod ground_tie;

use super::World;
use crate::physics::terrain_height_field;
use crate::sync::SyncChecksum;
use ::physics::HeightField;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::xsd;
use std::sync::Arc;

/// Immutable simulation height grid decoded from a scenario XSD.
///
/// The heights live in one place: the heightfield that the terrain body of the physics world
/// shares. The XSD keeps only the grid layout and the tile types.
#[derive(Debug)]
pub(super) struct TerrainSimulation {
    terrain: xsd::Terrain,
    /// The sim heights, or `None` if they cannot make a heightfield. Then the XSD keeps them
    /// and the world has no terrain body.
    heights: Option<Arc<HeightField>>,
    fingerprint: u32,
}

impl TerrainSimulation {
    fn new(mut terrain: xsd::Terrain) -> Self {
        let fingerprint = terrain_fingerprint(&terrain);
        let heights = match terrain_height_field(&terrain) {
            Ok(field) => {
                terrain.heights = Vec::new();
                Some(Arc::new(field))
            }
            Err(error) => {
                log::warn!("the sim terrain makes no physics heightfield: {error}");
                None
            }
        };
        Self {
            terrain,
            heights,
            fingerprint,
        }
    }

    fn surface_type(&self, position: Vec3) -> Option<u8> {
        if !position.is_finite() {
            return None;
        }
        let x = checked_grid_coordinate(
            world_to_grid(position.x, self.terrain.data_tile_scale),
            self.terrain.data_axis,
            true,
        )?;
        let z = checked_grid_coordinate(
            world_to_grid(position.z, self.terrain.data_tile_scale),
            self.terrain.data_axis,
            true,
        )?;
        self.terrain.tile_type(x, z)
    }

    fn height(&self, position: Vec3, clamp: bool) -> Option<f32> {
        if !position.is_finite() {
            return None;
        }
        let x = checked_grid_coordinate(
            world_to_grid(position.x, self.terrain.height_tile_scale),
            self.terrain.height_axis,
            clamp,
        )?;
        let z = checked_grid_coordinate(
            world_to_grid(position.z, self.terrain.height_tile_scale),
            self.terrain.height_axis,
            clamp,
        )?;
        let x = i32::try_from(x).ok()?;
        let z = i32::try_from(z).ok()?;
        let world_x = x.to_f32()? * self.terrain.height_tile_scale;
        let world_z = z.to_f32()? * self.terrain.height_tile_scale;
        let x_fraction = (position.x - world_x) / self.terrain.height_tile_scale;
        let z_fraction = (position.z - world_z) / self.terrain.height_tile_scale;
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

    const fn tile_scale(&self) -> f32 {
        self.terrain.height_tile_scale
    }

    /// Sim data tiles on each side and their world size (`gTerrainSimDataTileCount`,
    /// `gTerrainSimDataTileScale`).
    pub(super) const fn data_grid(&self) -> (usize, f32) {
        (self.terrain.data_axis, self.terrain.data_tile_scale)
    }

    fn height_sample(&self, x: i32, z: i32, clamp: bool) -> Option<f32> {
        let x = checked_grid_coordinate(x, self.terrain.height_axis, clamp)?;
        let z = checked_grid_coordinate(z, self.terrain.height_axis, clamp)?;
        match &self.heights {
            Some(field) => field.sim_height(x, z),
            None => self.terrain.height_sample(x, z),
        }
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
            || self.terrain.height_axis < 2
        {
            return None;
        }
        let maximum_world =
            (self.terrain.height_axis - 1).to_f32()? * self.terrain.height_tile_scale;
        let (entry, exit) = clip_segment_xz(start, direction, maximum_world)?;
        let clipped_start = start + direction * entry;
        if clipped_start.y < self.height(clipped_start, false)? {
            return None;
        }
        let clipped_end = start + direction * exit;
        let maximum_cell = i32::try_from(self.terrain.height_axis - 2).ok()?;
        let (minimum_x, maximum_x) = cell_span(
            clipped_start.x,
            clipped_end.x,
            self.terrain.height_tile_scale,
            maximum_cell,
        )?;
        let (minimum_z, maximum_z) = cell_span(
            clipped_start.z,
            clipped_end.z,
            self.terrain.height_tile_scale,
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
        let x1 = x.to_f32()? * self.terrain.height_tile_scale;
        let z1 = z.to_f32()? * self.terrain.height_tile_scale;
        let x2 = x1 + self.terrain.height_tile_scale;
        let z2 = z1 + self.terrain.height_tile_scale;
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
        checksum.hash_u32(u32::try_from(self.terrain.height_axis).unwrap_or(u32::MAX));
        checksum.hash_f32(self.terrain.height_tile_scale);
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

fn terrain_fingerprint(terrain: &xsd::Terrain) -> u32 {
    let mut checksum = SyncChecksum::new();
    checksum.hash_u32(u32::try_from(terrain.data_axis).unwrap_or(u32::MAX));
    checksum.hash_f32(terrain.data_tile_scale);
    checksum.hash_u32(u32::try_from(terrain.height_axis).unwrap_or(u32::MAX));
    checksum.hash_u32(u32::try_from(terrain.cache_axis).unwrap_or(u32::MAX));
    checksum.hash_f32(terrain.height_tile_scale);
    for &height in &terrain.heights {
        checksum.hash_f32(height);
    }
    if let Some(tile_types) = &terrain.tile_types {
        checksum.hash_u32(1);
        checksum.hash_bytes(tile_types);
    } else {
        checksum.hash_u32(0);
    }
    checksum.value()
}

/// Build an 8 by 8 flat terrain with a unit data tile scale for tests.
#[cfg(test)]
pub(crate) fn flat_test_terrain(height_tile_scale: f32, height: f32) -> xsd::Terrain {
    xsd::Terrain {
        byte_order: xsd::ByteOrder::Big,
        data_axis: xsd::HEIGHT_BLOCK_AXIS,
        data_tile_scale: 1.0,
        height_axis: xsd::HEIGHT_BLOCK_AXIS,
        cache_axis: xsd::HEIGHT_BLOCK_AXIS,
        height_tile_scale,
        heights: vec![height; xsd::HEIGHT_BLOCK_SIZE],
        tile_types: None,
    }
}

/// Build a square test terrain of `blocks` by `blocks` height blocks.
///
/// Each sample takes `height(x, z)` at its world position.
#[cfg(test)]
pub(crate) fn sampled_test_terrain(
    blocks: usize,
    height_tile_scale: f32,
    height: impl Fn(f32, f32) -> f32,
) -> xsd::Terrain {
    let axis = blocks * xsd::HEIGHT_BLOCK_AXIS;
    let mut terrain = xsd::Terrain {
        byte_order: xsd::ByteOrder::Big,
        data_axis: axis,
        data_tile_scale: height_tile_scale,
        height_axis: axis,
        cache_axis: axis,
        height_tile_scale,
        heights: vec![0.0; axis * axis],
        tile_types: None,
    };
    for z in 0..axis {
        for x in 0..axis {
            let index = terrain
                .height_index(x, z)
                .expect("the sample is inside the grid");
            let world_x = x.to_f32().expect("the grid index fits f32") * height_tile_scale;
            let world_z = z.to_f32().expect("the grid index fits f32") * height_tile_scale;
            terrain.heights[index] = height(world_x, world_z);
        }
    }
    terrain
}

impl World {
    /// Use the decoded scenario XSD for synchronized terrain queries.
    pub fn configure_terrain_simulation(&mut self, terrain: xsd::Terrain) {
        let simulation = TerrainSimulation::new(terrain);
        if let Some(heights) = &simulation.heights {
            self.physics.set_terrain(heights);
        }
        self.terrain_simulation = Some(simulation);
        // `BWorld` setup calls `BVisibleMap_initMap` with the data tile count on both axes.
        self.reset_visibility_grid();
    }

    /// Replaces the physics world with an empty one that has the terrain body of the current
    /// terrain.
    pub(super) fn reset_physics(&mut self) {
        self.physics = crate::physics::SimPhysics::new();
        if let Some(heights) = self
            .terrain_simulation
            .as_ref()
            .and_then(|terrain| terrain.heights.as_ref())
        {
            self.physics.set_terrain(heights);
        }
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

    /// Return the retail terrain surface byte at a world-space point.
    ///
    /// Coordinates are truncated and clamped exactly like `BTerrainSimRep`.
    #[must_use]
    pub fn terrain_surface_type(&self, position: Vec3) -> Option<u8> {
        self.terrain_simulation
            .as_ref()
            .and_then(|terrain| terrain.surface_type(position))
    }

    pub(super) fn terrain_simulation_tile_scale(&self) -> f32 {
        self.terrain_simulation
            .as_ref()
            .map_or(0.0, TerrainSimulation::tile_scale)
    }

    pub(super) fn projectile_terrain_intersection(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        self.terrain_simulation
            .as_ref()
            .and_then(|terrain| terrain.projectile_segment_intersection(start, end))
    }

    #[cfg(test)]
    pub(super) fn configure_flat_test_terrain(&mut self, height: f32) {
        self.configure_terrain_simulation(flat_test_terrain(1.0, height));
    }
}

#[cfg(test)]
mod tests;
