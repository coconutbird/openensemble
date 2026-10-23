//! Ray casts against the heightfield (`hkpSampledHeightFieldShape::castRay`).
//!
//! Retail `castRay` (`0x1408750B0`) and `castRayWithCollector` (`0x140875120`) call
//! `castRayInternal` (`0x1408753F0`). Both give no report for a start below the surface.
//! Only `castSphere` (`0x140875030`) sets that report, and the terrain rays do not use it.
//!
//! The terrain shape always sets the triangle flip (`getTriangleFlip`, `0x14041B470`). The
//! diagonal of each cell runs from `(x, z)` to `(x + 1, z + 1)`, so this file follows only the
//! flipped branches of `castRayInternal`.
//!
//! The walk uses three axes in the cell space: X, the diagonal coordinate `x - z` and Z. For
//! each axis it keeps the index of the next grid line, the ray parameter of the next crossing,
//! the parameter step between two crossings and the step direction. The lines of the three
//! axes are the edges of the triangles, so the walk visits each triangle along the ray.

use glam::Vec3;

use crate::heightfield::HeightField;
use crate::heightfield::query::{REAL_MAX, floor_correction};

/// The smallest coordinate change that is not parallel to an axis (`dword_1411F3B64`, 2^-15).
const PARALLEL_EPSILON: f32 = 1.0 / 32_768.0;

/// The float that moves the cell index into the low mantissa bits (`xmmword_1411BA290`).
const INDEX_MAGIC: f32 = 196_608.0;

/// The hit fraction limit of the nearest hit collector at the start (`m_hitFraction` = 1).
const START_EARLY_OUT: f32 = 1.0;

/// The ray parameter of the crossing before the first one (`flt_1411F431C`).
const NO_PREVIOUS_CROSSING: f32 = -1.0;

/// The X axis of the walk.
const X: usize = 0;
/// The diagonal axis of the walk: the coordinate `x - z`.
const DIAGONAL: usize = 1;
/// The Z axis of the walk.
const Z: usize = 2;

/// A ray hit on the heightfield, in the shape space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeightFieldRayHit {
    /// The hit fraction along the ray, from 0 at the start to 1 at the end.
    pub(crate) fraction: f32,
    /// The unit normal of the hit triangle, in the shape space.
    pub(crate) normal: Vec3,
}

impl HeightField {
    /// Casts a ray from `from` to `to` in the shape space and returns the nearest hit.
    ///
    /// The rules follow `castRay` with the nearest hit collector:
    ///
    /// - The hit fraction must be less than 1.
    /// - A ray that goes up through the surface gives no hit.
    /// - A ray that starts below the surface gives no hit in the triangle of its start. It can
    ///   hit a later triangle after it comes out above the surface.
    /// - A ray that is almost vertical tests only the triangle under its start.
    pub(crate) fn cast_ray_local(&self, from: Vec3, to: Vec3) -> Option<HeightFieldRayHit> {
        let to_int = Vec3::new(1.0 / self.tile_scale, 1.0, 1.0 / self.tile_scale);
        let offset = floor_correction() * self.tile_scale;
        let rounded = (Vec3::new(offset, 0.0, offset) + from) * to_int;
        let walk = RayWalk::new(
            self,
            to_int,
            CellPoint::new(from * to_int),
            CellPoint::new(to * to_int),
            [signed_cell_index(rounded.x), signed_cell_index(rounded.z)],
        );
        if walk.is_vertical() {
            walk.cast_vertical()
        } else {
            walk.cast_along()
        }
    }
}

/// Converts a scaled coordinate to a cell index like `castRayInternal`: the rounded value,
/// sign-extended from 16 bits (`movsx`).
fn signed_cell_index(value: f32) -> i32 {
    let [low, high, ..] = ((value + INDEX_MAGIC).to_bits() >> 6).to_le_bytes();
    i32::from(i16::from_le_bytes([low, high]))
}

/// Converts an index to a float with the rounding of `cvtdq2ps`.
#[expect(
    clippy::cast_precision_loss,
    reason = "retail converts the i32 index with cvtdq2ps, which rounds to the nearest float"
)]
fn index_float(index: i32) -> f32 {
    index as f32
}

/// Truncates a float to an integer like `cvttss2si`: a value outside the `i32` range or a NaN
/// gives `i32::MIN`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "retail truncates the float with cvttss2si, and the range check covers the rest"
)]
fn truncate(value: f32) -> i32 {
    if (-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        value as i32
    } else {
        i32::MIN
    }
}

/// Returns `true` if `index` is below `limit` in an unsigned compare: a negative index is
/// outside.
fn is_below(index: i32, limit: i32) -> bool {
    index.cast_unsigned() < limit.cast_unsigned()
}

/// Returns the first grid line above `coordinate`, from a guess at or below the coordinate.
fn first_line_above(coordinate: f32, guess: i32) -> i32 {
    let mut line = guess;
    while coordinate >= index_float(line) {
        line = line.wrapping_add(1);
    }
    line
}

/// Scales a vector to unit length like retail: `1 / sqrt` and one Newton step.
fn normalize(vector: Vec3) -> Vec3 {
    let length_squared = vector.z * vector.z + (vector.y * vector.y + vector.x * vector.x);
    let inverse = 1.0 / length_squared.sqrt();
    let refined = (0.5 * inverse) * (3.0 - (length_squared * inverse) * inverse);
    vector * refined
}

/// A point of the ray in the cell space, with the diagonal coordinate `x - z`.
#[derive(Clone, Copy, Debug)]
struct CellPoint {
    x: f32,
    diagonal: f32,
    z: f32,
    y: f32,
}

impl CellPoint {
    /// Makes the point from a point that is already in the cell space.
    fn new(scaled: Vec3) -> Self {
        Self {
            x: scaled.x,
            diagonal: scaled.x - scaled.z,
            z: scaled.z,
            y: scaled.y,
        }
    }

    /// Returns the point at the ray parameter `t` from `self` to `to`.
    fn lerp(self, to: Self, t: f32) -> Self {
        let keep = 1.0 - t;
        Self {
            x: keep * self.x + to.x * t,
            diagonal: keep * self.diagonal + to.diagonal * t,
            z: keep * self.z + to.z * t,
            y: keep * self.y + to.y * t,
        }
    }
}

/// One axis of the walk at the start.
struct AxisStart {
    /// The next grid line.
    line: i32,
    /// The step direction: -1 for a parallel axis.
    step: i32,
    /// The ray parameter between two crossings: 0 for a parallel axis.
    t_delta: f32,
    /// The ray parameter of the next crossing: [`REAL_MAX`] for a parallel axis.
    t_max: f32,
    /// The inverse of the coordinate change, or `None` for a parallel axis.
    inverse: Option<f32>,
}

impl AxisStart {
    /// Sets up one axis from the start and end coordinate and the cell of the start.
    fn new(from: f32, to: f32, cell: i32) -> Self {
        let change = to - from;
        if PARALLEL_EPSILON > change.abs() {
            return Self {
                line: cell,
                step: -1,
                t_delta: 0.0,
                t_max: REAL_MAX,
                inverse: None,
            };
        }
        let inverse = 1.0 / change;
        let (line, step, t_delta) = if 0.0 > change {
            (cell, -1, -inverse)
        } else {
            (cell.wrapping_add(1), 1, inverse)
        };
        Self {
            line,
            step,
            t_delta,
            t_max: (index_float(line) - from) * inverse,
            inverse: Some(inverse),
        }
    }
}

/// The ray height and the surface height at the last crossing.
#[derive(Clone, Copy)]
struct Crossing {
    t: f32,
    y: f32,
    height: f32,
}

/// The state of the triangle walk along one ray (`castRayInternal`).
struct RayWalk<'a> {
    field: &'a HeightField,
    resolution: i32,
    to_int: Vec3,
    from: CellPoint,
    to: CellPoint,
    /// The next grid line on each axis.
    line: [i32; 3],
    step: [i32; 3],
    t_delta: [f32; 3],
    t_max: [f32; 3],
    /// The inverse of the X change, or `None` if the ray is parallel to X.
    inverse_x: Option<f32>,
    /// The inverse of the Z change, or `None` if the ray is parallel to Z.
    inverse_z: Option<f32>,
    /// The absolute X change of the ray in the cell space.
    length_x: f32,
    /// The absolute Z change of the ray in the cell space.
    length_z: f32,
}

impl<'a> RayWalk<'a> {
    /// Sets up the three axes like `castRayInternal`.
    ///
    /// The diagonal line is not known yet: [`RayWalk::cast_along`] sets it after the clip.
    fn new(
        field: &'a HeightField,
        to_int: Vec3,
        from: CellPoint,
        to: CellPoint,
        [cell_x, cell_z]: [i32; 2],
    ) -> Self {
        let x = AxisStart::new(from.x, to.x, cell_x);
        let diagonal = AxisStart::new(from.diagonal, to.diagonal, 0);
        let z = AxisStart::new(from.z, to.z, cell_z);
        Self {
            field,
            resolution: i32::try_from(field.resolution()).unwrap_or(i32::MAX),
            to_int,
            from,
            to,
            line: [x.line, 0, z.line],
            step: [x.step, diagonal.step, z.step],
            t_delta: [x.t_delta, diagonal.t_delta, z.t_delta],
            t_max: [x.t_max, diagonal.t_max, z.t_max],
            inverse_x: x.inverse,
            inverse_z: z.inverse,
            length_x: (to.x - from.x).abs(),
            length_z: (to.z - from.z).abs(),
        }
    }

    /// Returns `true` if two of the three axes are parallel to the ray.
    fn is_vertical(&self) -> bool {
        self.t_delta[X] + self.t_delta[Z] == 0.0
            || self.t_delta[DIAGONAL] + self.t_delta[X] == 0.0
            || self.t_delta[DIAGONAL] + self.t_delta[Z] == 0.0
    }

    /// Returns the height of shape vertex `(x, z)` like `getHeightAt`: a negative index clamps
    /// to the first vertex.
    fn height(&self, x: i32, z: i32) -> f32 {
        self.field.height_at(
            usize::try_from(x).unwrap_or(0),
            usize::try_from(z).unwrap_or(0),
        )
    }

    /// Returns the unit normal of the slopes `(-dh/dx, 1, -dh/dz)` in the cell space.
    fn normal(&self, x: f32, z: f32) -> Vec3 {
        normalize(Vec3::new(x, 1.0, z) * self.to_int)
    }

    /// Tests the triangle under the start of an almost vertical ray.
    fn cast_vertical(&self) -> Option<HeightFieldRayHit> {
        let (cell_x, cell_z) = (self.line[X], self.line[Z]);
        if !is_below(cell_x, self.resolution - 1) || !is_below(cell_z, self.resolution - 1) {
            return None;
        }
        let sub_z = self.from.z - index_float(cell_z);
        let sub_x = self.from.x - index_float(cell_x);
        let h00 = self.height(cell_x, cell_z);
        let h11 = self.height(cell_x + 1, cell_z + 1);
        let (slope_x, slope_z) = if sub_x > sub_z {
            let h10 = self.height(cell_x + 1, cell_z);
            (h10 - h00, h11 - h10)
        } else {
            let h01 = self.height(cell_x, cell_z + 1);
            (h11 - h01, h01 - h00)
        };
        let height = (slope_z * sub_z + h00) + slope_x * sub_x;
        let end = self.to.y - height;
        let start = self.from.y - height;
        if end > start || 0.0 > start || end >= 0.0 {
            return None;
        }
        let fraction = start / (start - end);
        (START_EARLY_OUT > fraction).then(|| HeightFieldRayHit {
            fraction,
            normal: self.normal(-slope_x, -slope_z),
        })
    }

    /// Walks the triangles along the ray and returns the nearest hit.
    fn cast_along(mut self) -> Option<HeightFieldRayHit> {
        let mut current = self.from;
        let first_line_x = self.line[X];
        self.clip_x(&mut current)?;
        self.clip_z(&mut current, first_line_x)?;
        if !is_below(self.line[X], self.resolution)
            || !is_below(self.line[X].wrapping_sub(self.step[X]), self.resolution)
        {
            return None;
        }
        // The diagonal changes in each cell when X and Z step in opposite directions.
        let alternate = (self.step[Z] ^ self.step[X]) < 0 && self.t_delta[DIAGONAL] != 0.0;
        let mut grid_turn = self.start_diagonal(current, alternate);
        let steep = self.length_z > self.length_x;
        let last_crossing = |walk: &Self, axis: usize| {
            if walk.t_delta[axis] == 0.0 {
                -REAL_MAX
            } else {
                walk.t_max[axis] - walk.t_delta[axis]
            }
        };
        let entry = [
            last_crossing(&self, X),
            last_crossing(&self, DIAGONAL),
            last_crossing(&self, Z),
        ];
        let mut axis = entry_axis(entry, alternate, grid_turn, steep);
        self.t_max[axis] -= self.t_delta[axis];
        self.line[axis] = self.line[axis].wrapping_sub(self.step[axis]);
        if !is_below(self.line[X], self.resolution) {
            return None;
        }
        let mut early_out = START_EARLY_OUT;
        let mut best = None;
        let mut previous = Crossing {
            t: NO_PREVIOUS_CROSSING,
            y: 0.0,
            height: REAL_MAX,
        };
        loop {
            if !is_below(self.line[Z], self.resolution) || previous.t > early_out {
                return best;
            }
            let t = self.t_max[axis];
            let point = self.from.lerp(self.to, t);
            let height = self.edge_height(axis, point, alternate);
            let fraction = self.crossing_fraction(previous, t, point.y, height);
            previous = Crossing {
                t,
                y: point.y,
                height,
            };
            if let Some(fraction) = fraction {
                if fraction > early_out {
                    return best;
                }
                let normal = self.hit_normal(axis, alternate, steep);
                if early_out > fraction {
                    early_out = fraction;
                    best = Some(HeightFieldRayHit { fraction, normal });
                }
            }
            grid_turn = !grid_turn;
            self.t_max[axis] = t + self.t_delta[axis];
            self.line[axis] = self.line[axis].wrapping_add(self.step[axis]);
            axis = self.next_axis(alternate, grid_turn, steep);
            if !is_below(self.line[X], self.resolution) {
                return best;
            }
        }
    }

    /// Moves the start into the grid on X, and finds the Z line at the new start.
    ///
    /// Returns `None` if the ray stays outside the grid on X.
    fn clip_x(&mut self, current: &mut CellPoint) -> Option<()> {
        let t = if self.step[X] > 0 {
            if self.line[X] > 0 {
                return Some(());
            }
            if 0.0 > self.to.x {
                return None;
            }
            self.line[X] = 1;
            -(self.t_delta[X] * self.from.x)
        } else {
            let last = self.resolution - 2;
            if self.line[X] <= last {
                return Some(());
            }
            let edge = index_float(self.resolution - 1);
            if self.to.x > edge {
                return None;
            }
            self.line[X] = last;
            (self.from.x - edge) * self.t_delta[X]
        };
        self.t_max[X] = self.t_delta[X] + t;
        *current = self.from.lerp(self.to, t);
        let guess = self.line[Z]
            .wrapping_sub(2)
            .wrapping_add(truncate(t * self.length_z).wrapping_mul(self.step[Z]));
        self.line[Z] = (self.step[Z] >> 1).wrapping_add(first_line_above(current.z, guess));
        self.t_max[Z] = self.inverse_z.map_or(REAL_MAX, |inverse| {
            (index_float(self.line[Z]) - self.from.z) * inverse
        });
        Some(())
    }

    /// Moves the start into the grid on Z, and finds the X line at the new start.
    ///
    /// The X guess starts from `first_line_x`, the X line before [`RayWalk::clip_x`], like
    /// retail. Returns `None` if the ray stays outside the grid on Z.
    fn clip_z(&mut self, current: &mut CellPoint, first_line_x: i32) -> Option<()> {
        let t = if self.step[Z] > 0 {
            if self.line[Z] > 0 {
                return Some(());
            }
            if 0.0 > self.to.z {
                return None;
            }
            self.line[Z] = 1;
            -(self.t_delta[Z] * self.from.z)
        } else {
            let last = self.resolution - 2;
            if self.line[Z] <= last {
                return Some(());
            }
            let edge = index_float(self.resolution - 1);
            if self.to.z > edge {
                return None;
            }
            self.line[Z] = last;
            (self.from.z - edge) * self.t_delta[Z]
        };
        self.t_max[Z] = self.t_delta[Z] + t;
        *current = self.from.lerp(self.to, t);
        let guess = first_line_x
            .wrapping_sub(2)
            .wrapping_add(truncate(self.length_x * t).wrapping_mul(self.step[X]));
        self.line[X] = (self.step[X] >> 1).wrapping_add(first_line_above(current.x, guess));
        self.t_max[X] = self.inverse_x.map_or(REAL_MAX, |inverse| {
            (index_float(self.line[X]) - self.from.x) * inverse
        });
        Some(())
    }

    /// Sets the diagonal line and its next crossing at the start point.
    ///
    /// Returns `true` if the next step crosses a grid line, `false` if it can cross the
    /// diagonal.
    fn start_diagonal(&mut self, current: CellPoint, alternate: bool) -> bool {
        let mut grid_turn = true;
        self.line[DIAGONAL] = self.line[X].wrapping_sub(self.line[Z]);
        let offset = current.diagonal - index_float(self.line[DIAGONAL]);
        if alternate {
            if offset.abs() > 1.0 {
                self.line[DIAGONAL] = self.line[DIAGONAL].wrapping_sub(self.step[DIAGONAL]);
            } else {
                grid_turn = false;
            }
        } else if index_float(self.step[DIAGONAL]) * offset > 0.0 {
            grid_turn = false;
            self.line[DIAGONAL] = self.line[DIAGONAL].wrapping_add(self.step[DIAGONAL]);
        }
        self.t_max[DIAGONAL] = if self.t_delta[DIAGONAL] == 0.0 {
            REAL_MAX
        } else {
            ((index_float(self.line[DIAGONAL]) - self.from.diagonal) * self.t_delta[DIAGONAL])
                * index_float(self.step[DIAGONAL])
        };
        grid_turn
    }

    /// Returns the axis of the next crossing after a step.
    fn next_axis(&self, alternate: bool, grid_turn: bool, steep: bool) -> usize {
        let [t_x, t_diagonal, t_z] = self.t_max;
        if alternate {
            if !grid_turn {
                DIAGONAL
            } else if t_z > t_x {
                X
            } else {
                Z
            }
        } else if steep {
            if grid_turn {
                Z
            } else if t_diagonal <= t_x {
                DIAGONAL
            } else {
                X
            }
        } else if grid_turn {
            X
        } else if t_z <= t_diagonal {
            Z
        } else {
            DIAGONAL
        }
    }

    /// Returns the surface height where the ray crosses the line of `axis`.
    fn edge_height(&self, axis: usize, point: CellPoint, alternate: bool) -> f32 {
        let (line_x, line_z) = (self.line[X], self.line[Z]);
        let back_x = line_x.wrapping_sub(self.step[X]);
        let back_z = line_z.wrapping_sub(self.step[Z]);
        let (far, near, offset) = match axis {
            X => (
                self.height(line_x, back_z),
                self.height(line_x, line_z),
                (point.z - index_float(line_z)).abs(),
            ),
            Z => (
                self.height(back_x, line_z),
                self.height(line_x, line_z),
                (point.x - index_float(line_x)).abs(),
            ),
            _ if alternate => (
                self.height(back_x, line_z),
                self.height(line_x, back_z),
                (point.x - index_float(line_x)).abs(),
            ),
            _ => (
                self.height(back_x, back_z),
                self.height(line_x, line_z),
                (point.x - index_float(line_x)).abs(),
            ),
        };
        offset * (far - near) + near
    }

    /// Returns the hit fraction if the ray goes below the surface between the previous and
    /// this crossing.
    ///
    /// The ray must be below the surface at this crossing and above it at the previous one.
    /// For the first triangle after the start, the previous crossing is behind the start. Then
    /// the start must be above the surface.
    fn crossing_fraction(&self, previous: Crossing, t: f32, y: f32, height: f32) -> Option<f32> {
        if y >= height || previous.height > previous.y {
            return None;
        }
        let distance = y - height;
        if previous.t >= 0.0 {
            let previous_distance = previous.y - previous.height;
            return Some(
                previous_distance / (previous_distance - distance) * (t - previous.t) + previous.t,
            );
        }
        let ratio = t / (t - previous.t);
        let start_height = (1.0 - ratio) * height + ratio * previous.height;
        let start_distance = self.from.y - start_height;
        if 0.0 > start_distance {
            return None;
        }
        Some(start_distance / (start_distance - distance) * t)
    }

    /// Returns the normal of the triangle between the previous and this crossing.
    fn hit_normal(&self, axis: usize, alternate: bool, steep: bool) -> Vec3 {
        let (line_x, line_z) = (self.line[X], self.line[Z]);
        let back_x = line_x.wrapping_sub(self.step[X]);
        let back_z = line_z.wrapping_sub(self.step[Z]);
        let (along_x, along_z) = if alternate {
            let corner_x = self.height(line_x, back_z);
            let corner_z = self.height(back_x, line_z);
            if axis == DIAGONAL {
                let back = self.height(back_x, back_z);
                (back - corner_x, back - corner_z)
            } else {
                let next = self.height(line_x, line_z);
                (corner_z - next, corner_x - next)
            }
        } else {
            let back = self.height(back_x, back_z);
            let next = self.height(line_x, line_z);
            if axis == X || (axis == DIAGONAL && steep) {
                let corner = self.height(line_x, back_z);
                (back - corner, corner - next)
            } else {
                let corner = self.height(back_x, line_z);
                (corner - next, back - corner)
            }
        };
        self.normal(
            along_x * index_float(self.step[X]),
            along_z * index_float(self.step[Z]),
        )
    }
}

/// Returns the axis of the crossing before the start: the axis with the latest previous
/// crossing.
fn entry_axis(
    [previous_x, previous_diagonal, previous_z]: [f32; 3],
    alternate: bool,
    grid_turn: bool,
    steep: bool,
) -> usize {
    if alternate {
        if !grid_turn {
            DIAGONAL
        } else if previous_x > previous_z {
            X
        } else {
            Z
        }
    } else if steep {
        if grid_turn {
            Z
        } else if previous_x <= previous_diagonal {
            DIAGONAL
        } else {
            X
        }
    } else if grid_turn {
        X
    } else if previous_diagonal <= previous_z {
        Z
    } else {
        DIAGONAL
    }
}

#[cfg(test)]
mod tests;
