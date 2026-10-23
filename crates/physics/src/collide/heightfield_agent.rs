//! The agent of a convex shape against the terrain (`hkpHeightFieldAgent`).
//!
//! Retail: `hkpHeightFieldAgent_processCollision` (`0x140D9D6C0`). For each collision sphere of
//! the convex shape, in sphere order:
//!
//! 1. Move the sphere into the heightfield space and measure it (`collideSpheres`).
//! 2. If the distance is above the tolerance, remove the point of the sphere.
//! 3. Otherwise put the point on the sphere surface, `centre − radius · normal`, with the
//!    triangle normal. Add the point if the sphere has none, or else update it.
//!
//! So each sphere has at most one point, and the point keeps its key while the sphere stays
//! within the tolerance. The terrain is body B: the normal points from the terrain toward the
//! convex body.

use glam::Vec4;

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::contact::ContactPoint;
use crate::heightfield::HeightField;
use crate::math::Transform;
use crate::shape::Shape;

/// The heightfield agent: one optional point key for each collision sphere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeightFieldAgent {
    point_keys: Vec<Option<u32>>,
}

impl HeightFieldAgent {
    /// Makes the agent for a convex shape.
    pub(crate) fn new(convex: &Shape) -> Self {
        Self {
            point_keys: vec![None; convex.num_collision_spheres()],
        }
    }

    /// Updates the points of the convex body against the terrain.
    pub(crate) fn process(
        &mut self,
        convex: &CdBody<'_>,
        field_transform: Transform,
        field: &HeightField,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        let mut spheres = Vec::with_capacity(self.point_keys.len());
        convex.shape.collision_spheres(&mut spheres);
        let to_field = field_transform.inverse() * convex.transform;
        for sphere in &mut spheres {
            *sphere = to_field.transform_point(sphere.truncate()).extend(sphere.w);
        }
        let mut results: Vec<Vec4> = Vec::with_capacity(spheres.len());
        field.collide_spheres(&spheres, &mut results);
        for ((key, sphere), result) in self.point_keys.iter_mut().zip(&spheres).zip(&results) {
            if result.w > input.tolerance {
                if let Some(old) = key.take() {
                    sink.remove_point(old);
                }
                continue;
            }
            let normal = result.truncate();
            let position = normal * -sphere.w + sphere.truncate();
            let contact = ContactPoint {
                position: field_transform.transform_point(position),
                normal: field_transform.transform_vector(normal),
                distance: result.w,
            };
            let kept = key.is_some_and(|old| sink.update_point(old, contact));
            if !kept {
                *key = sink.add_point(contact);
            }
        }
    }
}
