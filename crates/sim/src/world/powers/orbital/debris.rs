//! The rocks thrown by an Orbital projectile impact (`BPowerUserOrbital` impact debris,
//! `0x14031A3D0`; 2008 `powerorbital.cpp:1042-1135`).
//!
//! Retail creates a unit of the rock proto object at the impact and runs action 0 of impact
//! point 0 of the damage template of its visual with the random impulse and direction. The rock
//! visuals have one `ThrowPart` action with all the rock meshes, so the visible rock is the
//! thrown part (`BUnit::createAndThrow`): an object with the part body, the impulse
//! `computeImpulseForce(direction, impulse · ForceMultiplier)` at the rotated part centre, and a
//! life of `DEFAULT_THROW_PART_LIFETIME` (10 seconds). The rock unit keeps no visible mesh; the
//! sim does not make it.

use super::super::common::create_power_visual;
use super::super::manager::PowerDebris;
use super::OrbitalPowerExecution;
use crate::entities::squads::formation_offset_to_world;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;

/// `DEFAULT_THROW_PART_LIFETIME` (2008 `damageaction.cpp:43`).
const LIFETIME_MS: u32 = 10_000;
const LARGE_IMPULSE: f32 = 650.0;
const MEDIUM_IMPULSE: f32 = 500.0;
const SMALL_IMPULSE: f32 = 350.0;
const LARGE_Y_RANGE: f32 = 0.05;
const MEDIUM_Y_RANGE: f32 = 0.1;
const SMALL_Y_RANGE: f32 = 0.15;
/// `computeImpulseForce`: the random scale of each axis of the direction.
const IMPULSE_SCALE_MIN: f32 = 0.6;
const IMPULSE_SCALE_MAX: f32 = 1.4;
/// The rocks start this high above the impact.
const SPAWN_HEIGHT: f32 = 2.0;

#[derive(Clone, Copy)]
struct DebrisSpawn<'name> {
    player_id: PlayerId,
    prototype: &'name str,
    impact_position: Vec3,
    projectile_direction: Vec3,
    maximum_impulse: f32,
    y_range: f32,
}

pub(super) fn spawn_impact(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &OrbitalPowerExecution,
    position: Vec3,
    direction: Vec3,
) {
    let large = random_inclusive(world, 2, 4);
    let medium = random_inclusive(world, 4, 7);
    let small = random_inclusive(world, 7, 12);
    for index in 0..large + medium + small {
        // Retail compares the index with the medium count, not with the large plus medium count,
        // so the medium rocks are the indices from the large count to the medium count.
        let (prototype, maximum_impulse, y_range) = if index < large {
            (
                execution.rock_large_prototype.as_str(),
                LARGE_IMPULSE,
                LARGE_Y_RANGE,
            )
        } else if index < medium {
            (
                execution.rock_medium_prototype.as_str(),
                MEDIUM_IMPULSE,
                MEDIUM_Y_RANGE,
            )
        } else {
            (
                execution.rock_small_prototype.as_str(),
                SMALL_IMPULSE,
                SMALL_Y_RANGE,
            )
        };
        spawn_rock(
            world,
            database,
            gameplay,
            DebrisSpawn {
                player_id: execution.player_id,
                prototype,
                impact_position: position,
                projectile_direction: direction,
                maximum_impulse,
                y_range,
            },
        );
    }
}

fn spawn_rock(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    request: DebrisSpawn<'_>,
) {
    // The arguments of the direction are drawn right to left (z, y, x), as the retail build
    // evaluates them (`0x14031A800`).
    let z = world.trigger_random_float(-1.0, 1.0);
    let y = world.trigger_random_float(1.0, 1.0 + request.y_range);
    let x = world.trigger_random_float(-1.0, 1.0);
    let direction = Vec3::new(x, y, z).normalize_or_zero();
    let impulse =
        world.trigger_random_float(request.maximum_impulse * 0.5, request.maximum_impulse);
    let Some(part) = gameplay
        .and_then(|gameplay| gameplay.damage_parts(request.prototype))
        .and_then(|parts| parts.impact_point(0))
        .cloned()
    else {
        return;
    };
    let forward = request.projectile_direction.normalize_or(-Vec3::Y);
    let position = request.impact_position + Vec3::Y * SPAWN_HEIGHT;
    let object_id = create_power_visual(
        world,
        database,
        request.player_id,
        position,
        forward,
        request.prototype,
    );
    if object_id.is_invalid() {
        return;
    }
    if !world.create_physics_object(object_id, part.template(), false) {
        let _removed = world.remove_object(object_id);
        return;
    }
    let force = impulse_force(world, direction, impulse * part.force_multiplier());
    let point = position + formation_offset_to_world(forward, part.throw_center_offset());
    apply_object_point_impulse(world, object_id, force, point);
    world.power_manager.debris.push(PowerDebris {
        object_id,
        expires_at_ms: world.game_time_ms.wrapping_add(LIFETIME_MS),
    });
}

/// `computeImpulseForce` of a ballistic hit with a direction that is not zero: each axis of the
/// direction times a random scale, plus `Y`, times the force.
fn impulse_force(world: &mut World, direction: Vec3, force: f32) -> Vec3 {
    let scale = Vec3::new(
        world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
        world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
        world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
    );
    (direction * scale + Vec3::Y) * force
}

fn apply_object_point_impulse(
    world: &mut World,
    object_id: crate::EntityId,
    force: Vec3,
    point: Vec3,
) {
    let Some(body) = world
        .physics_state(object_id)
        .map(crate::physics::UnitPhysics::body)
    else {
        return;
    };
    world
        .physics
        .world_mut()
        .apply_point_impulse(body, force, point);
}

/// Removes each thrown rock at the end of its life.
pub(super) fn update(world: &mut World) {
    let current_time = world.game_time_ms;
    let debris = std::mem::take(&mut world.power_manager.debris);
    for entry in debris {
        if world.get_object(entry.object_id).is_none() {
            continue;
        }
        if current_time >= entry.expires_at_ms {
            let _removed = world.remove_object(entry.object_id);
            continue;
        }
        world.power_manager.debris.push(entry);
    }
}

fn random_inclusive(world: &mut World, minimum: u32, maximum: u32) -> u32 {
    minimum + world.trigger_random_index(maximum.saturating_sub(minimum))
}

#[cfg(test)]
mod tests;
