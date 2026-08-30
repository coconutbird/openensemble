//! Retail trigger squad fly-in planning and synthetic carrier creation.

use crate::EntityId;
use crate::entities::squads::SquadTransportPlan;
use crate::scenario::placed::create_trigger_unit_squad;
use crate::spawn::object_prototype_id;
use crate::world::{
    TransportGroupRequest, World, plan_transport_groups, transport_carrier_spacing,
};
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::{Database, ProtoObject};

#[derive(Debug, Clone)]
pub(super) struct TriggerFlyInRequest {
    pub(super) passenger_squad_ids: Vec<EntityId>,
    pub(super) player_id: u8,
    pub(super) dropoff_position: Vec3,
    pub(super) fly_in_start: Option<Vec3>,
    pub(super) fly_off_end: Option<Vec3>,
    pub(super) facing: Option<Vec3>,
    pub(super) rally_point: Option<Vec3>,
    pub(super) attack_move: bool,
}

pub(super) fn trigger_fly_in(
    world: &mut World,
    database: &Database,
    request: &TriggerFlyInRequest,
) -> bool {
    if request.fly_in_start.is_none() && request.fly_off_end.is_none() {
        return false;
    }
    let Some(profile) = trigger_transport_profile(world, database, request.player_id) else {
        return false;
    };
    let formation_center = request
        .fly_in_start
        .filter(|start| !positions_are_equal(*start, request.dropoff_position))
        .unwrap_or(request.dropoff_position);
    let groups = plan_transport_groups(
        world,
        database,
        TransportGroupRequest {
            passenger_squad_ids: &request.passenger_squad_ids,
            formation_center,
            dropoff_center: request.dropoff_position,
            count_accepted_object_types: profile.count_contains,
            count_capacity: profile.count_capacity,
            load_accepted_object_types: profile.load_contains,
            load_capacity: profile.load_capacity,
            maximum_transports: profile.maximum_transports,
            carrier_spacing: profile.carrier_spacing,
        },
    );
    if groups.is_empty() {
        return true;
    }

    let mut carrier_ids = Vec::with_capacity(groups.len());
    let mut flights = Vec::with_capacity(groups.len());
    for group in groups {
        let direction =
            horizontal_direction(group.dropoff_position - group.pickup_position).unwrap_or(Vec3::Z);
        let start_position = group.pickup_position - direction * profile.settings.incoming_offset
            + Vec3::Y * profile.settings.incoming_height;
        let incoming_target = group.dropoff_position + Vec3::Y * profile.settings.dropoff_height;
        let outgoing_target = outgoing_target(request, &profile, group.dropoff_position, direction);
        let Some((carrier_id, carrier_unit_id)) = create_trigger_unit_squad(
            world,
            database,
            request.player_id,
            profile.trigger_prototype_id,
            start_position,
            direction,
            true,
        ) else {
            destroy_carriers(world, &carrier_ids);
            return false;
        };
        if let Some(unit) = world.get_unit_mut(carrier_unit_id) {
            unit.physics = None;
        }
        carrier_ids.push(carrier_id);
        flights.push((
            carrier_id,
            SquadTransportPlan {
                passenger_squad_ids: group.passenger_squad_ids,
                start_position,
                dropoff_position: group.dropoff_position,
                incoming_target,
                outgoing_target,
                rally_point: request.rally_point,
                attack_move: request.attack_move,
                facing: request.facing,
            },
        ));
    }
    let started = if flights.len() == 1 {
        let (carrier_id, plan) = flights.pop().expect("one trigger transport flight");
        world.start_transport_fly_in(carrier_id, plan)
    } else {
        world.start_transport_fly_in_batch(flights)
    };
    if started {
        true
    } else {
        destroy_carriers(world, &carrier_ids);
        false
    }
}

#[derive(Debug, Clone, Copy)]
struct TriggerTransportProfile<'a> {
    trigger_prototype_id: i32,
    count_contains: &'a [String],
    count_capacity: f32,
    load_contains: &'a [String],
    load_capacity: f32,
    maximum_transports: u32,
    carrier_spacing: f32,
    settings: TransportSettings,
}

fn trigger_transport_profile<'a>(
    world: &World,
    database: &'a Database,
    player_id: u8,
) -> Option<TriggerTransportProfile<'a>> {
    let civ_id = world.get_player(player_id)?.civ_id;
    let civ = usize::try_from(civ_id)
        .ok()
        .and_then(|index| database.civs.get(index))?;
    let (trigger_prototype_id, trigger) =
        object_prototype(database, civ.transport_trigger.as_deref()?)?;
    let count = civ
        .transport
        .as_deref()
        .and_then(|name| object_prototype(database, name))
        .map(|(_, prototype)| prototype);
    Some(TriggerTransportProfile {
        trigger_prototype_id,
        count_contains: count.map_or(&[], |prototype| prototype.contain.as_slice()),
        count_capacity: count.map_or(0.0, transport_capacity),
        load_contains: &trigger.contain,
        load_capacity: transport_capacity(trigger),
        maximum_transports: database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.transport_max)
            .unwrap_or(3),
        carrier_spacing: transport_carrier_spacing(trigger),
        settings: transport_settings(database),
    })
}

fn object_prototype<'a>(database: &'a Database, name: &str) -> Option<(i32, &'a ProtoObject)> {
    let prototype_id = object_prototype_id(database, name)?;
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
        .map(|prototype| (prototype_id, prototype))
}

fn transport_capacity(prototype: &ProtoObject) -> f32 {
    prototype
        .max_contained
        .filter(|capacity| *capacity > 0)
        .and_then(|capacity| capacity.to_f32())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct TransportSettings {
    incoming_height: f32,
    incoming_offset: f32,
    outgoing_height: f32,
    outgoing_offset: f32,
    dropoff_height: f32,
}

fn transport_settings(database: &Database) -> TransportSettings {
    let game_data = database.game_data.as_ref();
    TransportSettings {
        incoming_height: setting(
            game_data.and_then(|data| data.transport_incoming_height),
            60.0,
        ),
        incoming_offset: setting(
            game_data.and_then(|data| data.transport_incoming_offset),
            40.0,
        ),
        outgoing_height: setting(
            game_data.and_then(|data| data.transport_outgoing_height),
            60.0,
        ),
        outgoing_offset: setting(
            game_data.and_then(|data| data.transport_outgoing_offset),
            40.0,
        ),
        dropoff_height: setting(
            game_data.and_then(|data| data.transport_dropoff_height),
            12.0,
        ),
    }
}

fn outgoing_target(
    request: &TriggerFlyInRequest,
    profile: &TriggerTransportProfile<'_>,
    dropoff: Vec3,
    direction: Vec3,
) -> Vec3 {
    let horizontal = request.fly_off_end.map_or_else(
        || direction * profile.settings.outgoing_offset,
        |end| {
            let delta = end - request.dropoff_position;
            Vec3::new(delta.x, 0.0, delta.z)
        },
    );
    dropoff + horizontal + Vec3::Y * profile.settings.outgoing_height
}

fn destroy_carriers(world: &mut World, carrier_ids: &[EntityId]) {
    for carrier_id in carrier_ids {
        let _destroyed = world.kill_squad(*carrier_id, true);
    }
}

fn positions_are_equal(left: Vec3, right: Vec3) -> bool {
    left.distance_squared(right) <= 0.0001
}

fn horizontal_direction(value: Vec3) -> Option<Vec3> {
    Vec3::new(value.x, 0.0, value.z).try_normalize()
}

fn setting(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}
