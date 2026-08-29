//! Retail entity creation, kill, and immediate-destroy trigger effects.

use super::support::{
    EntityListKind, bool_at, combine_entities, entities_at, entity_list_mut, float_at, integer_at,
    player_at, unique_add, used_variable_id, variable_is_used, vector_at,
};
use super::{EffectOutcome, write_value};
use crate::entities::squads::SquadTransportPlan;
use crate::physics::{BoxCollider, PhysicsBody};
use crate::scenario::placed::create_trigger_unit_squad;
use crate::spawn::{MAX_SPAWN_BATCH, object_prototype_id, spawn_object_at, spawn_squad_at};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::Database;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::CreateObject => create_object(effect, script, world, database),
        EffectType::CreateSquad => create_squad(effect, script, world, database),
        EffectType::CreateSquads => create_squads(effect, script, world, database),
        EffectType::CreateUnit => create_unit(effect, script, world, database),
        EffectType::CreateObstructionUnit => {
            create_obstruction_unit(effect, script, world, database)
        }
        EffectType::Kill => kill_or_destroy(effect, script, world, false),
        EffectType::Destroy => kill_or_destroy(effect, script, world, true),
        _ => return None,
    };
    Some(outcome)
}

pub(super) fn create_object(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 5 | 6) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(prototype_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(position) = vector_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let forward = vector_at(effect, script, 10).unwrap_or(Vec3::Z);
    let clear = bool_at(effect, script, 9).unwrap_or(false);
    let created = spawn_object_at(world, database, player_id, prototype_id, position, forward).ok();
    if effect.version == 6
        && bool_at(effect, script, 11).unwrap_or(false)
        && let Some(unit) = created.and_then(|entity_id| world.get_unit_mut(entity_id))
    {
        unit.physics = None;
    }
    write_optional_entity(effect, script, 4, created, TriggerValue::Object);
    update_entity_list(effect, script, 8, created, clear, EntityListKind::Object);
    EffectOutcome::Applied
}

pub(super) fn create_squad(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 6 | 7) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(prototype_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(position) = vector_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let facing = if effect.version == 7 {
        vector_at(effect, script, 12)
    } else {
        None
    };
    let forward = facing.unwrap_or(Vec3::Z);
    let clear = bool_at(effect, script, 6).unwrap_or(false);
    let created = spawn_squad_at(world, database, player_id, prototype_id, position, forward).ok();
    if let Some(squad_id) = created {
        let rally_point = vector_at(effect, script, 10);
        let attack_move = bool_at(effect, script, 11).unwrap_or(false);
        let flew_in = trigger_fly_in(
            effect,
            script,
            world,
            database,
            TriggerFlyInRequest {
                passenger_squad_id: squad_id,
                player_id,
                dropoff_position: position,
                forward,
                facing,
                rally_point,
                attack_move,
            },
        );
        if !flew_in && let Some(rally_point) = rally_point {
            let _issued = world.issue_squad_move_order_to_position(
                player_id,
                squad_id,
                rally_point,
                attack_move,
                false,
            );
        }
    }
    write_optional_entity(effect, script, 4, created, TriggerValue::Squad);
    update_entity_list(effect, script, 5, created, clear, EntityListKind::Squad);
    EffectOutcome::Applied
}

fn trigger_fly_in(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: &Database,
    request: TriggerFlyInRequest,
) -> bool {
    let fly_in_start = variable_is_used(effect, script, 8)
        .then(|| vector_at(effect, script, 8))
        .flatten();
    let fly_off_end = variable_is_used(effect, script, 9)
        .then(|| vector_at(effect, script, 9))
        .flatten();
    if fly_in_start.is_none() && fly_off_end.is_none() {
        return false;
    }
    let Some(transport_prototype_id) =
        player_transport_prototype_id(world, database, request.player_id)
    else {
        return false;
    };
    let settings = transport_settings(database);
    let direction = flight_direction(fly_in_start, request.dropoff_position, request.forward);
    let start_anchor = fly_in_start.unwrap_or(request.dropoff_position);
    let start_position =
        start_anchor - direction * settings.incoming_offset + Vec3::Y * settings.incoming_height;
    let incoming_target = request.dropoff_position + Vec3::Y * settings.dropoff_height;
    let outgoing_target = fly_off_end
        .unwrap_or(request.dropoff_position + direction * settings.outgoing_offset)
        + Vec3::Y * settings.outgoing_height;
    let Some((transport_squad_id, transport_unit_id)) = create_trigger_unit_squad(
        world,
        database,
        request.player_id,
        transport_prototype_id,
        start_position,
        direction,
        true,
    ) else {
        return false;
    };
    if let Some(unit) = world.get_unit_mut(transport_unit_id) {
        unit.physics = None;
    }
    let started = world.start_transport_fly_in(
        transport_squad_id,
        SquadTransportPlan {
            passenger_squad_id: request.passenger_squad_id,
            start_position,
            dropoff_position: request.dropoff_position,
            incoming_target,
            outgoing_target,
            rally_point: request.rally_point,
            attack_move: request.attack_move,
            facing: request.facing,
        },
    );
    if !started {
        let _destroyed = world.kill_squad(transport_squad_id, true);
    }
    started
}

#[derive(Debug, Clone, Copy)]
struct TriggerFlyInRequest {
    passenger_squad_id: EntityId,
    player_id: u8,
    dropoff_position: Vec3,
    forward: Vec3,
    facing: Option<Vec3>,
    rally_point: Option<Vec3>,
    attack_move: bool,
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
        incoming_height: game_data
            .and_then(|data| data.transport_incoming_height)
            .unwrap_or(60.0),
        incoming_offset: game_data
            .and_then(|data| data.transport_incoming_offset)
            .unwrap_or(40.0),
        outgoing_height: game_data
            .and_then(|data| data.transport_outgoing_height)
            .unwrap_or(60.0),
        outgoing_offset: game_data
            .and_then(|data| data.transport_outgoing_offset)
            .unwrap_or(40.0),
        dropoff_height: game_data
            .and_then(|data| data.transport_dropoff_height)
            .unwrap_or(12.0),
    }
}

fn player_transport_prototype_id(world: &World, database: &Database, player_id: u8) -> Option<i32> {
    let civ_id = world.get_player(player_id)?.civ_id;
    let civ = usize::try_from(civ_id)
        .ok()
        .and_then(|index| database.civs.get(index))?;
    object_prototype_id(database, civ.transport_trigger.as_deref()?)
}

fn flight_direction(start: Option<Vec3>, dropoff: Vec3, fallback: Vec3) -> Vec3 {
    start
        .map(|start| dropoff - start)
        .and_then(horizontal_direction)
        .or_else(|| horizontal_direction(fallback))
        .unwrap_or(Vec3::Z)
}

fn horizontal_direction(value: Vec3) -> Option<Vec3> {
    Vec3::new(value.x, 0.0, value.z).try_normalize()
}

pub(super) fn create_squads(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(prototype_ids) = proto_squad_list_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    if prototype_ids.len() > usize::try_from(MAX_SPAWN_BATCH).unwrap_or(usize::MAX) {
        return EffectOutcome::Skipped;
    }
    let Some(player_id) = player_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(position) = vector_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let facing = vector_at(effect, script, 4).unwrap_or(Vec3::Z);
    let mut created = Vec::with_capacity(prototype_ids.len());
    for prototype_id in prototype_ids {
        if let Ok(squad_id) =
            spawn_squad_at(world, database, player_id, prototype_id, position, facing)
        {
            unique_add(&mut created, squad_id);
        }
    }

    // Retail falls back to this ground order whenever transport creation or
    // fly-in setup fails. The transport action itself is not modeled yet, so
    // preserve that deterministic fallback while retaining exact batch/output
    // behavior.
    if let Some(rally_point) = vector_at(effect, script, 7) {
        for squad_id in &created {
            let _issued = world.issue_move_order(player_id, *squad_id, rally_point);
        }
    }
    write_squad_batch_outputs(effect, script, &created);
    EffectOutcome::Applied
}

pub(super) fn create_unit(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(prototype_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(position) = vector_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(start_built) = bool_at(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let forward = if effect.version == 2 {
        vector_at(effect, script, 10).unwrap_or(Vec3::Z)
    } else {
        Vec3::Z
    };
    let created = create_trigger_unit_squad(
        world,
        database,
        player_id,
        prototype_id,
        position,
        forward,
        start_built,
    );
    let Some((squad_id, unit_id)) = created else {
        write_optional_entity(effect, script, 5, None, TriggerValue::Unit);
        write_optional_entity(effect, script, 7, None, TriggerValue::Squad);
        return EffectOutcome::Applied;
    };
    let clear = bool_at(effect, script, 9).unwrap_or(false);
    write_optional_entity(effect, script, 5, Some(unit_id), TriggerValue::Unit);
    update_entity_list(
        effect,
        script,
        6,
        Some(unit_id),
        clear,
        EntityListKind::Unit,
    );
    write_optional_entity(effect, script, 7, Some(squad_id), TriggerValue::Squad);
    update_entity_list(
        effect,
        script,
        8,
        Some(squad_id),
        clear,
        EntityListKind::Squad,
    );
    EffectOutcome::Applied
}

fn create_obstruction_unit(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let (Some(position), Some(forward), Some(radius_x), Some(radius_y), Some(radius_z)) = (
        vector_at(effect, script, 1),
        vector_at(effect, script, 2),
        float_at(effect, script, 3),
        float_at(effect, script, 4),
        float_at(effect, script, 5),
    ) else {
        return EffectOutcome::Skipped;
    };
    let created = obstruction_prototype_id(database).and_then(|prototype_id| {
        spawn_object_at(
            world,
            database,
            crate::player::GAIA_PLAYER,
            prototype_id,
            position,
            forward,
        )
        .ok()
    });
    if let Some(unit) = created.and_then(|unit_id| world.get_unit_mut(unit_id)) {
        let half_extents = Vec3::new(radius_x, radius_y, radius_z);
        unit.obstruction_half_extents = half_extents;
        unit.physics = Some(PhysicsBody::static_obstruction(BoxCollider::new(
            half_extents,
            Vec3::ZERO,
        )));
        unit.base.configure_prototype_mobility(true);
    }

    write_optional_entity(effect, script, 7, created, TriggerValue::Unit);
    update_entity_list(
        effect,
        script,
        8,
        created,
        bool_at(effect, script, 6).unwrap_or(false),
        EntityListKind::Unit,
    );
    EffectOutcome::Applied
}

fn obstruction_prototype_id(database: &Database) -> Option<i32> {
    let prototype_name = database
        .game_data
        .as_ref()?
        .code_proto_objects
        .as_ref()?
        .entries
        .iter()
        .find(|mapping| mapping.object_type.eq_ignore_ascii_case("Obstruction"))?
        .proto_name
        .trim();
    object_prototype_id(database, prototype_name)
}

pub(super) fn kill_or_destroy(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    immediate: bool,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let unit_slot = entities_at(effect, script, 3, EntityListKind::Unit);
    let unit_list_slot = entities_at(effect, script, 4, EntityListKind::Unit);
    let squad_slot = entities_at(effect, script, 5, EntityListKind::Squad);
    let squad_list_slot = entities_at(effect, script, 6, EntityListKind::Squad);
    let object_slot = (effect.version == 4)
        .then(|| entities_at(effect, script, 7, EntityListKind::Object))
        .flatten();
    let object_list_slot = (effect.version == 4)
        .then(|| entities_at(effect, script, 8, EntityListKind::Object))
        .flatten();
    if [
        unit_slot.as_ref(),
        unit_list_slot.as_ref(),
        squad_slot.as_ref(),
        squad_list_slot.as_ref(),
        object_slot.as_ref(),
        object_list_slot.as_ref(),
    ]
    .iter()
    .all(Option::is_none)
    {
        return EffectOutcome::Skipped;
    }

    let units = combine_entities(unit_slot, unit_list_slot);
    let squads = combine_entities(squad_slot, squad_list_slot);
    let objects = combine_entities(object_slot, object_list_slot);
    for entity_id in units {
        let _killed = world.kill_unit(entity_id, immediate);
    }
    for entity_id in squads {
        let _killed = world.kill_squad(entity_id, immediate);
    }
    for entity_id in objects {
        let _killed = world.kill_entity(entity_id, immediate);
    }
    EffectOutcome::Applied
}

fn write_optional_entity(
    effect: &Effect,
    script: &mut TriggerScript,
    signature_id: u16,
    entity_id: Option<EntityId>,
    wrap: fn(EntityId) -> TriggerValue,
) {
    let Some(variable_id) = used_variable_id(effect, script, signature_id) else {
        return;
    };
    let _outcome = write_value(
        script,
        variable_id,
        wrap(entity_id.unwrap_or(EntityId::INVALID)),
    );
}

fn update_entity_list(
    effect: &Effect,
    script: &mut TriggerScript,
    signature_id: u16,
    entity_id: Option<EntityId>,
    clear: bool,
    kind: EntityListKind,
) {
    let Some(variable_id) = used_variable_id(effect, script, signature_id) else {
        return;
    };
    let Some(variable) = script.get_variable_mut(variable_id) else {
        return;
    };
    let Some(values) = entity_list_mut(kind, &mut variable.value) else {
        return;
    };
    if clear {
        values.clear();
    }
    if let Some(entity_id) = entity_id {
        unique_add(values, entity_id);
    }
}

fn proto_squad_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<i32>> {
    let variable_id = used_variable_id(effect, script, signature_id)?;
    match &script.get_variable(variable_id)?.value {
        TriggerValue::ProtoSquadList(values) => Some(values.clone()),
        _ => None,
    }
}

fn write_squad_batch_outputs(effect: &Effect, script: &mut TriggerScript, created: &[EntityId]) {
    if let Some(variable_id) = used_variable_id(effect, script, 9)
        && let Some(variable) = script.get_variable_mut(variable_id)
    {
        variable.value = TriggerValue::SquadList(created.to_vec());
        variable.is_null = false;
    }
    let clear_existing = bool_at(effect, script, 11).unwrap_or(false);
    if let Some(variable_id) = used_variable_id(effect, script, 10)
        && let Some(variable) = script.get_variable_mut(variable_id)
        && let TriggerValue::SquadList(values) = &mut variable.value
    {
        if clear_existing {
            values.clear();
        }
        values.extend_from_slice(created);
        variable.is_null = false;
    }
}

#[cfg(test)]
mod tests;
