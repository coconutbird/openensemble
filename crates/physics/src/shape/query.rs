//! Shape queries for the collision detection: bounding boxes and collision spheres.
//!
//! The rules follow the Havok 5.1 shape methods in the retail binary:
//!
//! - `getAabb`: the shape box, expanded by the convex radius and the tolerance.
//! - `getCollisionSpheres` (vtable slots 9 and 10): the spheres that the heightfield agent
//!   tests against the terrain.

use glam::{Mat3, Vec3, Vec4};

use crate::math::Transform;
use crate::shape::Shape;

/// The sign of each box corner, in the order of the retail table at `0x140F17920`.
///
/// Corner `i` has a negative X for bit 0, a negative Y for bit 1 and a negative Z for bit 2.
const BOX_CORNER_SIGNS: [Vec3; 8] = [
    Vec3::new(1.0, 1.0, 1.0),
    Vec3::new(-1.0, 1.0, 1.0),
    Vec3::new(1.0, -1.0, 1.0),
    Vec3::new(-1.0, -1.0, 1.0),
    Vec3::new(1.0, 1.0, -1.0),
    Vec3::new(-1.0, 1.0, -1.0),
    Vec3::new(1.0, -1.0, -1.0),
    Vec3::new(-1.0, -1.0, -1.0),
];

/// The factor for the rim spheres of a long cylinder
/// (`m_cylBaseRadiusFactorForHeightFieldCollisions`, set to 0.8 in the constructor
/// `0x140D90B20`).
const CYLINDER_BASE_RADIUS_FACTOR: f32 = 0.8;

/// `cos(45°)` as the retail cylinder code stores it.
const FRAC_1_SQRT_2: f32 = 0.707_106_77;

/// An axis-aligned bounding box in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Aabb {
    /// The minimum corner.
    pub(crate) min: Vec3,
    /// The maximum corner.
    pub(crate) max: Vec3,
}

impl Aabb {
    /// Makes the box of a centre and half extents.
    pub(crate) fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            min: center - half_extents,
            max: center + half_extents,
        }
    }

    /// Returns `true` if the boxes overlap or touch.
    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
            && self.min.z <= other.max.z
            && other.min.z <= self.max.z
    }

    /// Returns the smallest box that holds both boxes.
    pub(crate) fn union(&self, other: &Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

/// Returns the box of local half extents around a local centre (`hkAabbUtil::calcAabb`).
fn transformed_box(transform: &Transform, center: Vec3, half_extents: Vec3, expand: f32) -> Aabb {
    let rotation = transform.rotation;
    let abs = Mat3::from_cols(
        rotation.x_axis.abs(),
        rotation.y_axis.abs(),
        rotation.z_axis.abs(),
    );
    let extent = abs * half_extents + Vec3::splat(expand);
    Aabb::from_center_half_extents(transform.transform_point(center), extent)
}

/// The data of a cylinder that the collision queries need (`hkpCylinderShape`).
pub(crate) struct CylinderFrame {
    /// The unit axis from `a` to `b`.
    pub(crate) axis: Vec3,
    /// The first unit vector at a right angle to the axis (`m_perpendicular1`).
    pub(crate) perpendicular1: Vec3,
    /// The second unit vector at a right angle to the axis (`m_perpendicular2`).
    pub(crate) perpendicular2: Vec3,
}

impl CylinderFrame {
    /// Builds the frame like `presetPerpendicularVectors` (`0x140D91D10`).
    ///
    /// The first perpendicular swaps the two largest components of the axis and negates one;
    /// the second is `axis × perpendicular1`.
    pub(crate) fn new(a: Vec3, b: Vec3) -> Self {
        let axis = (b - a).normalize_or_zero();
        let abs = axis.abs();
        // `larger` is the larger of X and Y; `smaller` is the other one.
        let (larger, smaller, smaller_value) = if abs.x > abs.y {
            (0, 1, abs.y)
        } else {
            (1, 0, abs.x)
        };
        // `other` is the second component to keep: Z, unless Z is the smallest.
        let other = if smaller_value > abs.z { smaller } else { 2 };
        let mut perpendicular = Vec3::ZERO;
        perpendicular[larger] = axis[other];
        perpendicular[other] = -axis[larger];
        let perpendicular1 = perpendicular.normalize_or_zero();
        let perpendicular2 = axis.cross(perpendicular1);
        Self {
            axis,
            perpendicular1,
            perpendicular2,
        }
    }
}

impl Shape {
    /// Returns `true` for a convex shape (`hkpConvexShape`).
    ///
    /// A transform of a convex shape counts as convex. Retail collapses each `.shp` transform
    /// of a convex child into an `hkpConvexTransformShape` or `hkpConvexTranslateShape`, which
    /// are convex shapes.
    pub(crate) fn is_convex(&self) -> bool {
        match self {
            Self::Box { .. }
            | Self::Sphere { .. }
            | Self::Capsule { .. }
            | Self::Cylinder { .. }
            | Self::ConvexVertices { .. } => true,
            Self::Transform { child, .. } => child.is_convex(),
            Self::List { .. } | Self::HeightField(_) => false,
        }
    }

    /// Returns the world box of the shape, expanded by `tolerance` (`getAabb`).
    ///
    /// A heightfield scans all its heights here. The collision world caches the height range
    /// of the terrain instead, as retail does.
    pub(crate) fn aabb(&self, transform: &Transform, tolerance: f32) -> Aabb {
        match self {
            Self::Box {
                half_extents,
                radius,
            } => transformed_box(transform, Vec3::ZERO, *half_extents, tolerance + radius),
            Self::Sphere { radius } => Aabb::from_center_half_extents(
                transform.translation,
                Vec3::splat(radius + tolerance),
            ),
            Self::Capsule { a, b, radius } => {
                let a = transform.transform_point(*a);
                let b = transform.transform_point(*b);
                let expand = Vec3::splat(radius + tolerance);
                Aabb {
                    min: a.min(b) - expand,
                    max: a.max(b) + expand,
                }
            }
            Self::Cylinder {
                a,
                b,
                radius,
                convex_radius,
            } => {
                let a = transform.transform_point(*a);
                let b = transform.transform_point(*b);
                let expand = Vec3::splat(radius + convex_radius + tolerance);
                Aabb {
                    min: a.min(b) - expand,
                    max: a.max(b) + expand,
                }
            }
            Self::ConvexVertices {
                vertices, radius, ..
            } => {
                let (min, max) = vertices.iter().fold(
                    (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                    |(min, max), vertex| (min.min(*vertex), max.max(*vertex)),
                );
                if vertices.is_empty() {
                    return Aabb::from_center_half_extents(transform.translation, Vec3::ZERO);
                }
                let center = (min + max) * 0.5;
                let half_extents = (max - min) * 0.5;
                transformed_box(transform, center, half_extents, tolerance + radius)
            }
            Self::Transform {
                transform: local,
                child,
            } => child.aabb(&(*transform * *local), tolerance),
            Self::List { children } => children
                .iter()
                .map(|child| child.aabb(transform, tolerance))
                .reduce(|a, b| a.union(&b))
                .unwrap_or_else(|| {
                    Aabb::from_center_half_extents(transform.translation, Vec3::ZERO)
                }),
            Self::HeightField(field) => field.aabb(transform, tolerance, field.height_range()),
        }
    }

    /// Returns the number of collision spheres (`getNumCollisionSpheres`), or 0 for a shape
    /// that is not convex.
    pub(crate) fn num_collision_spheres(&self) -> usize {
        match self {
            Self::Box { .. } => 8,
            Self::Sphere { .. } => 1,
            Self::Capsule { .. } => 2,
            Self::Cylinder { .. } => 18,
            Self::ConvexVertices { vertices, .. } => vertices.len(),
            Self::Transform { child, .. } if child.is_convex() => child.num_collision_spheres(),
            Self::Transform { .. } | Self::List { .. } | Self::HeightField(_) => 0,
        }
    }

    /// Appends the collision spheres of a convex shape to `out` (`getCollisionSpheres`).
    ///
    /// Each sphere has its centre in `xyz` and its radius in `w`, in the shape space. Shapes that
    /// are not convex add nothing.
    pub(crate) fn collision_spheres(&self, out: &mut Vec<Vec4>) {
        match self {
            Self::Box {
                half_extents,
                radius,
            } => out.extend(
                BOX_CORNER_SIGNS
                    .iter()
                    .map(|sign| (*sign * *half_extents).extend(*radius)),
            ),
            Self::Sphere { radius } => out.push(Vec4::new(0.0, 0.0, 0.0, *radius)),
            Self::Capsule { a, b, radius } => {
                out.push(a.extend(*radius));
                out.push(b.extend(*radius));
            }
            Self::Cylinder { a, b, radius, .. } => cylinder_spheres(*a, *b, *radius, out),
            Self::ConvexVertices {
                vertices, radius, ..
            } => out.extend(vertices.iter().map(|vertex| vertex.extend(*radius))),
            Self::Transform { transform, child } if child.is_convex() => {
                let start = out.len();
                child.collision_spheres(out);
                for sphere in &mut out[start..] {
                    let center = transform.transform_point(sphere.truncate());
                    *sphere = center.extend(sphere.w);
                }
            }
            Self::Transform { .. } | Self::List { .. } | Self::HeightField(_) => {}
        }
    }
}

/// Appends the 18 collision spheres of a cylinder (`0x140D910F0`).
///
/// The rim spheres have the radius of the cylinder, not the convex radius. On a cylinder that
/// is at least two radii long, the rim spheres move in to 0.8 of the radius and two axis
/// spheres with twice the radius cover the caps.
fn cylinder_spheres(a: Vec3, b: Vec3, radius: f32, out: &mut Vec<Vec4>) {
    let frame = CylinderFrame::new(a, b);
    let mut rim = radius;
    let mut axis_offset = 0.0;
    if (b - a).length_squared() >= radius * 4.0 * radius {
        rim = radius * CYLINDER_BASE_RADIUS_FACTOR;
        axis_offset = radius;
    }
    let p1 = frame.perpendicular1 * rim;
    let p2 = frame.perpendicular2 * rim;
    let diagonal = rim * FRAC_1_SQRT_2;
    let sum = (frame.perpendicular2 + frame.perpendicular1) * diagonal;
    let difference = (frame.perpendicular1 - frame.perpendicular2) * diagonal;
    for center in [a, b] {
        for offset in [p1, sum, p2, -difference, -p1, -sum, -p2, difference] {
            out.push((center + offset).extend(radius));
        }
    }
    let cap_radius = axis_offset + radius;
    out.push((a + frame.axis * axis_offset).extend(cap_radius));
    out.push((b - frame.axis * axis_offset).extend(cap_radius));
}
