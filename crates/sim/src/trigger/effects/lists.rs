//! Retail entity-list construction, query, and mutation effects.

use super::support::{
    EntityListKind, bool_at, entity_list, entity_list_mut, float_at, integer_at, object_type_at,
    player_at, unique_add, used_variable_id, variable_is_used, vector_at,
};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue, VarId};
use crate::{EntityId, World};

pub(super) fn get_units(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if effect.version != 3 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(filters) = unit_query_filters(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let mut results = world.find_live_units(
        filters.player_id,
        filters.object_type.as_deref(),
        filters.area,
    );
    if let Some(filter_list) = filters.filter_list {
        results.retain(|unit_id| {
            filter_list.contains(unit_id)
                || filter_list.iter().any(|filter_id| {
                    world
                        .get_squad(*filter_id)
                        .is_some_and(|squad| squad.contains_unit(*unit_id))
                })
        });
    }
    write_query_outputs(effect, script, &results, EntityListKind::Unit, 5, 6);
    EffectOutcome::Applied
}

pub(super) fn get_squads(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if effect.version != 4 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(filters) = squad_query_filters(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let mut results = if filters.area.is_some() {
        world.find_live_squads(
            filters.player_id,
            filters.prototype_id,
            filters.object_type.as_deref(),
            filters.area,
        )
    } else {
        world.find_live_squads(filters.player_id, filters.prototype_id, None, None)
    };
    if filters.area.is_some() {
        if let Some(filter_list) = filters.filter_list {
            results.retain(|squad_id| filter_list.contains(squad_id));
        }
    } else {
        if let Some(filter_list) = filters.filter_list {
            remove_unordered(&mut results, |squad_id| filter_list.contains(squad_id));
        }
        if let Some(object_type) = filters.object_type.as_deref() {
            remove_unordered(&mut results, |squad_id| {
                world.get_squad(*squad_id).is_some_and(|squad| {
                    squad.unit_ids.iter().all(|unit_id| {
                        world
                            .get_unit(*unit_id)
                            .is_some_and(|unit| unit.is_object_type(object_type))
                    })
                })
            });
        }
    }
    write_query_outputs(effect, script, &results, EntityListKind::Squad, 5, 6);
    EffectOutcome::Applied
}

pub(super) fn list_get_size(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(values) = list_at(effect, script, 1, kind) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let size = i32::try_from(values.len()).unwrap_or(i32::MAX);
    write_value(script, output_id, TriggerValue::Int(size))
}

pub(super) fn get_proto_squad(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let prototype_id = scalar_at(effect, script, 1, EntityListKind::Squad)
        .and_then(|squad_id| world.get_squad(squad_id))
        .map_or(-1, |squad| squad.proto_squad_id);
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::ProtoSquad(prototype_id))
}

pub(super) fn list_add(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(destination_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    if !has_list(script, destination_id, kind) {
        return EffectOutcome::Skipped;
    }
    if bool_at(effect, script, 5).unwrap_or(false) {
        mutate_list(script, destination_id, kind, Vec::clear);
    }
    if let Some(entity_id) = scalar_at(effect, script, 2, kind)
        && entity_exists(world, entity_id, kind)
    {
        mutate_list(script, destination_id, kind, |values| {
            unique_add(values, entity_id);
        });
    }
    add_list_slot(effect, script, destination_id, kind, 3);
    add_list_slot(effect, script, destination_id, kind, 4);
    EffectOutcome::Applied
}

pub(super) fn list_remove(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(destination_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    if !has_list(script, destination_id, kind) {
        return EffectOutcome::Skipped;
    }
    if bool_at(effect, script, 4).unwrap_or(false) {
        mutate_list(script, destination_id, kind, Vec::clear);
        return EffectOutcome::Applied;
    }
    if let Some(entity_id) = scalar_at(effect, script, 2, kind) {
        mutate_list(script, destination_id, kind, |values| {
            remove_first(values, entity_id);
        });
    }
    let remove_list_id = used_variable_id(effect, script, 3);
    if remove_list_id == Some(destination_id) {
        remove_self_list(script, destination_id, kind);
    } else if let Some(values) = list_at(effect, script, 3, kind).cloned() {
        mutate_list(script, destination_id, kind, |destination| {
            for entity_id in values {
                remove_first(destination, entity_id);
            }
        });
    }
    EffectOutcome::Applied
}

#[derive(Debug)]
struct UnitQueryFilters {
    player_id: Option<u8>,
    object_type: Option<String>,
    area: Option<(glam::Vec3, f32)>,
    filter_list: Option<Vec<EntityId>>,
}

fn unit_query_filters(effect: &Effect, script: &TriggerScript) -> Option<UnitQueryFilters> {
    let player_id = optional_player(effect, script, 1).ok()?;
    let object_type = optional_object_type(effect, script, 2).ok()?;
    let area = optional_area(effect, script, 3, 4).ok()?;
    let filter_list = optional_list(effect, script, 7, EntityListKind::Unit).ok()?;
    Some(UnitQueryFilters {
        player_id,
        object_type,
        area,
        filter_list,
    })
}

#[derive(Debug)]
struct SquadQueryFilters {
    player_id: Option<u8>,
    prototype_id: Option<i32>,
    object_type: Option<String>,
    area: Option<(glam::Vec3, f32)>,
    filter_list: Option<Vec<EntityId>>,
}

fn squad_query_filters(effect: &Effect, script: &TriggerScript) -> Option<SquadQueryFilters> {
    let player_id = optional_player(effect, script, 1).ok()?;
    let prototype_id = optional_integer(effect, script, 2).ok()?;
    let area = optional_area(effect, script, 3, 4).ok()?;
    let filter_list = optional_list(effect, script, 7, EntityListKind::Squad).ok()?;
    let object_type = optional_object_type(effect, script, 8).ok()?;
    Some(SquadQueryFilters {
        player_id,
        prototype_id,
        object_type,
        area,
        filter_list,
    })
}

fn optional_player(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Result<Option<u8>, InvalidInput> {
    if variable_is_used(effect, script, signature_id) {
        player_at(effect, script, signature_id)
            .map(Some)
            .ok_or(InvalidInput)
    } else {
        Ok(None)
    }
}

fn optional_integer(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Result<Option<i32>, InvalidInput> {
    if variable_is_used(effect, script, signature_id) {
        integer_at(effect, script, signature_id)
            .map(Some)
            .ok_or(InvalidInput)
    } else {
        Ok(None)
    }
}

fn optional_object_type(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Result<Option<String>, InvalidInput> {
    if variable_is_used(effect, script, signature_id) {
        object_type_at(effect, script, signature_id)
            .map(Some)
            .ok_or(InvalidInput)
    } else {
        Ok(None)
    }
}

fn optional_area(
    effect: &Effect,
    script: &TriggerScript,
    location_slot: u16,
    distance_slot: u16,
) -> Result<Option<(glam::Vec3, f32)>, InvalidInput> {
    if variable_is_used(effect, script, location_slot)
        && variable_is_used(effect, script, distance_slot)
    {
        let location = vector_at(effect, script, location_slot).ok_or(InvalidInput)?;
        let distance = float_at(effect, script, distance_slot).ok_or(InvalidInput)?;
        Ok(Some((location, distance)))
    } else {
        Ok(None)
    }
}

fn optional_list(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Result<Option<Vec<EntityId>>, InvalidInput> {
    if variable_is_used(effect, script, signature_id) {
        list_at(effect, script, signature_id, kind)
            .cloned()
            .map(Some)
            .ok_or(InvalidInput)
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy)]
struct InvalidInput;

fn write_query_outputs(
    effect: &Effect,
    script: &mut TriggerScript,
    results: &[EntityId],
    kind: EntityListKind,
    list_slot: u16,
    count_slot: u16,
) {
    if let Some(count_id) = used_variable_id(effect, script, count_slot) {
        let count = i32::try_from(results.len()).unwrap_or(i32::MAX);
        let _outcome = write_value(script, count_id, TriggerValue::Int(count));
    }
    if let Some(list_id) = used_variable_id(effect, script, list_slot) {
        let value = match kind {
            EntityListKind::Unit => TriggerValue::UnitList(results.to_vec()),
            EntityListKind::Squad => TriggerValue::SquadList(results.to_vec()),
            EntityListKind::Object => TriggerValue::ObjectList(results.to_vec()),
        };
        let _outcome = write_value(script, list_id, value);
    }
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<&'a Vec<EntityId>> {
    entity_list(kind, value_at(effect, script, signature_id)?)
}

fn scalar_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<EntityId> {
    match (kind, value_at(effect, script, signature_id)?) {
        (EntityListKind::Unit, TriggerValue::Unit(value))
        | (EntityListKind::Squad, TriggerValue::Squad(value))
        | (EntityListKind::Object, TriggerValue::Object(value)) => Some(*value),
        _ => None,
    }
}

fn entity_exists(world: &World, entity_id: EntityId, kind: EntityListKind) -> bool {
    match kind {
        EntityListKind::Unit => world.get_unit(entity_id).is_some(),
        EntityListKind::Squad => world.get_squad(entity_id).is_some(),
        EntityListKind::Object => world.entity_position(entity_id).is_some(),
    }
}

fn add_list_slot(
    effect: &Effect,
    script: &mut TriggerScript,
    destination_id: VarId,
    kind: EntityListKind,
    signature_id: u16,
) {
    let Some(values) = list_at(effect, script, signature_id, kind).cloned() else {
        return;
    };
    mutate_list(script, destination_id, kind, |destination| {
        for entity_id in values {
            unique_add(destination, entity_id);
        }
    });
}

fn has_list(script: &TriggerScript, variable_id: VarId, kind: EntityListKind) -> bool {
    script
        .get_variable(variable_id)
        .and_then(|variable| entity_list(kind, &variable.value))
        .is_some()
}

fn mutate_list(
    script: &mut TriggerScript,
    variable_id: VarId,
    kind: EntityListKind,
    mutate: impl FnOnce(&mut Vec<EntityId>),
) {
    if let Some(values) = script
        .get_variable_mut(variable_id)
        .and_then(|variable| entity_list_mut(kind, &mut variable.value))
    {
        mutate(values);
    }
}

fn remove_first(values: &mut Vec<EntityId>, entity_id: EntityId) {
    if let Some(index) = values.iter().position(|value| *value == entity_id) {
        values.remove(index);
    }
}

fn remove_self_list(script: &mut TriggerScript, variable_id: VarId, kind: EntityListKind) {
    mutate_list(script, variable_id, kind, |values| {
        let mut index = 0;
        while index < values.len() {
            let entity_id = values[index];
            remove_first(values, entity_id);
            index += 1;
        }
    });
}

fn remove_unordered<T>(values: &mut Vec<T>, keep: impl Fn(&T) -> bool) {
    let mut index = 0;
    while index < values.len() {
        if keep(&values[index]) {
            index += 1;
        } else {
            values.swap_remove(index);
        }
    }
}
