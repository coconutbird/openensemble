//! The reorient step of the vehicle actions (`hkpReorientAction::applyAction`, copied into each
//! action: ground `0x1402D05E0`, warthog `0x1402D8B50`, ghost `0x1402CDC60`, hover
//! `0x1402D29C0`).

use glam::Vec3;
use physics::{BodyId, World as PhysicsWorld};

use super::frame::{BodyFrame, hk_acos, major_axis_component};

/// Turns the body toward world up about one axis.
///
/// `body_axis` is the rotation axis in body space. The impulse is
/// `strength · angle / dt · axis − damping / dt · ω`, where `angle` is the signed angle between
/// the body up axis and world up projected onto the plane normal to the axis.
pub(super) fn reorient(
    physics: &mut PhysicsWorld,
    body: BodyId,
    dt: f32,
    body_axis: Vec3,
    strength: f32,
    damping: f32,
) {
    let Some(frame) = BodyFrame::read(physics, body) else {
        return;
    };
    let inv_dt = 1.0 / dt;
    let body_up = frame.up();
    let rotation_axis = frame.matrix * body_axis;
    let distance = Vec3::Y.dot(rotation_axis);
    let projected_up = (Vec3::Y - rotation_axis * distance).normalize();
    let mut angle = hk_acos(body_up.dot(projected_up));
    let calculated_axis = body_up.cross(projected_up);
    if (major_axis_component(calculated_axis) < 0.0) != (major_axis_component(rotation_axis) < 0.0)
    {
        angle = -angle;
    }
    let damping_term = frame.angular_velocity * (damping * inv_dt);
    let impulse = rotation_axis * (strength * angle * inv_dt) - damping_term;
    physics.apply_angular_impulse(body, impulse);
}
