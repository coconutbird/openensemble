//! Collision queries on the heightfield: the sphere distances, the height range and the box.
//!
//! The rules follow the retail shape `BTerrainPhysicsHeightfieldShape`: `collideSpheres`
//! (`0x14041B730`), `getHeightAt` (`0x14041B3B0`) with the triangle flip on, and the Havok
//! `hkpSampledHeightFieldShape::getAabb` (`0x140874E70`).

use glam::{Mat3, Vec3, Vec4};

use crate::heightfield::HeightField;
use crate::math::Transform;
use crate::shape::Aabb;

/// `HK_REAL_MAX` (`0x7F7FFFEE`), the distance of a sphere outside the heightfield.
pub(crate) const REAL_MAX: f32 = f32::from_bits(0x7F7F_FFEE);

/// The float that moves the cell index into the low mantissa bits (`xmmword_1411BA290`).
///
/// Adding `3 · 2^16` leaves 6 fraction bits in the mantissa, so `bits >> 6` is the rounded
/// integer part in the low 16 bits.
const INDEX_MAGIC: f32 = 196_608.0;

/// The surface of the heightfield at one point, in the shape space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceSample {
    /// The terrain height at the point.
    pub(crate) height: f32,
    /// The unit normal of the triangle under the point.
    pub(crate) normal: Vec3,
}

/// Returns the floor correction of the cell index (`0x140D843F0`).
///
/// The bisection finds the smallest offset below 1 that the index rounding still maps to the
/// next integer. Adding the offset before the rounding makes the rounding a floor.
pub(crate) fn floor_correction() -> f32 {
    let mut high = 11.0_f32;
    let mut low = 10.0_f32;
    for _ in 0..23 {
        let middle = f32::midpoint(high, low);
        if cell_index(middle) >= 11 {
            high = middle;
        } else {
            low = middle;
        }
    }
    f32::midpoint(high, low) - 11.0
}

/// Converts a scaled coordinate to a cell index like retail: the low 16 bits of the rounded
/// value. A negative coordinate gives a large index.
fn cell_index(value: f32) -> u16 {
    let [low, high, ..] = ((value + INDEX_MAGIC).to_bits() >> 6).to_le_bytes();
    u16::from_le_bytes([low, high])
}

/// Converts a grid index to a float. The grid indices fit in 16 bits, like the retail ones.
pub(crate) fn index_to_f32(index: usize) -> f32 {
    u16::try_from(index).map_or(f32::from(u16::MAX), f32::from)
}

impl HeightField {
    /// Returns the scale of the shape space: the tile scale on X and Z, 1 on Y.
    fn scale(&self) -> Vec3 {
        Vec3::new(self.tile_scale, 1.0, self.tile_scale)
    }

    /// Returns the factors from the shape space to the cell space (`m_floatToIntScale`).
    fn float_to_int_scale(&self) -> Vec3 {
        Vec3::new(1.0 / self.tile_scale, 1.0, 1.0 / self.tile_scale)
    }

    /// Returns the lowest and the highest height of the grid.
    ///
    /// Retail computes this range once, at the first `getAabb` call, and never again. The
    /// collision world keeps the range of the terrain body for the same reason.
    #[must_use]
    pub(crate) fn height_range(&self) -> (f32, f32) {
        self.heights
            .iter()
            .fold((self.heights[0], self.heights[0]), |(min, max), height| {
                (
                    if *height <= min { *height } else { min },
                    if max <= *height { *height } else { max },
                )
            })
    }

    /// Returns the world box of the heightfield with a cached height range (`getAabb`).
    pub(crate) fn aabb(&self, transform: &Transform, tolerance: f32, range: (f32, f32)) -> Aabb {
        let last = index_to_f32(self.resolution()) - 1.0;
        let extent = Vec3::new(last, range.1 - range.0, last) * self.scale();
        let half = extent * 0.5;
        let center = Vec3::new(half.x, f32::midpoint(range.0, range.1), half.z);
        let rotation = transform.rotation;
        let abs = Mat3::from_cols(
            rotation.x_axis.abs(),
            rotation.y_axis.abs(),
            rotation.z_axis.abs(),
        );
        Aabb::from_center_half_extents(
            transform.transform_point(center),
            abs * half + Vec3::splat(tolerance),
        )
    }

    /// Returns the surface under a point of the shape space, or `None` outside the grid.
    ///
    /// The cell is `floor(x / s)`, `floor(z / s)` through the retail rounding. The diagonal of
    /// each cell runs from `(x, z)` to `(x + 1, z + 1)`.
    pub(crate) fn sample(&self, point: Vec3) -> Option<SurfaceSample> {
        let to_int = self.float_to_int_scale();
        let scaled = point * to_int;
        let offset = floor_correction() * self.tile_scale;
        let rounded = (Vec3::new(offset, 0.0, offset) + point) * to_int;
        let index_x = cell_index(rounded.x);
        let index_z = cell_index(rounded.z);
        let (cell_x, cell_z) = (usize::from(index_x), usize::from(index_z));
        let last_cell = self.resolution() - 1;
        if cell_x >= last_cell || cell_z >= last_cell {
            return None;
        }
        let sub_x = scaled.x - f32::from(index_x);
        let sub_z = scaled.z - f32::from(index_z);
        let h00 = self.height_at(cell_x, cell_z);
        let h11 = self.height_at(cell_x + 1, cell_z + 1);
        let (dx, dz) = if sub_x <= sub_z {
            let h01 = self.height_at(cell_x, cell_z + 1);
            (h11 - h01, h01 - h00)
        } else {
            let h10 = self.height_at(cell_x + 1, cell_z);
            (h10 - h00, h11 - h10)
        };
        let height = (dz * sub_z + h00) + dx * sub_x;
        let normal = (Vec3::new(-dx, 1.0, -dz) * to_int).normalize();
        Some(SurfaceSample { height, normal })
    }

    /// Measures each sphere against the terrain (`collideSpheres`).
    ///
    /// The spheres are in the shape space, with the radius in `w`. Each result has the triangle
    /// normal in `xyz` and the distance in `w`: the vertical gap between the centre and the
    /// surface, minus the radius. This is not the distance along the normal. A sphere outside the
    /// grid gets the normal `(0, 1, 0)` and the distance [`REAL_MAX`].
    pub(crate) fn collide_spheres(&self, spheres: &[Vec4], out: &mut Vec<Vec4>) {
        out.clear();
        out.extend(spheres.iter().map(|sphere| {
            let center = sphere.truncate();
            match self.sample(center) {
                Some(surface) => surface
                    .normal
                    .extend((center.y - surface.height) - sphere.w),
                None => Vec4::new(0.0, 1.0, 0.0, REAL_MAX),
            }
        }));
    }

    /// Returns the terrain height and the triangle normal under a world point, or `None` outside
    /// the grid and its skirt.
    ///
    /// The terrain body must sit at [`HeightField::body_position`] with no rotation, as retail
    /// places it.
    #[must_use]
    pub fn surface_at(&self, x: f32, z: f32) -> Option<(f32, Vec3)> {
        let origin = self.body_position();
        self.sample(Vec3::new(x - origin.x, 0.0, z - origin.z))
            .map(|surface| (surface.height + origin.y, surface.normal))
    }
}
