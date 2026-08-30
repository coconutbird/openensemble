//! Synchronized debris thrown by an Orbital projectile impact.

use super::super::common::create_power_visual;
use super::super::manager::PowerDebris;
use super::OrbitalPowerExecution;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;

const LIFETIME_MS: u32 = 10_000;
const GRAVITY: f32 = 9.81;
const RETAIL_FALLBACK_MASS: f32 = 100.0;
const RESTITUTION: f32 = 0.5;
const GROUND_FRICTION: f32 = 0.8;
const REST_SPEED: f32 = 0.5;
const LARGE_IMPULSE: f32 = 650.0;
const MEDIUM_IMPULSE: f32 = 500.0;
const SMALL_IMPULSE: f32 = 350.0;
const LARGE_Y_RANGE: f32 = 0.05;
const MEDIUM_Y_RANGE: f32 = 0.1;
const SMALL_Y_RANGE: f32 = 0.15;
const IMPULSE_SCALE_MIN: f32 = 0.6;
const IMPULSE_SCALE_MAX: f32 = 1.4;
const OFFSET: f32 = 0.1;

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
    execution: &OrbitalPowerExecution,
    position: Vec3,
    direction: Vec3,
) {
    let counts = [
        random_inclusive(world, 2, 4),
        random_inclusive(world, 4, 7),
        random_inclusive(world, 7, 12),
    ];
    let categories = [
        (
            execution.rock_large_prototype.as_str(),
            counts[0],
            LARGE_IMPULSE,
            LARGE_Y_RANGE,
        ),
        (
            execution.rock_medium_prototype.as_str(),
            counts[1],
            MEDIUM_IMPULSE,
            MEDIUM_Y_RANGE,
        ),
        (
            execution.rock_small_prototype.as_str(),
            counts[2],
            SMALL_IMPULSE,
            SMALL_Y_RANGE,
        ),
    ];
    for (prototype, count, maximum_impulse, y_range) in categories {
        for _ in 0..count {
            spawn_object(
                world,
                database,
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
}

fn spawn_object(world: &mut World, database: &Database, request: DebrisSpawn<'_>) {
    let direction = Vec3::new(
        world.trigger_random_float(-1.0, 1.0),
        world.trigger_random_float(1.0, 1.0 + request.y_range),
        world.trigger_random_float(-1.0, 1.0),
    )
    .normalize_or(Vec3::Y);
    let impulse =
        world.trigger_random_float(request.maximum_impulse * 0.5, request.maximum_impulse);
    let randomized = direction
        * Vec3::new(
            world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
            world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
            world.trigger_random_float(IMPULSE_SCALE_MIN, IMPULSE_SCALE_MAX),
        )
        + Vec3::Y;
    let local_offset = Vec3::new(
        world.trigger_random_float(-OFFSET, OFFSET),
        world.trigger_random_float(-OFFSET, OFFSET),
        world.trigger_random_float(-OFFSET, OFFSET),
    );
    let forward = request.projectile_direction.normalize_or(-Vec3::Y);
    let right = Vec3::Y.cross(forward).normalize_or(Vec3::X);
    let up = forward.cross(right).normalize_or(Vec3::Y);
    let position = request.impact_position
        + Vec3::Y * 2.0
        + right * local_offset.x
        + up * local_offset.y
        + forward * local_offset.z;
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
    // Retail ThrowPart falls back to mass 100 when no damage blueprint is present.
    // Damage-template blueprints are visual assets not yet decoded by the simulation.
    let velocity = randomized * (impulse / RETAIL_FALLBACK_MASS);
    if let Some(object) = world.get_object_mut(object_id) {
        object.base.velocity = velocity;
    }
    world.power_manager.debris.push(PowerDebris {
        object_id,
        velocity,
        expires_at_ms: world.game_time_ms.wrapping_add(LIFETIME_MS),
    });
}

pub(super) fn update(world: &mut World, dt: f32) {
    let current_time = world.game_time_ms;
    let mut debris = std::mem::take(&mut world.power_manager.debris);
    let mut remaining = Vec::with_capacity(debris.len());
    for mut entry in debris.drain(..) {
        let Some(mut position) = world
            .get_object(entry.object_id)
            .map(|object| object.base.position)
        else {
            continue;
        };
        if current_time >= entry.expires_at_ms {
            let _removed = world.remove_object(entry.object_id);
            continue;
        }
        entry.velocity.y -= GRAVITY * dt;
        position += entry.velocity * dt;
        settle_on_terrain(world, &mut position, &mut entry.velocity);
        if let Some(object) = world.get_object_mut(entry.object_id) {
            object.base.set_position(position);
            object.base.velocity = entry.velocity;
        }
        remaining.push(entry);
    }
    world.power_manager.debris = remaining;
}

fn settle_on_terrain(world: &World, position: &mut Vec3, velocity: &mut Vec3) {
    let Some(height) = world.terrain_height(*position, true) else {
        return;
    };
    if position.y > height || velocity.y >= 0.0 {
        return;
    }
    position.y = height;
    velocity.y = -velocity.y * RESTITUTION;
    velocity.x *= GROUND_FRICTION;
    velocity.z *= GROUND_FRICTION;
    if velocity.y.abs() < REST_SPEED {
        velocity.y = 0.0;
    }
}

fn random_inclusive(world: &mut World, minimum: u32, maximum: u32) -> u32 {
    minimum + world.trigger_random_index(maximum.saturating_sub(minimum))
}
