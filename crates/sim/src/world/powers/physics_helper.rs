//! The physics impulses of `BPowerHelper` on aircraft (2008 `powerhelper.cpp:292-384`).

use glam::Vec3;

use crate::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::world::World;

/// `cMaxImpulseMult`.
const MAXIMUM_IMPULSE: f32 = 150.0;
/// `cMinimumFalloffPercent`.
const MINIMUM_FALLOFF: f32 = 0.75;
/// `cHealthImpulseMult`.
const HEALTH_IMPULSE_MULTIPLIER: f32 = 2.0;
/// `cPointImpulseMult` (`xmmword_1411F4F20`).
const POINT_IMPULSE_MULTIPLIER: f32 = 2.5;
/// `cFloatCompareEpsilon`: a smaller radius gives no falloff.
const MINIMUM_RADIUS: f32 = 0.000_001;

/// The arguments of one `impulsePhysicsAirUnit` call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::world) struct AirImpulse {
    /// `impulseScale`.
    pub(in crate::world) scale: f32,
    /// `impulseTarget`: the point that the aircraft is pushed toward and down from. `None`
    /// picks a random point below the aircraft.
    pub(in crate::world) target: Option<Vec3>,
    /// `radius`: the impulse falls off to 75 % at twice this distance from the target.
    pub(in crate::world) radius: f32,
}

/// `BUnit_isPhysicsAircraft` (`0x1404E55B0`): the `PhysicsInfo` of the proto object has an
/// aircraft vehicle type (hawk to sentinel), the movement type is air and the unit has a body.
pub(in crate::world) fn is_physics_aircraft(
    world: &World,
    gameplay: Option<&GameplayCatalog>,
    unit_id: EntityId,
) -> bool {
    let Some(unit) = world.get_unit(unit_id) else {
        return false;
    };
    unit.flying
        && unit.physics_object.is_some()
        && gameplay
            .and_then(|gameplay| gameplay.object_physics(&unit.proto_object_name))
            .and_then(|physics| physics.info.as_ref())
            .is_some_and(|info| info.vehicle_type().is_aircraft())
}

/// `PowerHelper_impulsePhysicsAirUnit` (`0x140312F50`, 2008 `powerhelper.cpp:326-384`): pushes a
/// physics aircraft toward the target and down.
///
/// `d = xz(target − position)`, `J = (normalize(d) − Y) · (2 − hp/hpMax) · 150 · scale`, times
/// `0.75 + 0.25 · (1 − 0.5·|d|/radius)` for a radius. `applyImpulse(J)`, then
/// `applyPointImpulse(2.5·J, position + normalize(d))`. A target straight above or below gives
/// a NaN direction, which the DE impulse wrappers replace with zero. Returns `false` for a unit
/// that is not a physics aircraft.
pub(in crate::world) fn impulse_physics_air_unit(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    unit_id: EntityId,
    impulse: AirImpulse,
) -> bool {
    if !is_physics_aircraft(world, gameplay, unit_id) {
        return false;
    }
    let Some((position, radius_x, radius_z, health)) = world.get_unit(unit_id).map(|unit| {
        (
            unit.base.position,
            unit.obstruction_half_extents.x,
            unit.obstruction_half_extents.z,
            unit.hitpoints / unit.max_hitpoints,
        )
    }) else {
        return false;
    };
    let target = impulse.target.unwrap_or_else(|| {
        let x = world.trigger_random_float(-0.5, 0.5) * radius_x;
        let z = world.trigger_random_float(-0.5, 0.5) * radius_z;
        position + Vec3::new(x, -radius_x.max(radius_z), z)
    });
    let mut difference = target - position;
    difference.y = 0.0;
    let direction = difference / difference.length();
    // `Lerp(2, 1, hp/hpMax)`, which DE folds to `2 − hp/hpMax`.
    let mut multiplier = (HEALTH_IMPULSE_MULTIPLIER - health) * MAXIMUM_IMPULSE * impulse.scale;
    if impulse.radius > MINIMUM_RADIUS {
        let falloff = 1.0 - difference.length() * 0.5 / impulse.radius;
        multiplier *= falloff * (1.0 - MINIMUM_FALLOFF) + MINIMUM_FALLOFF;
    }
    let linear = (direction - Vec3::Y) * multiplier;
    let _linear = world.apply_unit_impulse(unit_id, linear);
    let _point = world.apply_unit_point_impulse(
        unit_id,
        linear * POINT_IMPULSE_MULTIPLIER,
        position + direction,
    );
    true
}

/// `PowerHelper_impulsePhysicsAirUnits` (`0x140312D10`, 2008 `powerhelper.cpp:292-323`): each
/// unit of the squads in the radius gets the impulse toward `location` with a chance of
/// `percent_chance` in `0..=100`.
pub(in crate::world) fn impulse_physics_air_units(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    location: Vec3,
    radius: f32,
    percent_chance: u32,
) {
    let unit_ids = world
        .find_live_squads(None, None, None, Some((location, radius)))
        .into_iter()
        .flat_map(|squad_id| {
            world
                .get_squad(squad_id)
                .map_or_else(Vec::new, |squad| squad.unit_ids.clone())
        })
        .collect::<Vec<_>>();
    for unit_id in unit_ids {
        if world.trigger_random_index(100) > percent_chance {
            continue;
        }
        let _impulsed = impulse_physics_air_unit(
            world,
            gameplay,
            unit_id,
            AirImpulse {
                scale: 1.0,
                target: Some(location),
                radius,
            },
        );
    }
}

/// `(normalize(dir.xz), 1)`: the nudge direction of Rage and Carpet Bombing. A vertical `dir`
/// gives NaN, which the DE impulse wrapper replaces with zero.
pub(in crate::world) fn nudge_direction(direction: Vec3) -> Vec3 {
    let mut planar = Vec3::new(direction.x, 0.0, direction.z);
    planar /= planar.length();
    planar.y = 1.0;
    planar
}

#[cfg(test)]
mod tests;
