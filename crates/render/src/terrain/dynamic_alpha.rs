//! CPU mirror for the renderer-owned dynamic terrain visibility mask.

use glam::Vec2;
use num_traits::ToPrimitive;

const SAMPLES_PER_TERRAIN_CELL: u32 = 4;
const BITS_PER_WORD: u32 = 32;

/// Bit-packed dynamic terrain visibility at retail's four-samples-per-cell density.
#[derive(Clone, Debug)]
pub struct DynamicTerrainAlphaMask {
    dimension: u32,
    words_per_row: u32,
    words: Vec<u32>,
}

impl DynamicTerrainAlphaMask {
    /// Creates an all-visible mask for a square packed terrain grid.
    #[must_use]
    pub fn new(terrain_dimension: u32) -> Option<Self> {
        if terrain_dimension == 0 {
            return None;
        }
        let dimension = terrain_dimension.checked_mul(SAMPLES_PER_TERRAIN_CELL)?;
        let words_per_row = dimension.div_ceil(BITS_PER_WORD);
        let word_count = words_per_row
            .checked_mul(dimension)
            .and_then(|count| usize::try_from(count).ok())?;
        Some(Self {
            dimension,
            words_per_row,
            words: vec![u32::MAX; word_count],
        })
    }

    /// Logical pixel width and height of the unpacked visibility image.
    #[must_use]
    pub const fn dimension(&self) -> u32 {
        self.dimension
    }

    /// `R32Uint` texels in each bit-packed texture row.
    #[must_use]
    pub const fn words_per_row(&self) -> u32 {
        self.words_per_row
    }

    /// Packed rows ready for upload to an `R32Uint` texture.
    #[must_use]
    pub fn words(&self) -> &[u32] {
        &self.words
    }

    /// Writes a world-space circle, correcting retail's world-radius/pixel-radius mix-up.
    pub fn set_circle_world(
        &mut self,
        center: Vec2,
        radius: f32,
        world_min: Vec2,
        world_max: Vec2,
        enabled: bool,
    ) -> bool {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        let Some(space) = RasterSpace::new(self.dimension, world_min, world_max) else {
            return false;
        };
        let radius_vector = Vec2::splat(radius);
        let Some(range) = space.pixel_range(center - radius_vector, center + radius_vector) else {
            return false;
        };
        let radius_squared = radius * radius;
        self.write_range(range, enabled, |pixel| {
            space.pixel_to_world(pixel).distance_squared(center) <= radius_squared
        })
    }

    /// Writes a world-space rectangle oriented by normalized object X/Z axes.
    pub fn set_oriented_rectangle_world(
        &mut self,
        center: Vec2,
        axes: [Vec2; 2],
        half_extents: Vec2,
        world_bounds: [Vec2; 2],
        enabled: bool,
    ) -> bool {
        let [axis_x, axis_z] = axes;
        let [world_min, world_max] = world_bounds;
        if !center.is_finite()
            || !axis_x.is_finite()
            || !axis_z.is_finite()
            || !half_extents.is_finite()
            || half_extents.min_element() < 0.0
        {
            return false;
        }
        let Some(axis_x) = axis_x.try_normalize() else {
            return false;
        };
        let Some(axis_z) = (axis_z - axis_x * axis_z.dot(axis_x)).try_normalize() else {
            return false;
        };
        let Some(space) = RasterSpace::new(self.dimension, world_min, world_max) else {
            return false;
        };
        let extent_x = axis_x * half_extents.x;
        let extent_z = axis_z * half_extents.y;
        let corners = [
            center - extent_x - extent_z,
            center - extent_x + extent_z,
            center + extent_x + extent_z,
            center + extent_x - extent_z,
        ];
        let minimum = corners
            .iter()
            .copied()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min);
        let maximum = corners
            .iter()
            .copied()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
        let Some(range) = space.pixel_range(minimum, maximum) else {
            return false;
        };
        self.write_range(range, enabled, |pixel| {
            let delta = space.pixel_to_world(pixel) - center;
            delta.dot(axis_x).abs() <= half_extents.x && delta.dot(axis_z).abs() <= half_extents.y
        })
    }

    fn write_range(
        &mut self,
        range: PixelRange,
        enabled: bool,
        contains: impl Fn(Vec2) -> bool,
    ) -> bool {
        let mut changed = false;
        for y in range.min_y..=range.max_y {
            for x in range.min_x..=range.max_x {
                if contains(Vec2::new(
                    x.to_f32().unwrap_or(f32::MAX),
                    y.to_f32().unwrap_or(f32::MAX),
                )) {
                    changed |= self.set_pixel(x, y, enabled);
                }
            }
        }
        changed
    }

    fn set_pixel(&mut self, x: u32, y: u32, enabled: bool) -> bool {
        let Some(index) = y
            .checked_mul(self.words_per_row)
            .and_then(|row| row.checked_add(x / BITS_PER_WORD))
            .and_then(|index| usize::try_from(index).ok())
        else {
            return false;
        };
        let Some(word) = self.words.get_mut(index) else {
            return false;
        };
        let bit = 1_u32 << (x % BITS_PER_WORD);
        let previous = *word;
        if enabled {
            *word |= bit;
        } else {
            *word &= !bit;
        }
        *word != previous
    }
}

#[derive(Clone, Copy)]
struct RasterSpace {
    world_min: Vec2,
    world_extent: Vec2,
    maximum_pixel: f32,
}

impl RasterSpace {
    fn new(dimension: u32, world_min: Vec2, world_max: Vec2) -> Option<Self> {
        let world_extent = world_max - world_min;
        if !world_min.is_finite() || !world_max.is_finite() || world_extent.min_element() <= 0.0 {
            return None;
        }
        Some(Self {
            world_min,
            world_extent,
            maximum_pixel: dimension.checked_sub(1)?.to_f32()?,
        })
    }

    fn pixel_to_world(self, pixel: Vec2) -> Vec2 {
        self.world_min + pixel / self.maximum_pixel * self.world_extent
    }

    fn world_to_pixel(self, world: Vec2) -> Vec2 {
        (world - self.world_min) / self.world_extent * self.maximum_pixel
    }

    fn pixel_range(self, world_min: Vec2, world_max: Vec2) -> Option<PixelRange> {
        let minimum = self.world_to_pixel(world_min);
        let maximum = self.world_to_pixel(world_max);
        if maximum.x < 0.0
            || maximum.y < 0.0
            || minimum.x > self.maximum_pixel
            || minimum.y > self.maximum_pixel
        {
            return None;
        }
        Some(PixelRange {
            min_x: minimum.x.floor().clamp(0.0, self.maximum_pixel).to_u32()?,
            min_y: minimum.y.floor().clamp(0.0, self.maximum_pixel).to_u32()?,
            max_x: maximum.x.ceil().clamp(0.0, self.maximum_pixel).to_u32()?,
            max_y: maximum.y.ceil().clamp(0.0, self.maximum_pixel).to_u32()?,
        })
    }
}

#[derive(Clone, Copy)]
struct PixelRange {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::DynamicTerrainAlphaMask;

    fn enabled(mask: &DynamicTerrainAlphaMask, x: u32, y: u32) -> bool {
        let index = usize::try_from(y * mask.words_per_row() + x / 32).expect("word index");
        mask.words()[index] & (1_u32 << (x % 32)) != 0
    }

    #[test]
    fn retail_density_is_four_times_the_terrain_grid() {
        let mask = DynamicTerrainAlphaMask::new(8).expect("mask");
        assert_eq!(mask.dimension(), 32);
        assert_eq!(mask.words_per_row(), 1);
        assert_eq!(mask.words().len(), 32);
        assert!(mask.words().iter().all(|word| *word == u32::MAX));
    }

    #[test]
    fn circle_can_cut_and_restore_a_world_region() {
        let mut mask = DynamicTerrainAlphaMask::new(4).expect("mask");
        let changed =
            mask.set_circle_world(Vec2::splat(8.0), 2.0, Vec2::ZERO, Vec2::splat(16.0), false);
        assert!(changed);
        assert!(!enabled(&mask, 8, 8));
        assert!(enabled(&mask, 2, 2));
        assert!(mask.set_circle_world(Vec2::splat(8.0), 2.0, Vec2::ZERO, Vec2::splat(16.0), true,));
        assert!(enabled(&mask, 8, 8));
    }

    #[test]
    fn oriented_rectangle_uses_object_axes() {
        let mut mask = DynamicTerrainAlphaMask::new(8).expect("mask");
        assert!(mask.set_oriented_rectangle_world(
            Vec2::splat(16.0),
            [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0)],
            Vec2::new(6.0, 1.0),
            [Vec2::ZERO, Vec2::splat(32.0)],
            false,
        ));
        assert!(!enabled(&mask, 19, 19));
        assert!(enabled(&mask, 19, 13));
    }
}
