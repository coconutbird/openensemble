//! Retail prototype attachment effects.

use super::{EffectOutcome, value_at};
use crate::spawn::find_object_by_id;
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use pipeline::database::hw1::Database;

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<EffectOutcome> {
    (effect.effect_type == EffectType::AttachmentAddType)
        .then(|| add_type(effect, script, world, database))
}

fn add_type(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 5) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(prototype_id) = prototype_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    if find_object_by_id(database, prototype_id).is_none() {
        return EffectOutcome::Applied;
    }
    let receiving_units = if effect.version == 3 {
        receiving_units_v3(effect, script, world)
    } else {
        receiving_units_v5(effect, script, world)
    };
    for unit_id in receiving_units {
        let _attachment = world.add_prototype_attachment_to_unit(database, unit_id, prototype_id);
    }
    EffectOutcome::Applied
}

fn receiving_units_v3(effect: &Effect, script: &TriggerScript, world: &World) -> Vec<EntityId> {
    let mut receiving = Vec::new();
    if let Some(unit_id) = entity_at(effect, script, 5) {
        unique_add(&mut receiving, unit_id);
    }
    for unit_id in entity_list_at(effect, script, 3, ListType::Unit) {
        unique_add(&mut receiving, unit_id);
    }
    append_squad_members(&mut receiving, entity_at(effect, script, 6), world);
    for squad_id in entity_list_at(effect, script, 4, ListType::Squad) {
        append_squad_members(&mut receiving, Some(squad_id), world);
    }
    receiving
}

fn receiving_units_v5(effect: &Effect, script: &TriggerScript, world: &World) -> Vec<EntityId> {
    let mut receiving = entity_list_at(effect, script, 3, ListType::Unit);
    if let Some(unit_id) = entity_at(effect, script, 5) {
        unique_add(&mut receiving, unit_id);
    }
    for squad_id in entity_list_at(effect, script, 4, ListType::Squad) {
        append_squad_members(&mut receiving, Some(squad_id), world);
    }
    append_squad_members(&mut receiving, entity_at(effect, script, 6), world);
    receiving
}

fn append_squad_members(receiving: &mut Vec<EntityId>, squad_id: Option<EntityId>, world: &World) {
    let Some(squad) = squad_id.and_then(|squad_id| world.get_squad(squad_id)) else {
        return;
    };
    for &unit_id in &squad.unit_ids {
        unique_add(receiving, unit_id);
    }
}

fn prototype_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<i32> {
    match value_at(effect, script, slot)? {
        TriggerValue::ProtoObject(value) | TriggerValue::Int(value) => Some(*value),
        _ => None,
    }
}

fn entity_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    value_at(effect, script, slot).and_then(TriggerValue::as_entity)
}

#[derive(Debug, Clone, Copy)]
enum ListType {
    Unit,
    Squad,
}

fn entity_list_at(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
    list_type: ListType,
) -> Vec<EntityId> {
    match (list_type, value_at(effect, script, slot)) {
        (ListType::Unit, Some(TriggerValue::UnitList(values)))
        | (ListType::Squad, Some(TriggerValue::SquadList(values))) => values.clone(),
        _ => Vec::new(),
    }
}

fn unique_add(values: &mut Vec<EntityId>, value: EntityId) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
#[path = "attachments/tests.rs"]
mod tests;
