//! New contact points: the material, the contact listener and the first impulse estimate
//! (`hkpSimpleConstraintContactMgr_addContactPoint`, `0x14088C150`, and
//! `hkpSimpleContactConstraint_processNewPoints_restitution`, `0x14089C8F0`).

use std::collections::BTreeMap;

use glam::Vec3;

use super::material::{decode_restitution, mix_friction, mix_restitution};
use super::{StepInfo, decode_ufloat8};
use crate::body::{Body, BodyId};
use crate::contact::{Manifold, ManifoldPoint};
use crate::world::{ContactAdded, ContactListener};

/// The added mass that keeps the impulse estimate finite between two fixed bodies.
const ESTIMATE_EPSILON: f32 = 1.0e-10;

/// The share of the approach velocity that the first impulse estimate removes.
const ESTIMATE_FACTOR: f32 = -0.2;

/// Processes the new points of every manifold in manifold order, then point order.
///
/// For each new point this mixes the material, reports the point to the listener and removes
/// it if the listener rejects it. A point without restitution gets its warm-start impulse
/// now; a point with restitution gets it at the start of the next solve, with the restitution
/// bias.
pub(crate) fn process_new_points(
    bodies: &BTreeMap<BodyId, Body>,
    manifolds: &mut [Manifold],
    listener: &mut dyn ContactListener,
) {
    for manifold in manifolds {
        let (Some(a), Some(b)) = (bodies.get(&manifold.body_a), bodies.get(&manifold.body_b))
        else {
            continue;
        };
        let mut index = 0;
        while index < manifold.points.len() {
            let point = manifold.points[index];
            if !point.properties.is_new {
                index += 1;
                continue;
            }
            let friction_code = mix_friction(a.friction, b.friction);
            let restitution_code = mix_restitution(a.restitution, b.restitution);
            let projected_velocity = projected_velocity(&point, a, b);
            let response = listener.contact_point_added(&ContactAdded {
                manifold: manifold.id,
                body_a: manifold.body_a,
                body_b: manifold.body_b,
                key: point.key,
                contact: point.contact,
                projected_velocity,
            });
            if !response.accept {
                manifold.remove_point(index);
                continue;
            }
            if let Some(scale) = response.inv_mass_scale {
                manifold.inv_mass_scale = scale;
            }
            let properties = &mut manifold.points[index].properties;
            properties.is_new = false;
            properties.friction_code = friction_code;
            properties.friction = decode_ufloat8(friction_code);
            properties.restitution_code = restitution_code;
            properties.restitution = decode_restitution(restitution_code);
            if restitution_code == 0 {
                properties.impulse_applied = impulse_estimate(a, b) * projected_velocity;
                properties.internal_solver_data = 0.0;
                properties.internal_data_a = point.contact.distance;
            } else {
                properties.restitution_pending = true;
            }
            index += 1;
        }
    }
}

/// Applies the restitution bias to the pending points of a manifold, with the body
/// velocities at the start of the solve.
pub(crate) fn apply_pending_restitution(
    manifold: &mut Manifold,
    a: &Body,
    b: &Body,
    step: &StepInfo,
) {
    for point in &mut manifold.points {
        if !point.properties.restitution_pending {
            continue;
        }
        let velocity = projected_velocity(point, a, b);
        let restitution = decode_restitution(point.properties.restitution_code);
        // Retail runs `solveSingleContact` instead when the approach is faster than
        // `m_contactRestingVelocity` and the restitution is above 0.3. The world keeps the
        // retail value `HK_REAL_MAX`, so that branch never runs.
        let properties = &mut point.properties;
        properties.impulse_applied = impulse_estimate(a, b) * velocity * (restitution + 1.0);
        let bias = restitution * velocity * step.substep_delta_time * -1.3;
        properties.internal_solver_data = bias;
        properties.internal_data_a = bias + point.contact.distance;
        properties.restitution_pending = false;
    }
}

/// `(1 / (imB + imA + 1e-10)) · −0.2`.
fn impulse_estimate(a: &Body, b: &Body) -> f32 {
    (1.0 / (b.inverse_mass() + a.inverse_mass() + ESTIMATE_EPSILON)) * ESTIMATE_FACTOR
}

/// The velocity of A relative to B along the normal at the point. Negative means approaching.
fn projected_velocity(point: &ManifoldPoint, a: &Body, b: &Body) -> f32 {
    let position = point.contact.position;
    let relative: Vec3 = a.point_velocity(position) - b.point_velocity(position);
    let value = relative * point.contact.normal;
    value.z + (value.y + value.x)
}
