//! Cleansing's synchronized aircraft impulse and air-intersection visual.

use super::super::common::create_power_visual;
use crate::EntityId;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;

const IMPULSE_RADIUS: f32 = 10.0;
const IMPULSE_CHANCE_MAXIMUM: u32 = 100;
const IMPULSE_CHANCE: u32 = 20;
const MAXIMUM_IMPULSE: f32 = 150.0;
const MINIMUM_FALLOFF: f32 = 0.75;
const HEALTH_IMPULSE_MULTIPLIER: f32 = 2.0;
const POINT_IMPULSE_MULTIPLIER: f32 = 2.5;
const AIR_IMPACT_RADIUS: f32 = 3.0;
const SHRUNK_BOUNDS_SCALE: f32 = 0.8;

pub(super) fn impulse_air_units(world: &mut World, location: Vec3) {
    let unit_ids = world
        .find_live_squads(None, None, None, Some((location, IMPULSE_RADIUS)))
        .into_iter()
        .flat_map(|squad_id| {
            world
                .get_squad(squad_id)
                .map_or_else(Vec::new, |squad| squad.unit_ids.clone())
        })
        .collect::<Vec<_>>();
    for unit_id in unit_ids {
        if world.trigger_random_index(IMPULSE_CHANCE_MAXIMUM) > IMPULSE_CHANCE {
            continue;
        }
        impulse_air_unit(world, unit_id, location);
    }
}

fn impulse_air_unit(world: &mut World, unit_id: EntityId, location: Vec3) {
    let Some((position, health_ratio)) = world.get_unit(unit_id).and_then(|unit| {
        (unit.flying && unit.physics.is_some()).then(|| {
            let ratio = if unit.max_hitpoints > 0.0 {
                (unit.hitpoints / unit.max_hitpoints).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (unit.base.position, ratio)
        })
    }) else {
        return;
    };
    let mut difference = location - position;
    difference.y = 0.0;
    let direction = difference.normalize_or_zero();
    let impulse_direction = direction - Vec3::Y;
    let health_multiplier =
        HEALTH_IMPULSE_MULTIPLIER + (1.0 - HEALTH_IMPULSE_MULTIPLIER) * health_ratio;
    let distance_ratio = if IMPULSE_RADIUS > f32::EPSILON {
        difference.length() / IMPULSE_RADIUS
    } else {
        0.0
    };
    let falloff = MINIMUM_FALLOFF + (1.0 - MINIMUM_FALLOFF) * (1.0 - 0.5 * distance_ratio);
    let impulse = impulse_direction * MAXIMUM_IMPULSE * health_multiplier * falloff;
    if let Some(unit) = world.get_unit_mut(unit_id) {
        let _center = unit.apply_impulse(impulse);
        let _point =
            unit.apply_impulse_at_point(impulse * POINT_IMPULSE_MULTIPLIER, position + direction);
    }
}

pub(super) fn update_air_impact(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    prototype: Option<&str>,
    position: Vec3,
    current_id: EntityId,
) -> EntityId {
    let Some(prototype) = prototype else {
        return current_id;
    };
    let Some(height) = air_impact_height(world, position) else {
        if !current_id.is_invalid() {
            let _removed = world.remove_object(current_id);
        }
        return EntityId::INVALID;
    };
    let impact_position = Vec3::new(position.x, height, position.z);
    let object_id = if world.get_object(current_id).is_some() {
        current_id
    } else {
        create_power_visual(
            world,
            database,
            player_id,
            impact_position,
            Vec3::Z,
            prototype,
        )
    };
    if let Some(object) = world.get_object_mut(object_id) {
        object.base.set_position(impact_position);
    }
    object_id
}

fn air_impact_height(world: &World, position: Vec3) -> Option<f32> {
    let mut height = position.y;
    let mut needed = false;
    for squad_id in world.find_live_squads(None, None, None, Some((position, AIR_IMPACT_RADIUS))) {
        let Some(unit) = world.get_squad(squad_id).and_then(|squad| {
            squad
                .unit_ids
                .iter()
                .find_map(|unit_id| world.get_unit(*unit_id))
        }) else {
            continue;
        };
        if !unit.flying || unit.base.position.y <= height {
            continue;
        }
        let (center, half_extents) = unit.simulation_bounds();
        let half_extents = half_extents * SHRUNK_BOUNDS_SCALE;
        if (position.x - center.x).abs() > half_extents.x
            || (position.z - center.z).abs() > half_extents.z
        {
            continue;
        }
        height = center.y + half_extents.y * 0.6;
        needed = true;
    }
    needed.then_some(height)
}
