//! Cleansing's synchronized aircraft impulse and air-intersection visual.

use super::super::common::create_power_visual;
use super::super::physics_helper::impulse_physics_air_units;
use crate::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;

/// The radius of the aircraft impulse around the beam (2008 `powercleansing.cpp:958`).
const IMPULSE_RADIUS: f32 = 10.0;
/// The chance in percent that one aircraft gets the impulse.
const IMPULSE_CHANCE: u32 = 20;
const AIR_IMPACT_RADIUS: f32 = 3.0;
const SHRUNK_BOUNDS_SCALE: f32 = 0.8;

/// `impulsePhysicsAirUnits(beam position, 10, 20)` on each damage tick.
pub(super) fn impulse_air_units(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    location: Vec3,
) {
    impulse_physics_air_units(world, gameplay, location, IMPULSE_RADIUS, IMPULSE_CHANCE);
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
