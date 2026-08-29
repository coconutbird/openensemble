//! Shared configuration for scenario-layered ground-vehicle rigid bodies.

use super::Unit;
use crate::gameplay::{GroundVehicleKind, GroundVehiclePhysicsProfile};
use crate::physics::PhysicsBody;

// `BPhysicsGhostAction::calcMovement` applies cFwdK to the desired/current
// velocity difference. Our simpler deterministic controller expresses the
// same zero-velocity response as a maximum acceleration.
const GHOST_FORWARD_RESPONSE: f32 = 2.5;

pub(crate) fn configure_ground_vehicle_physics(
    unit: &mut Unit,
    profile: &GroundVehiclePhysicsProfile,
) {
    if profile.kind() != GroundVehicleKind::Ghost || unit.is_building() {
        return;
    }
    let acceleration = if unit.acceleration.is_finite() && unit.acceleration > 0.0 {
        unit.acceleration
    } else {
        unit.speed.max(0.0) * GHOST_FORWARD_RESPONSE
    };
    unit.acceleration = acceleration;
    unit.physics = Some(PhysicsBody::ground_vehicle(
        profile.material(),
        profile.collider(),
        unit.base.position.y,
        unit.speed,
        acceleration,
        unit.turn_rate_degrees,
    ));
}
