//! The agents of primitive shape pairs that retail registers with a dedicated agent.
//!
//! `hkpAgentRegisterUtil::_registerTerminalAgents` (`0x140D94830`) registers four Havok 5.1
//! agent2 agents for the shape pairs that Halo Wars uses. The world runs each one through the
//! agent3 bridge:
//!
//! - `hkpSphereSphereAgent` (`processCollision` `0x140D9DCC0`): one point.
//! - `hkpSphereCapsuleAgent` (`0x140D9F220`): one point.
//! - `hkpSphereBoxAgent` (`0x140DA24B0`): one point.
//! - `hkpCapsuleCapsuleAgent` (`0x140DA0D10`): up to three points.
//!
//! Each agent keeps one contact point id for each point slot (`m_contactPointId`). In each step
//! the agent computes the point of a slot again. A slot outside the tolerance loses its point.
//! A slot inside the tolerance keeps its point, or adds a new point if it has none. These
//! agents have no negative create distance: they make a point as soon as it is within
//! `CollisionInput::tolerance`.

mod capsule;
mod sphere;
#[cfg(test)]
mod tests;

use glam::{Mat3, Vec3};

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::contact::ContactPoint;
use crate::shape::Shape;

/// The persistent state of a primitive agent: the key of the point in each slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveAgent {
    /// A sphere (A) against a sphere (B).
    SphereSphere(Option<u32>),
    /// A sphere (A) against a capsule (B).
    SphereCapsule(Option<u32>),
    /// A sphere (A) against a box (B).
    SphereBox(Option<u32>),
    /// A capsule (A) against a capsule (B), with three point slots.
    CapsuleCapsule([Option<u32>; 3]),
}

impl PrimitiveAgent {
    /// Makes the agent for two primitive shapes, with no points.
    ///
    /// # Panics
    ///
    /// Panics if retail has no dedicated agent for the pair in this order. The valid pairs are
    /// sphere–sphere, sphere–capsule, sphere–box and capsule–capsule.
    pub(crate) fn new(a: &Shape, b: &Shape) -> Self {
        match (a, b) {
            (Shape::Sphere { .. }, Shape::Sphere { .. }) => Self::SphereSphere(None),
            (Shape::Sphere { .. }, Shape::Capsule { .. }) => Self::SphereCapsule(None),
            (Shape::Sphere { .. }, Shape::Box { .. }) => Self::SphereBox(None),
            (Shape::Capsule { .. }, Shape::Capsule { .. }) => Self::CapsuleCapsule([None; 3]),
            _ => panic!("no primitive agent for this shape pair"),
        }
    }

    /// Updates the points of two primitive shapes (`processCollision`).
    ///
    /// The shapes of `a` and `b` must be the shapes that made the agent. The agent does nothing
    /// for other shapes.
    pub(crate) fn process(
        &mut self,
        a: &CdBody<'_>,
        b: &CdBody<'_>,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        let tolerance = input.tolerance;
        match (self, a.shape, b.shape) {
            (
                Self::SphereSphere(key),
                Shape::Sphere { radius: radius_a },
                Shape::Sphere { radius: radius_b },
            ) => {
                let contact = sphere::sphere_sphere(
                    a.transform.translation,
                    *radius_a,
                    b.transform.translation,
                    *radius_b,
                    tolerance,
                );
                update_slot(key, contact, sink);
            }
            (
                Self::SphereCapsule(key),
                Shape::Sphere {
                    radius: sphere_radius,
                },
                Shape::Capsule {
                    a: start,
                    b: end,
                    radius: capsule_radius,
                },
            ) => {
                let rotation = b.transform.rotation;
                let translation = b.transform.translation;
                let segment = [
                    sphere_capsule_vertex(rotation, translation, *start),
                    sphere_capsule_vertex(rotation, translation, *end),
                ];
                let contact = sphere::sphere_capsule(
                    a.transform.translation,
                    *sphere_radius,
                    segment,
                    *capsule_radius,
                    tolerance,
                );
                update_slot(key, contact, sink);
            }
            (
                Self::SphereBox(key),
                Shape::Sphere {
                    radius: sphere_radius,
                },
                Shape::Box {
                    half_extents,
                    radius: box_radius,
                },
            ) => {
                let contact = sphere::sphere_box(
                    a.transform.translation,
                    *sphere_radius,
                    &b.transform,
                    *half_extents,
                    *box_radius,
                    tolerance,
                );
                update_slot(key, contact, sink);
            }
            (
                Self::CapsuleCapsule(keys),
                Shape::Capsule {
                    a: start_a,
                    b: end_a,
                    radius: radius_a,
                },
                Shape::Capsule {
                    a: start_b,
                    b: end_b,
                    radius: radius_b,
                },
            ) => {
                let segment_a = capsule_segment(a, *start_a, *end_a);
                let segment_b = capsule_segment(b, *start_b, *end_b);
                let points =
                    capsule::capsule_capsule(segment_a, *radius_a, segment_b, *radius_b, tolerance);
                // The agent loop at `0x140DA0D10`: a slot loses its point when
                // `tolerance <= distance`.
                for (key, point) in keys.iter_mut().zip(points) {
                    let contact = (point.distance < tolerance).then_some(point);
                    update_slot(key, contact, sink);
                }
            }
            _ => {}
        }
    }
}

/// Writes the result of one point slot to the sink.
///
/// `None` removes the point of the slot. `Some` updates the point, or adds it if the slot has
/// no point or a contact listener removed it. A rejected add (`0xFFFF` in retail) leaves the
/// slot empty, so the agent tries again in the next step.
fn update_slot(key: &mut Option<u32>, contact: Option<ContactPoint>, sink: &mut ContactSink<'_>) {
    match contact {
        Some(contact) => match *key {
            Some(existing) if sink.update_point(existing, contact) => {}
            _ => *key = sink.add_point(contact),
        },
        None => {
            if let Some(existing) = key.take() {
                sink.remove_point(existing);
            }
        }
    }
}

/// Moves a capsule vertex to world space in the order of `0x140D9F220`:
/// `(x·c0 + y·c1) + (z·c2 + t)`.
fn sphere_capsule_vertex(rotation: Mat3, translation: Vec3, vertex: Vec3) -> Vec3 {
    (rotation.x_axis * vertex.x + rotation.y_axis * vertex.y)
        + (rotation.z_axis * vertex.z + translation)
}

/// Moves the two capsule vertices of a body to world space in the order of `0x140DA0D10`:
/// `(y·c1 + x·c0) + (z·c2 + t)`.
fn capsule_segment(body: &CdBody<'_>, start: Vec3, end: Vec3) -> [Vec3; 2] {
    let rotation = body.transform.rotation;
    let translation = body.transform.translation;
    let transform = |vertex: Vec3| {
        (rotation.y_axis * vertex.y + rotation.x_axis * vertex.x)
            + (rotation.z_axis * vertex.z + translation)
    };
    [transform(start), transform(end)]
}

/// Returns a vector that is perpendicular to `direction` (the inlined helper in `0x140D9F220`
/// and `0x140DD79B0`).
///
/// The helper takes the two components that are not the smallest one, swaps them and negates
/// one. The result is not normalized.
fn perpendicular(direction: Vec3) -> Vec3 {
    let components = direction.to_array();
    let magnitudes = direction.abs().to_array();
    let (mut first, mut smallest_xy, mut smallest) = (1, 0, magnitudes[0]);
    if magnitudes[0] > magnitudes[1] {
        first = 0;
        smallest_xy = 1;
        smallest = magnitudes[1];
    }
    let second = if smallest > magnitudes[2] {
        smallest_xy
    } else {
        2
    };
    let mut result = [0.0; 3];
    result[first] = components[second];
    result[second] = -components[first];
    Vec3::from_array(result)
}

/// Returns `vector / |vector|`, the normalization that retail does with `rsqrtss` and one
/// Newton step.
fn normalize(vector: Vec3) -> Vec3 {
    vector * (1.0 / vector.dot(vector).sqrt())
}
