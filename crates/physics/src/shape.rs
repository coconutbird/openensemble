//! Collision shapes.

mod query;

use std::sync::Arc;

use glam::{Vec3, Vec4};

use crate::heightfield::HeightField;
use crate::math::Transform;

pub(crate) use query::Aabb;

/// The convex radius that Havok uses by default (`hkConvexShapeDefaultRadius`).
///
/// Retail gives this radius to every box and every convex hull that it builds.
pub const DEFAULT_CONVEX_RADIUS: f32 = 0.05;

/// A collision shape in the local space of its body.
///
/// The shapes are the Havok 5.1 shapes that Halo Wars DE builds from `.shp` files, blueprints,
/// debris parts and the terrain. Compound shapes share their children through `Arc`, because
/// many bodies use the same shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `hkpBoxShape`: a box centred on the origin.
    ///
    /// The collision surface is the box with `half_extents`, expanded by `radius`.
    Box {
        /// Half the size of the box on each local axis, without the radius.
        half_extents: Vec3,
        /// The convex radius around the box.
        radius: f32,
    },
    /// `hkpSphereShape`: a sphere centred on the origin.
    Sphere {
        /// The radius of the sphere.
        radius: f32,
    },
    /// `hkpCapsuleShape`: the points within `radius` of the segment from `a` to `b`.
    Capsule {
        /// The first end of the segment.
        a: Vec3,
        /// The second end of the segment.
        b: Vec3,
        /// The radius around the segment.
        radius: f32,
    },
    /// `hkpCylinderShape`: a cylinder from `a` to `b`, expanded by `convex_radius`.
    Cylinder {
        /// The centre of the first cap.
        a: Vec3,
        /// The centre of the second cap.
        b: Vec3,
        /// The radius of the cylinder, without the convex radius.
        radius: f32,
        /// The convex radius around the cylinder (the Havok padding radius).
        convex_radius: f32,
    },
    /// `hkpConvexVerticesShape`: the convex hull of `vertices`, expanded by `radius`.
    ConvexVertices {
        /// The hull vertices.
        vertices: Vec<Vec3>,
        /// The face planes: the outward normal in `xyz` and the plane offset in `w`, so that a
        /// point `p` on the plane gives `normal.dot(p) + w == 0`.
        planes: Vec<Vec4>,
        /// The convex radius around the hull.
        radius: f32,
    },
    /// `hkpTransformShape`: a child shape moved by a rigid transform.
    Transform {
        /// The transform from the child space to the parent space.
        transform: Transform,
        /// The child shape.
        child: Arc<Shape>,
    },
    /// `hkpListShape`: a list of child shapes in the same space.
    List {
        /// The child shapes in authored order.
        children: Vec<Arc<Shape>>,
    },
    /// `hkpSampledHeightFieldShape`: the terrain. Only a fixed body can use this shape.
    HeightField(Arc<HeightField>),
}
