//! Weapon knockback (`BUnitActionRangedAttack_doKnockbackAttack` @ `0x14055DB00`, 2008
//! `unitactionrangedattack.cpp:4087-4137`).

use crate::entity_id::EntityId;
use crate::gameplay::KnockbackProfile;
use crate::world::World;
use glam::Vec3;

/// Above this upward body speed, the knockback adds no upward impulse.
const TOP_UP_VELOCITY: f32 = 6.0;
/// Above this upward body speed, the upward impulse goes down linearly to the top speed.
const MEDIUM_UP_VELOCITY: f32 = 3.0;
/// The degree factor of the retail code (`0.017453292`).
const RADIANS_PER_DEGREE: f32 = 0.017_453_292;

impl World {
    /// The knockback of an attack on a target unit that has a body and is not attached:
    /// `norm(target − attacker)` with `y = |y|`, times `cos(launch angle)` and `force · mass`.
    /// The launch angle (degrees) and then the force are drawn from the sim random stream.
    pub(super) fn do_knockback_attack(
        &mut self,
        attacker_id: EntityId,
        target_id: EntityId,
        profile: KnockbackProfile,
    ) {
        let Some(attacker_position) = self.units.get(attacker_id).map(|unit| unit.base.position)
        else {
            return;
        };
        let Some(target_position) = self
            .units
            .get(target_id)
            .filter(|unit| unit.object_state.attached_to().is_none())
            .map(|unit| unit.base.position)
        else {
            return;
        };
        let (Some(mass), Some(velocity)) = (
            self.unit_body_mass(target_id),
            self.unit_body_linear_velocity(target_id),
        ) else {
            return;
        };
        let degrees = self
            .sim_rng
            .range_float(profile.launch_angle_min, profile.launch_angle_max);
        let angle = degrees * RADIANS_PER_DEGREE;
        let force = self
            .sim_rng
            .range_float(profile.force_min, profile.force_max);
        let impulse = knockback_impulse(
            target_position - attacker_position,
            angle,
            force * mass,
            velocity.y,
        );
        let _applied = self.apply_unit_impulse(target_id, impulse);
    }
}

fn knockback_impulse(offset: Vec3, angle: f32, force: f32, upward_velocity: f32) -> Vec3 {
    let mut impulse = offset.normalize_or_zero();
    impulse.y = impulse.y.abs();
    impulse *= crate::math::cos(angle) * force;
    if upward_velocity > TOP_UP_VELOCITY {
        impulse.y = 0.0;
    } else if upward_velocity > MEDIUM_UP_VELOCITY {
        impulse.y *=
            1.0 - (upward_velocity - MEDIUM_UP_VELOCITY) / (TOP_UP_VELOCITY - MEDIUM_UP_VELOCITY);
    }
    impulse
}

#[cfg(test)]
mod tests;
