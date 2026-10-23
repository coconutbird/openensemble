//! The body motion of a Ram hit (`BUnitActionCollisionAttack_handleCollisions` @ `0x1400F1FB0`,
//! 2008 `UnitActionCollisionAttack.cpp:600-780`).
//!
//! - A bowlable target that lives is thrown (`BUnit::throwUnit` @ `0x1404EA690`, with the rammer
//!   as the thrower, no thrower action and a velocity scalar of 1).
//! - A rammable target that lives and has a physics-controlled body gets the rammed impulse.
//! - An overrun action bounces the rammer up when it bowls a unit and moves fast enough.
//! - After a rammable hit, the rammer body stops, gets the rammer impulse back, and its squad
//!   moves behind the rammer.

use super::{CollisionDisposition, RamTargetSnapshot, RammerSnapshot};
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, OverrunTuning};
use crate::math;
use crate::world::World;
use glam::Vec3;

const RAMMER_IMPULSE_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const RAMMER_IMPULSE_FORCE: f32 = 10.0;
const RAMMED_IMPULSE_ANGLE: f32 = std::f32::consts::FRAC_PI_2 * 0.75;
const RAMMED_IMPULSE_FORCE: f32 = 7.0;
const RAMMED_MEDIUM_UP_VELOCITY: f32 = 3.0;
const RAMMED_TOP_UP_VELOCITY: f32 = 6.0;
/// `BUnitActionThrown::impulseUnit` without a thrower action: the direction turns 45 degrees off
/// the thrower velocity, the launch angle is 30 degrees and the speed is 10.
const THROW_TURN_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const THROW_LAUNCH_ANGLE: f32 = std::f32::consts::FRAC_PI_6;
const THROW_SPEED: f32 = 10.0;

impl World {
    pub(super) fn apply_rammed_target_motion(
        &mut self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        direction: Vec3,
        disposition: CollisionDisposition,
        gameplay: &GameplayCatalog,
    ) {
        match disposition {
            CollisionDisposition::Bowlable => self.throw_bowled_target(attacker, target, gameplay),
            CollisionDisposition::Rammable => {
                if !self.has_physics_controlled_body(target.id) {
                    return;
                }
                let (Some(mass), Some(velocity)) = (
                    self.unit_body_mass(target.id),
                    self.unit_body_linear_velocity(target.id),
                ) else {
                    return;
                };
                let impulse = rammed_impulse(direction, mass, velocity.y);
                let _applied = self.apply_unit_impulse(target.id, impulse);
            }
        }
    }

    fn throw_bowled_target(
        &mut self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) {
        if !self.can_be_thrown(target.id) {
            return;
        }
        let Some(release_physics) = self.ensure_throw_physics(Some(gameplay), target.id) else {
            return;
        };
        let velocity = bowled_throw_velocity(attacker.position, attacker.velocity, target.position);
        let _thrown = self.begin_unit_throw(target.id, attacker.id, velocity, release_physics);
    }

    /// The overrun bounce: `(0, OverrunJumpForce, 0)` on the rammer body when its speed is above
    /// `OverrunMinVel`.
    pub(super) fn apply_overrun_bounce(&mut self, attacker_id: EntityId, tuning: OverrunTuning) {
        let Some(velocity) = self.unit_body_linear_velocity(attacker_id) else {
            return;
        };
        if velocity.length() > tuning.minimum_speed {
            let _applied =
                self.apply_unit_impulse(attacker_id, Vec3::new(0.0, tuning.jump_force, 0.0));
        }
    }

    /// The rammer after a rammable hit. A physics-controlled body loses its velocities and gets
    /// `(-dir·cos45°, sin45°)·10·m`; the squad goes to `unit − dir·obstructionRadius`. Without such
    /// a body, the squad goes to the unit position.
    pub(super) fn push_rammer_back(
        &mut self,
        attacker_id: EntityId,
        squad_id: EntityId,
        direction: Vec3,
    ) {
        let Some((position, radius)) = self
            .units
            .get(attacker_id)
            .map(|unit| (unit.base.position, unit.obstruction_radius()))
        else {
            return;
        };
        let mut squad_position = position;
        if self.has_physics_controlled_body(attacker_id)
            && let Some(mass) = self.unit_body_mass(attacker_id)
        {
            let _linear = self.set_unit_body_linear_velocity(attacker_id, Vec3::ZERO);
            let _angular = self.set_unit_body_angular_velocity(attacker_id, Vec3::ZERO);
            let impulse =
                angled_impulse(-direction, RAMMER_IMPULSE_ANGLE, RAMMER_IMPULSE_FORCE, mass);
            let _applied = self.apply_unit_impulse(attacker_id, impulse);
            squad_position -= direction * radius;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.set_position(squad_position);
            squad.set_leash_position(squad_position, true);
        }
    }

    /// `mFlagPhysicsControl`: the unit has a body that is not fixed.
    fn has_physics_controlled_body(&self, unit_id: EntityId) -> bool {
        self.units
            .get(unit_id)
            .is_some_and(crate::entities::Unit::is_physics_driven)
    }
}

/// `BUnitActionThrown::impulseUnit` with a thrower and no thrower action. The direction is the
/// planar thrower velocity turned 45 degrees toward the side of the target; with no thrower
/// velocity, the unit goes straight up. The sim has no pather, so the landing point is not moved
/// to a passable tile.
fn bowled_throw_velocity(thrower: Vec3, thrower_velocity: Vec3, unit: Vec3) -> Vec3 {
    let to_unit = Vec3::new(unit.x - thrower.x, 0.0, unit.z - thrower.z).normalize_or_zero();
    let heading = Vec3::new(thrower_velocity.x, 0.0, thrower_velocity.z).normalize_or_zero();
    let turn = if heading.cross(to_unit).y >= 0.0 {
        THROW_TURN_ANGLE
    } else {
        -THROW_TURN_ANGLE
    };
    let direction = (math::quat_from_rotation_y(turn) * heading).normalize_or_zero();
    if direction == Vec3::ZERO {
        return Vec3::Y * THROW_SPEED;
    }
    let (sin, cos) = math::sin_cos(THROW_LAUNCH_ANGLE);
    Vec3::new(direction.x * cos, sin, direction.z * cos) * THROW_SPEED
}

fn rammed_impulse(direction: Vec3, mass: f32, upward_velocity: f32) -> Vec3 {
    let mut impulse = angled_impulse(direction, RAMMED_IMPULSE_ANGLE, RAMMED_IMPULSE_FORCE, mass);
    if upward_velocity > RAMMED_TOP_UP_VELOCITY {
        impulse.y = 0.0;
    } else if upward_velocity > RAMMED_MEDIUM_UP_VELOCITY {
        let span = RAMMED_TOP_UP_VELOCITY - RAMMED_MEDIUM_UP_VELOCITY;
        impulse.y *= 1.0 - (upward_velocity - RAMMED_MEDIUM_UP_VELOCITY) / span;
    }
    impulse
}

fn angled_impulse(direction: Vec3, angle: f32, force: f32, mass: f32) -> Vec3 {
    let (sin, cos) = math::sin_cos(angle);
    let mut impulse = direction * cos;
    impulse.y = sin;
    impulse * (force * mass.max(0.0))
}

#[cfg(test)]
mod tests;
