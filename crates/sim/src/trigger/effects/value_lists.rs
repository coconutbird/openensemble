//! Retail typed non-entity list mutation effects.

use super::support::{bool_at, used_variable_id};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, TriggerVec3, VarId};

#[derive(Debug, Clone, Copy)]
enum ScalarListKind {
    ProtoObject,
    ProtoSquad,
    Tech,
    Integer,
}

pub(super) fn execute(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    match effect.effect_type {
        EffectType::LocationListAdd => add_locations(effect, script),
        EffectType::LocationListRemove => remove_locations(effect, script),
        EffectType::LocationListGetSize => location_list_size(effect, script),
        EffectType::ProtoObjectListAdd => add_scalars(effect, script, ScalarListKind::ProtoObject),
        EffectType::ProtoObjectListRemove => {
            remove_scalars(effect, script, ScalarListKind::ProtoObject)
        }
        EffectType::ProtoSquadListAdd => add_scalars(effect, script, ScalarListKind::ProtoSquad),
        EffectType::ProtoSquadListRemove => {
            remove_scalars(effect, script, ScalarListKind::ProtoSquad)
        }
        EffectType::TechListAdd => add_scalars(effect, script, ScalarListKind::Tech),
        EffectType::TechListRemove => remove_scalars(effect, script, ScalarListKind::Tech),
        EffectType::IntegerListAdd => add_scalars(effect, script, ScalarListKind::Integer),
        EffectType::IntegerListRemove => remove_scalars(effect, script, ScalarListKind::Integer),
        EffectType::IntegerListGetSize => integer_list_size(effect, script),
        _ => EffectOutcome::Unsupported(effect.raw_type),
    }
}

fn add_locations(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(destination_id) = vector_list_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let single = vector_at(effect, script, 2);
    let additions = vector_list_at(effect, script, 3).cloned();
    let clear = bool_at(effect, script, 4).unwrap_or(false);
    let Some(destination) = vector_list_mut(script, destination_id) else {
        return EffectOutcome::Skipped;
    };
    if clear {
        destination.clear();
    }
    destination.extend(single);
    destination.extend(additions.unwrap_or_default());
    EffectOutcome::Applied
}

fn remove_locations(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(destination_id) = vector_list_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let single = vector_at(effect, script, 2);
    let removals = vector_list_at(effect, script, 3).cloned();
    let remove_all = bool_at(effect, script, 4).unwrap_or(false);
    let Some(destination) = vector_list_mut(script, destination_id) else {
        return EffectOutcome::Skipped;
    };
    if remove_all {
        destination.clear();
        return EffectOutcome::Applied;
    }
    if let Some(single) = single {
        remove_first(destination, &single);
    }
    for value in removals.unwrap_or_default() {
        remove_first(destination, &value);
    }
    EffectOutcome::Applied
}

fn location_list_size(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(size) =
        vector_list_at(effect, script, 1).and_then(|values| i32::try_from(values.len()).ok())
    else {
        return EffectOutcome::Skipped;
    };
    write_size(effect, script, size)
}

fn add_scalars(effect: &Effect, script: &mut TriggerScript, kind: ScalarListKind) -> EffectOutcome {
    let Some(destination_id) = scalar_list_id(effect, script, 4, kind) else {
        return EffectOutcome::Skipped;
    };
    let single = scalar_at(effect, script, 1, kind);
    let additions = scalar_list_at(effect, script, 2, kind).cloned();
    let clear = bool_at(effect, script, 3).unwrap_or(false);
    let Some(destination) = scalar_list_mut(script, destination_id, kind) else {
        return EffectOutcome::Skipped;
    };
    if clear {
        destination.clear();
    }
    destination.extend(single);
    destination.extend(additions.unwrap_or_default());
    EffectOutcome::Applied
}

fn remove_scalars(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: ScalarListKind,
) -> EffectOutcome {
    let destination_slot = if matches!(kind, ScalarListKind::Integer) {
        5
    } else {
        4
    };
    let Some(destination_id) = scalar_list_id(effect, script, destination_slot, kind) else {
        return EffectOutcome::Skipped;
    };
    let single = scalar_at(effect, script, 1, kind);
    let removals = scalar_list_at(effect, script, 2, kind).cloned();
    let remove_all = bool_at(effect, script, 3).unwrap_or(false);
    let remove_duplicates =
        matches!(kind, ScalarListKind::Integer) && bool_at(effect, script, 4).unwrap_or(false);
    let Some(destination) = scalar_list_mut(script, destination_id, kind) else {
        return EffectOutcome::Skipped;
    };
    if remove_all {
        destination.clear();
        return EffectOutcome::Applied;
    }
    for value in single.into_iter().chain(removals.unwrap_or_default()) {
        if remove_duplicates {
            destination.retain(|candidate| *candidate != value);
        } else {
            remove_first(destination, &value);
        }
    }
    EffectOutcome::Applied
}

fn integer_list_size(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(size) = scalar_list_at(effect, script, 1, ScalarListKind::Integer)
        .and_then(|values| i32::try_from(values.len()).ok())
    else {
        return EffectOutcome::Skipped;
    };
    write_size(effect, script, size)
}

fn write_size(effect: &Effect, script: &mut TriggerScript, size: i32) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Int(size))
}

fn vector_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<TriggerVec3> {
    value_at(effect, script, slot).and_then(TriggerValue::as_location)
}

fn vector_list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    slot: u16,
) -> Option<&'a Vec<TriggerVec3>> {
    match value_at(effect, script, slot)? {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => Some(values),
        _ => None,
    }
}

fn vector_list_id(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, slot)?;
    vector_list_value(script, variable_id).map(|_| variable_id)
}

fn vector_list_value(script: &TriggerScript, variable_id: VarId) -> Option<&Vec<TriggerVec3>> {
    match &script.get_variable(variable_id)?.value {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => Some(values),
        _ => None,
    }
}

fn vector_list_mut(
    script: &mut TriggerScript,
    variable_id: VarId,
) -> Option<&mut Vec<TriggerVec3>> {
    match &mut script.get_variable_mut(variable_id)?.value {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => Some(values),
        _ => None,
    }
}

fn scalar_at(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
    kind: ScalarListKind,
) -> Option<i32> {
    match (kind, value_at(effect, script, slot)?) {
        (ScalarListKind::ProtoObject, TriggerValue::ProtoObject(value))
        | (ScalarListKind::ProtoSquad, TriggerValue::ProtoSquad(value))
        | (ScalarListKind::Tech, TriggerValue::Tech(value))
        | (ScalarListKind::Integer, TriggerValue::Int(value)) => Some(*value),
        _ => None,
    }
}

fn scalar_list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    slot: u16,
    kind: ScalarListKind,
) -> Option<&'a Vec<i32>> {
    let value = value_at(effect, script, slot)?;
    scalar_list(kind, value)
}

fn scalar_list_id(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
    kind: ScalarListKind,
) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, slot)?;
    script
        .get_variable(variable_id)
        .and_then(|variable| scalar_list(kind, &variable.value))
        .map(|_| variable_id)
}

fn scalar_list(kind: ScalarListKind, value: &TriggerValue) -> Option<&Vec<i32>> {
    match (kind, value) {
        (ScalarListKind::ProtoObject, TriggerValue::ProtoObjectList(values))
        | (ScalarListKind::ProtoSquad, TriggerValue::ProtoSquadList(values))
        | (ScalarListKind::Tech, TriggerValue::TechList(values))
        | (ScalarListKind::Integer, TriggerValue::IntegerList(values)) => Some(values),
        _ => None,
    }
}

fn scalar_list_mut(
    script: &mut TriggerScript,
    variable_id: VarId,
    kind: ScalarListKind,
) -> Option<&mut Vec<i32>> {
    match (kind, &mut script.get_variable_mut(variable_id)?.value) {
        (ScalarListKind::ProtoObject, TriggerValue::ProtoObjectList(values))
        | (ScalarListKind::ProtoSquad, TriggerValue::ProtoSquadList(values))
        | (ScalarListKind::Tech, TriggerValue::TechList(values))
        | (ScalarListKind::Integer, TriggerValue::IntegerList(values)) => Some(values),
        _ => None,
    }
}

fn remove_first<T: PartialEq>(values: &mut Vec<T>, target: &T) {
    if let Some(index) = values.iter().position(|value| value == target) {
        values.remove(index);
    }
}

#[cfg(test)]
mod tests;
