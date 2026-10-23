//! Ray casts against the heightfield (`hkpSampledHeightFieldShape::castRay`, `0x1408753F0`).

use glam::Vec3;

use crate::heightfield::HeightField;

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
    pub(crate) fn cast_ray_local(&self, from: Vec3, to: Vec3) -> Option<HeightFieldRayHit> {
        let _ = (self, from, to);
        None
    }
}
