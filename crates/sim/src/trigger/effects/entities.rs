//! Retail entity creation, kill, and immediate-destroy trigger effects.

use super::support::{
    EntityListKind, bool_at, combine_entities, entities_at, entity_list_mut, integer_at, player_at,
    unique_add, used_variable_id, variable_is_used, vector_at,
};
use super::{EffectOutcome, write_value};
use crate::scenario::placed::create_trigger_unit_squad;
use crate::spawn::{MAX_SPAWN_BATCH, spawn_object_at, spawn_squad_at};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::Database;

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
    if variable_is_used(effect, script, 8)
        || variable_is_used(effect, script, 9)
        || bool_at(effect, script, 11).unwrap_or(false)
    {
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
    let forward = if effect.version == 7 {
        vector_at(effect, script, 12).unwrap_or(Vec3::Z)
    } else {
        Vec3::Z
    };
    let clear = bool_at(effect, script, 6).unwrap_or(false);
    let created = spawn_squad_at(world, database, player_id, prototype_id, position, forward).ok();
    if let (Some(squad_id), Some(rally_point)) = (created, vector_at(effect, script, 10))
        && let Some(squad) = world.get_squad_mut(squad_id)
    {
        squad.move_to(rally_point);
    }
    write_optional_entity(effect, script, 4, created, TriggerValue::Squad);
    update_entity_list(effect, script, 5, created, clear, EntityListKind::Squad);
    EffectOutcome::Applied
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
