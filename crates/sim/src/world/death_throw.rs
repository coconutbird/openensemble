//! The physics deaths of `BUnit::damage` (DE `BUnit_damage`, 2008 `unit.cpp:5510-5655`): the
//! rocket death roll, the thrown physics replacement
//! (`BUnit_createAndThrowPhysicsReplacement`, `0x1404DEA30`) and the replacement of an
//! animation kill (`BUnit_killAndThrowPhysicsReplacement`, `0x1404E01B0`).

use super::World;
use crate::entity_id::EntityId;
use crate::gameplay::{DeathThrowProfile, GameplayCatalog};
use crate::math;
use crate::physics::ClamshellAnimation;
use crate::physics::death::{MIN_ROCKET_TIME_MS, RocketOnDeath};
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// The smallest squared length of the horizontal throw direction (`cFloatCompareEpsilon`).
const MIN_DIRECTION_LENGTH_SQUARED: f32 = 0.000_001;
/// The largest random part of the force and launch factors.
const FACTOR_RANDOM_MAX: f32 = 0.1;
/// The largest random lateral offset of the pelvis impulse of a clamshell.
const MAX_LATERAL_OFFSET: f32 = 0.04;
/// The largest random vertical offset of the pelvis impulse of a clamshell.
const MAX_VERTICAL_OFFSET: f32 = 0.8;
/// The pelvis impulse of a clamshell is 1.5 times the others (`xmmword_1411F4E90`).
const PELVIS_IMPULSE_SCALE: f32 = 1.5;
/// The extra distance beyond the obstruction radius at which the damage direction matters.
const FORCE_ANGLE_MIN_EXTRA_DISTANCE: f32 = 0.1;

/// The damage values that `BUnit::damage` reads for a physics death (`BDamage`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeathThrowContext {
    /// The throw values of the weapon (`mpDamageInfo`).
    pub(crate) profile: DeathThrowProfile,
    /// `mDamagePos`: the start of the projectile, the attacker or the area centre.
    pub(crate) damage_position: Vec3,
    /// `mDirection`: the damage direction (zero for an area target).
    pub(crate) direction: Vec3,
    /// `mDistanceFactor`: 1 for a direct hit, the area factor for an area target.
    pub(crate) distance_factor: f32,
}

impl World {
    /// The physics death of a unit that a weapon killed (`BUnit::damage` without
    /// `mForceThrow`). Returns the thrown replacement.
    ///
    /// The rocket death needs the `RocketOnDeath` flag and a successful roll of the death
    /// manager. The replacement is thrown if the weapon has `ThrowUnits` or the rocket death
    /// happens. A `PhysicsForceMaxAngle` above zero lowers the force by the angle between the
    /// damage direction and the direction from the damage position to the unit.
    pub(in crate::world) fn throw_killed_unit(
        &mut self,
        unit_id: EntityId,
        context: &DeathThrowContext,
        attacker_id: Option<EntityId>,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        let unit = self.units.get(unit_id)?;
        let rocket_on_death = gameplay.rocket_on_death(&unit.proto_object_name)
            && self.check_rocket_death_chance(gameplay);
        if !context.profile.throw_units && !rocket_on_death {
            return None;
        }
        let unit = self.units.get(unit_id)?;
        let force_scaler = force_scaler(
            context,
            unit.base.position,
            unit.obstruction_half_extents.x.abs(),
        );
        let throw_position = match attacker_id.and_then(|id| self.units.get(id)) {
            Some(attacker) if context.profile.launch_axial => normalized(attacker.base.velocity),
            _ => context.damage_position,
        };
        self.create_and_throw_physics_replacement(
            unit_id,
            &context.profile,
            ThrowSource {
                position: throw_position,
                distance_factor: context.distance_factor,
                force_scaler,
            },
            rocket_on_death,
            gameplay,
        )
    }

    /// `BDeathManager::checkDeathChanceSpecial(cDeathSpecialRocket)` (DE
    /// `BDeathManager_checkDeathChanceSpecialRocket`, `0x1401FDEA0`). A roll below the current
    /// chance succeeds and sets the chance to `MinChance`. A failed roll adds `ChanceGrowth` up
    /// to `MaxChance`. Without the data there is no rocket death and no roll.
    pub(crate) fn check_rocket_death_chance(&mut self, gameplay: &GameplayCatalog) -> bool {
        let Some(special) = gameplay.rocket_death_chance() else {
            return false;
        };
        let chance = self.rocket_death_chance.unwrap_or(special.max_chance);
        let roll = self.sim_rng.distribution();
        if chance > roll {
            self.rocket_death_chance = Some(special.min_chance);
            return true;
        }
        self.rocket_death_chance = Some((chance + special.chance_growth).min(special.max_chance));
        false
    }

    /// `BUnit::createAndThrowPhysicsReplacement`: replaces the unit with its
    /// `PhysicsReplacementInfo` body and throws it. Returns `None` for a garrisoned unit, a
    /// unit without a replacement info, or an info without `ThrownByProjectiles`.
    pub(crate) fn create_and_throw_physics_replacement(
        &mut self,
        unit_id: EntityId,
        profile: &DeathThrowProfile,
        source: ThrowSource,
        rocket_on_death: bool,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        let unit = self.units.get(unit_id)?;
        if unit.is_garrisoned() {
            return None;
        }
        let template =
            std::sync::Arc::clone(gameplay.physics_replacement(&unit.proto_object_name)?);
        if !template.info().thrown_by_projectiles() {
            return None;
        }
        let position = unit.base.position;
        let radius_y = unit.obstruction_half_extents.y.abs();
        let proto_object_name = unit.proto_object_name.clone();
        let replacement_id = self.spawn_physics_replacement(unit_id, &template)?;
        self.unit_body(replacement_id)?;
        if rocket_on_death {
            self.setup_rocket_on_death(
                replacement_id,
                gameplay.rocket_head_bone(&proto_object_name),
            );
        }
        let impulse = self.death_throw_impulse(profile, position, source);
        let animation = if rocket_on_death {
            ClamshellAnimation::ClamshellRocket
        } else if profile.flail_thrown_units {
            ClamshellAnimation::Flail
        } else {
            ClamshellAnimation::Clamshell
        };
        if self.unit_clamshell_bodies(replacement_id).is_some() {
            let offset_z = self
                .sim_rng
                .range_float(-MAX_LATERAL_OFFSET, MAX_LATERAL_OFFSET);
            let offset_y = self.sim_rng.range_float(0.0, MAX_VERTICAL_OFFSET);
            let offset_x = self
                .sim_rng
                .range_float(-MAX_LATERAL_OFFSET, MAX_LATERAL_OFFSET);
            self.apply_clamshell_point_impulses(
                replacement_id,
                impulse,
                Vec3::new(offset_x, offset_y, offset_z),
                animation,
            );
        } else {
            let point = position + Vec3::Y * (radius_y * 0.5);
            let _applied = self.apply_unit_point_impulse(replacement_id, impulse, point);
        }
        if let Some(replacement) = self.units.get_mut(replacement_id) {
            replacement.base.set_selectable(false);
            replacement.hitpoints = 0.0;
            replacement.kill();
        }
        Some(replacement_id)
    }

    /// The throw impulse: `force · (dir·cos a, sin a)` (2008 `unit.cpp:10519-10563`).
    fn death_throw_impulse(
        &mut self,
        profile: &DeathThrowProfile,
        position: Vec3,
        source: ThrowSource,
    ) -> Vec3 {
        let mut direction = if profile.launch_axial {
            source.position
        } else {
            position - source.position
        };
        direction.y = 0.0;
        if direction.length_squared() < MIN_DIRECTION_LENGTH_SQUARED {
            direction = Vec3::X;
        }
        let direction = normalized(direction);
        // `getWeaponPhysicsMultiplier` of the attacking player: the sim has no
        // `WeaponPhysicsMultiplier` tech effect, so it is 1.
        let player_multiplier = 1.0;
        let force_factor = (source.distance_factor
            + self.sim_rng.range_float(0.0, FACTOR_RANDOM_MAX))
        .clamp(0.0, 1.0);
        let force_min = profile.force_min * player_multiplier;
        let force_max = profile.force_max * player_multiplier;
        let force = ((force_max - force_min) * force_factor + force_min) * source.force_scaler;
        let launch_factor = (source.distance_factor
            + self.sim_rng.range_float(0.0, FACTOR_RANDOM_MAX))
        .clamp(0.0, 1.0);
        let degrees = profile.launch_angle_min
            + (profile.launch_angle_max - profile.launch_angle_min) * launch_factor;
        let angle = (degrees * PI) / 180.0;
        let mut launch = direction * math::cos(angle);
        launch.y = math::sin(angle);
        launch * force
    }

    /// `setupRocketOnDeath` (2008 `unitactionphysics.cpp:631-650`, DE inline in
    /// `0x1404DEA30`): the start time, a rotation that retail draws and does not use, and a
    /// duration of `750 + (rand >> 19 & 0xFFF)` milliseconds.
    fn setup_rocket_on_death(&mut self, unit_id: EntityId, head_bone: Option<Vec3>) {
        let _rotation = self.sim_rng.range_float(0.0, TAU);
        let duration_ms = MIN_ROCKET_TIME_MS + ((self.sim_rng.next_state() >> 19) & 0xFFF);
        let start_ms = self.game_time_ms;
        if let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        {
            state.death.rocket = Some(RocketOnDeath {
                start_ms,
                duration_ms,
                explode: true,
                head_bone,
            });
        }
    }

    /// `BClamshellPhysicsObject::applyPointImpulses(J, J, 1.5·J, offset)`: the pelvis gets
    /// `1.5·J` at its origin plus `offset`, the upper and lower bodies get `J` at their
    /// origins. The replacement plays `animation`.
    fn apply_clamshell_point_impulses(
        &mut self,
        unit_id: EntityId,
        impulse: Vec3,
        pelvis_offset: Vec3,
        animation: ClamshellAnimation,
    ) {
        let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        else {
            return;
        };
        state.death.clamshell_animation = Some(animation);
        let pelvis = state.body;
        let Some([upper, lower]) = state.clamshell_bodies() else {
            return;
        };
        let world = self.physics.world_mut();
        for (body, impulse, offset) in [
            (pelvis, impulse * PELVIS_IMPULSE_SCALE, pelvis_offset),
            (upper, impulse, Vec3::ZERO),
            (lower, impulse, Vec3::ZERO),
        ] {
            let origin = world.position(body).unwrap_or(Vec3::ZERO);
            world.apply_point_impulse(body, impulse, origin + offset);
        }
    }

    fn unit_clamshell_bodies(&self, unit_id: EntityId) -> Option<[::physics::BodyId; 2]> {
        self.units
            .get(unit_id)?
            .physics_object
            .as_ref()?
            .clamshell_bodies()
    }
}

/// Where a throw comes from (`createAndThrowPhysicsReplacement` arguments).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ThrowSource {
    /// `damagePos`: the damage position, or the throw direction for `PhysicsLaunchAxial`.
    pub(crate) position: Vec3,
    /// `distanceFactor`.
    pub(crate) distance_factor: f32,
    /// `forceScaler`.
    pub(crate) force_scaler: f32,
}

/// `forceScaler` (2008 `unit.cpp:5600-5630`, DE `0x1404D5D97`-`0x1404D5F0B`): with a positive
/// `PhysicsForceMaxAngle` and a unit at least its obstruction radius plus 0.1 from the damage
/// position, `1 - (angle - maxAngle) / π` for an angle above the maximum, at least 0.
fn force_scaler(context: &DeathThrowContext, unit_position: Vec3, radius_x: f32) -> f32 {
    let max_angle = context.profile.force_max_angle;
    if max_angle <= 0.0 {
        return 1.0;
    }
    let mut direction = context.direction;
    direction.y = 0.0;
    let direction = normalized(direction);
    let to_target = unit_position - context.damage_position;
    if to_target.length() < radius_x + FORCE_ANGLE_MIN_EXTRA_DISTANCE {
        return 1.0;
    }
    let angle = math::acos(direction.dot(normalized(to_target)));
    let excess = angle.abs() - max_angle;
    if excess > 0.0 {
        (1.0 - excess / PI).max(0.0)
    } else {
        1.0
    }
}

/// `XMVector3Normalize`: a zero vector stays zero.
fn normalized(value: Vec3) -> Vec3 {
    let length = value.length();
    if length == 0.0 {
        Vec3::ZERO
    } else {
        value / length
    }
}

#[cfg(test)]
mod tests {
    use super::World;
    use crate::gameplay::{DeathChance, GameplayCatalog};
    use pipeline::database::hw1::Database;

    #[test]
    fn the_rocket_death_chance_resets_to_the_minimum_and_grows_after_a_failure() {
        let mut gameplay = GameplayCatalog::from_tactics(&Database::new(), []);
        gameplay.set_test_rocket_death(
            "grunt",
            Some(DeathChance {
                max_chance: 1.5,
                min_chance: -0.06,
                chance_growth: 0.06,
            }),
        );
        assert!(gameplay.rocket_on_death("Grunt"));
        let mut world = World::new();
        assert!(world.check_rocket_death_chance(&gameplay));
        assert_eq!(world.rocket_death_chance, Some(-0.06));
        assert!(!world.check_rocket_death_chance(&gameplay));
        assert!(world.rocket_death_chance.unwrap().abs() < 1.0e-6);
    }

    #[test]
    fn without_death_manager_data_there_is_no_roll() {
        let gameplay = GameplayCatalog::from_tactics(&Database::new(), []);
        let mut world = World::new();
        let before = world.sim_rng;
        assert!(!world.check_rocket_death_chance(&gameplay));
        assert_eq!(world.sim_rng, before);
    }
}
