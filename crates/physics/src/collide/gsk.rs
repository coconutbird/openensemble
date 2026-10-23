//! The convex–convex agent: GSK closest points and the GSK manifold.
//!
//! Retail collides each pair of bodies through the agent stream of the pair (the NN track of
//! `hkpAgentNnMachine`). For two convex shapes that stream runs `hkPredGskAgent3`
//! (`hkPredGskAgent3::registerAgent3`, `0x140DC5200`, process `0x140DC4B80`). A pair with a bare
//! cylinder runs `hkPredGskCylinderAgent3` (registered at `0x140DC5DF0`, process
//! `0x140DC5410`). The older `hkpPredGskfAgent` (`0x140D94F60`) only runs under an agent of the
//! older kind.
//!
//! One collide step of a pair, as the stream machine (`0x1408A3A80`) and the agent run it:
//!
//! 1. On the first step the separating distance is unknown, so the agent runs the full query.
//! 2. Else the separating distance of the last step drops by the motion of the bodies along the
//!    separating normal (the time of impact estimate, TIM).
//! 3. If the estimate is at least the tolerance, all points are removed and nothing else runs.
//! 4. If the estimate is above `manifold_tim_distance`, only the existing points are rebuilt
//!    from their vertex ids (the manifold TIM). No point is added.
//! 5. Else the full query runs (`hkGskAgentUtil_processCollisionNoTim`, `0x140DD92E0`): GSK finds
//!    the closest point, the manifold rebuilds its points, and the closest point is kept or
//!    added.
//!
//! The full query adds a new closest point only below a create distance: `create_4d_contact`
//! for a vertex–face or edge–edge feature (four vertices), else `create_contact`.

mod cache;
mod manifold;
mod penetration;
mod simplex;
mod solver;
#[cfg(test)]
mod tests;

use glam::{Mat3, Vec3};

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::convex::{ConvexRef, dot, rotate};
use crate::collide::sink::ContactSink;
use crate::contact::ContactPoint;
use crate::math::Transform;
use crate::shape::Shape;

use cache::GskCache;
use manifold::{Frame, GskManifold};

/// The cylinder flag of shape A (`GSK_FLAGS_SHAPE_A_IS_CYLINDER`).
const SHAPE_A_IS_CYLINDER: u8 = 0x01;

/// The flag that puts a capsule in place of cylinder A
/// (`GSK_FLAGS_REPLACE_SHAPE_A_WITH_CAPSULE`). The flag of B is one bit higher.
const REPLACE_A_WITH_CAPSULE: u8 = 0x04;

/// A capsule replaces a cylinder again when `|n · axis|` is below this value (`0x1411F3C1C`).
const CAPSULE_ON_COSINE: f32 = 0.05;

/// A capsule can turn back into the cylinder above this `|n · axis|` (`0x1411F3C44`).
const CAPSULE_OFF_COSINE: f32 = 0.1;

/// The capsule test distance above which a capsule replaces the cylinder (`0x1411F3BCC`).
const CAPSULE_ON_DISTANCE: f32 = 0.01;

/// The extra radius of the replacement capsule (`0x1411F3BA8`).
const CAPSULE_EXTRA_RADIUS: f32 = 0.002;

/// The persistent state of one convex pair: the GSK cache, the manifold and the separating
/// plane of the last step.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GskAgent {
    /// `true` if shape A is a bare cylinder, `false`/`true` for B (the cylinder agent).
    cylinder: [bool; 2],
    cache: Option<GskCache>,
    manifold: GskManifold,
    /// The world separating normal and distance of the last step, `None` before the first.
    separating: Option<(Vec3, f32)>,
    /// The body transforms of the last step, for the motion estimate.
    previous: Option<(Transform, Transform)>,
}

impl GskAgent {
    /// Makes the agent for two convex shapes.
    pub(crate) fn new(a: &Shape, b: &Shape) -> Self {
        Self {
            cylinder: [
                matches!(a, Shape::Cylinder { .. }),
                matches!(b, Shape::Cylinder { .. }),
            ],
            ..Self::default()
        }
    }

    /// Updates the points of two convex shapes.
    pub(crate) fn process(
        &mut self,
        a: &CdBody<'_>,
        b: &CdBody<'_>,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        let shape_a = ConvexRef::Shape(a.shape);
        let shape_b = ConvexRef::Shape(b.shape);
        let cache = self.cache.get_or_insert_with(|| {
            let b_to_a = a.transform.inverse() * b.transform;
            let mut cache = GskCache::new(&shape_a, &shape_b, &b_to_a);
            if matches!(a.shape, Shape::Cylinder { .. }) {
                cache.flags |= SHAPE_A_IS_CYLINDER | REPLACE_A_WITH_CAPSULE;
            }
            if matches!(b.shape, Shape::Cylinder { .. }) {
                cache.flags |= (SHAPE_A_IS_CYLINDER | REPLACE_A_WITH_CAPSULE) << 1;
            }
            cache
        });
        let estimate = self.separating.map(|(normal, distance)| {
            let motion = self.previous.map_or(0.0, |(old_a, old_b)| {
                motion_bound(normal, a, &old_a, b, &old_b)
            });
            distance - motion
        });
        self.previous = Some((a.transform, b.transform));
        if let Some(estimate) = estimate
            && estimate >= input.tolerance
        {
            if let Some((normal, _)) = self.separating {
                self.separating = Some((normal, estimate));
            }
            self.manifold.clear(sink);
            return;
        }
        let is_cylinder_pair = self.cylinder[0] || self.cylinder[1];
        let fresh_normal = if is_cylinder_pair {
            Some(update_cylinder_flags(cache, &mut self.manifold, a, b, sink))
        } else {
            None
        };
        let capsule_a = capsule_replacement(a.shape, cache.flags, 0);
        let capsule_b = capsule_replacement(b.shape, cache.flags, 1);
        let agent_a = capsule_a.unwrap_or(shape_a);
        let agent_b = capsule_b.unwrap_or(shape_b);
        match estimate {
            Some(estimate) if estimate > input.quality.manifold_tim_distance => {
                let normal = self.separating.map_or(Vec3::Y, |(n, _)| n);
                self.separating = Some((normal, estimate));
                let frame = Frame {
                    a: agent_a,
                    b: agent_b,
                    world_a: a.transform,
                    world_b: b.transform,
                    normal,
                    tolerance: input.tolerance,
                };
                let kept = self.manifold.verify(0, &frame, sink);
                write_points(&mut self.manifold, &kept, sink);
            }
            _ => {
                // The cylinder agent runs GSK on a copy of the cache when a capsule replaces a
                // cylinder; the real cache keeps the cylinder features.
                let mut work = *cache;
                if capsule_a.is_some() || capsule_b.is_some() {
                    convert_cache_to_capsules(&mut work, capsule_a.is_some(), capsule_b.is_some());
                }
                let separating = full_query(
                    &mut work,
                    &mut self.manifold,
                    (agent_a, agent_b),
                    (a.transform, b.transform),
                    input,
                    sink,
                );
                if capsule_a.is_none() && capsule_b.is_none() {
                    *cache = work;
                }
                self.separating = Some(fresh_normal.unwrap_or(separating));
            }
        }
    }
}

/// Runs the full query of one step (`hkGskAgentUtil_processCollisionNoTim`, `0x140DD92E0`).
/// Returns the new separating normal and distance.
fn full_query(
    cache: &mut GskCache,
    manifold: &mut GskManifold,
    shapes: (ConvexRef<'_>, ConvexRef<'_>),
    transforms: (Transform, Transform),
    input: &CollisionInput,
    sink: &mut ContactSink<'_>,
) -> (Vec3, f32) {
    let (shape_a, shape_b) = shapes;
    let (world_a, world_b) = transforms;
    let closest = cache.closest_point(shape_a, shape_b, &world_a, &world_b, input.tolerance);
    let Some(position) = closest.position else {
        manifold.clear(sink);
        return (closest.normal, closest.distance);
    };
    let found = manifold.find(cache);
    let frame = Frame {
        a: shape_a,
        b: shape_b,
        world_a,
        world_b,
        normal: closest.normal,
        tolerance: input.tolerance,
    };
    let mut kept = manifold.verify(usize::from(found), &frame, sink);
    let contact = ContactPoint {
        position,
        normal: closest.normal,
        distance: closest.distance,
    };
    if found {
        if let Some(first) = manifold.points.first() {
            kept.push((first.key, contact));
        }
    } else {
        let create = if cache.dim_a + cache.dim_b == 4 {
            input.quality.create_4d_contact
        } else {
            input.quality.create_contact
        };
        if closest.distance < create {
            manifold.add(cache, contact, &mut kept, sink);
        }
    }
    write_points(manifold, &kept, sink);
    (closest.normal, closest.distance)
}

/// Writes the new geometry of the kept points (the contact manager `processContact`). A point
/// that a listener removed is added again with a new key.
fn write_points(
    manifold: &mut GskManifold,
    kept: &[(u32, ContactPoint)],
    sink: &mut ContactSink<'_>,
) {
    for &(key, contact) in kept {
        if sink.update_point(key, contact) {
            continue;
        }
        let Some(index) = manifold.points.iter().position(|p| p.key == key) else {
            continue;
        };
        match sink.add_point(contact) {
            Some(new_key) => manifold.points[index].key = new_key,
            None => {
                manifold.points.swap_remove(index);
            }
        }
    }
}

/// Returns the decrease of the separating distance by the motion since the last step (the TIM
/// of `0x1408A3A80`): `n · (ΔB - ΔA)` plus the rotation term of each body.
///
/// Retail uses the centre of mass motion and `|Δangle| · objectRadius` of the rigid motion.
/// The collision world has no motion data, so this port uses the origin of each body and the
/// bound `‖R1 - R0‖ · r`, where `r` is half the diagonal of the shape box.
fn motion_bound(
    normal: Vec3,
    a: &CdBody<'_>,
    old_a: &Transform,
    b: &CdBody<'_>,
    old_b: &Transform,
) -> f32 {
    let linear = (b.transform.translation - old_b.translation)
        - (a.transform.translation - old_a.translation);
    let angular = rotation_bound(a, old_a) + rotation_bound(b, old_b);
    let product = normal * linear;
    (product.z + angular) + (product.y + product.x)
}

fn rotation_bound(body: &CdBody<'_>, old: &Transform) -> f32 {
    let change: Mat3 = body.transform.rotation - old.rotation;
    let columns = [change.x_axis, change.y_axis, change.z_axis];
    let norm = columns.iter().map(|c| dot(*c, *c)).sum::<f32>().sqrt();
    if norm == 0.0 {
        return 0.0;
    }
    let bounds = body.shape.aabb(&Transform::IDENTITY, 0.0);
    let extent = bounds.max - bounds.min;
    norm * (0.5 * dot(extent, extent).sqrt())
}

/// Returns the capsule that replaces cylinder `side` (0 for A, 1 for B), if its flag is set.
fn capsule_replacement(shape: &Shape, flags: u8, side: u8) -> Option<ConvexRef<'static>> {
    let Shape::Cylinder {
        a,
        b,
        radius,
        convex_radius,
    } = shape
    else {
        return None;
    };
    (flags & (REPLACE_A_WITH_CAPSULE << side) != 0).then(|| ConvexRef::Capsule {
        a: *a,
        b: *b,
        radius: (radius + convex_radius) + CAPSULE_EXTRA_RADIUS,
    })
}

/// Turns the cylinder vertex ids of the cache copy into capsule ids (`0x140DC5C09`).
fn convert_cache_to_capsules(cache: &mut GskCache, side_a: bool, side_b: bool) {
    let capsule_id = |id: u16| (1 - ((id >> 7) & 1)) << 4;
    if side_a {
        cache.vertices[0] = capsule_id(cache.vertices[0]);
        if cache.dim_a > 1 {
            cache.vertices[0] = 16;
            cache.vertices[1] = 0;
            if cache.dim_a == 3 {
                cache.dim_a = 2;
                cache.vertices[2] = cache.vertices[3];
            }
        }
    }
    if side_b {
        let first = usize::from(cache.dim_a);
        cache.vertices[first] = capsule_id(cache.vertices[first]);
        if cache.dim_b > 1 {
            cache.vertices[first] = 16;
            cache.vertices[first + 1] = 0;
            if cache.dim_b == 3 {
                cache.dim_b = 2;
            }
        }
    }
}

/// Updates the capsule flags of the cylinder agent (`0x140DC5410`) and returns the separating
/// normal of the real shapes.
///
/// For each cylinder, `c = |n · axis|` with the world axis. A capsule replaces the cylinder if
/// `c < 0.05`, or if the capsule test distance is above `0.01`. A capsule turns back into the
/// cylinder if `c > 0.1` and the capsule test distance is negative. Each change removes all
/// points, because the vertex ids change their meaning.
fn update_cylinder_flags(
    cache: &mut GskCache,
    manifold: &mut GskManifold,
    a: &CdBody<'_>,
    b: &CdBody<'_>,
    sink: &mut ContactSink<'_>,
) -> (Vec3, f32) {
    let separating = cache.separating_normal(
        ConvexRef::Shape(a.shape),
        ConvexRef::Shape(b.shape),
        &a.transform,
        &b.transform,
    );
    for (side, body) in [a, b].into_iter().enumerate() {
        let Shape::Cylinder {
            a: start, b: end, ..
        } = body.shape
        else {
            continue;
        };
        let shift = u8::try_from(side).unwrap_or(0);
        let axis = rotate(&body.transform, (*end - *start).normalize_or_zero());
        let cosine = dot(separating.0, axis).abs();
        let flag = REPLACE_A_WITH_CAPSULE << shift;
        let replaced = cache.flags & flag != 0;
        let change = if replaced {
            cosine > CAPSULE_OFF_COSINE && capsule_test_distance(cache, a, b) < 0.0
        } else {
            cosine < CAPSULE_ON_COSINE || capsule_test_distance(cache, a, b) > CAPSULE_ON_DISTANCE
        };
        if change {
            cache.flags ^= flag;
            manifold.clear(sink);
        }
    }
    separating
}

/// Returns the surface distance with each cylinder replaced by a capsule of the cylinder radius
/// (`0x140DC5EB0`). The query starts from a new cache.
fn capsule_test_distance(cache: &GskCache, a: &CdBody<'_>, b: &CdBody<'_>) -> f32 {
    let replace = |shape: &Shape, flag: u8| match shape {
        Shape::Cylinder {
            a: start,
            b: end,
            radius,
            ..
        } if cache.flags & flag != 0 => Some(ConvexRef::Capsule {
            a: *start,
            b: *end,
            radius: *radius,
        }),
        _ => None,
    };
    let shape_a = replace(a.shape, SHAPE_A_IS_CYLINDER).unwrap_or(ConvexRef::Shape(a.shape));
    let shape_b = replace(b.shape, SHAPE_A_IS_CYLINDER << 1).unwrap_or(ConvexRef::Shape(b.shape));
    let b_to_a = a.transform.inverse() * b.transform;
    let mut test = GskCache::new(&shape_a, &shape_b, &b_to_a);
    test.separating_normal(shape_a, shape_b, &a.transform, &b.transform)
        .1
}
