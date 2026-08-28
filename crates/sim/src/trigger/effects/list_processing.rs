//! Retail list partition, shuffle, and set-difference effects.

use super::support::{
    EntityListKind, entity_list, entity_list_mut, float_at, integer_at, unique_add,
    used_variable_id, variable_is_used,
};
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, VarId};
use crate::{EntityId, World};
use num_traits::ToPrimitive;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    match effect.effect_type {
        EffectType::SquadListPartition => partition(effect, script, EntityListKind::Squad),
        EffectType::UnitListPartition => partition(effect, script, EntityListKind::Unit),
        EffectType::SquadListShuffle
        | EffectType::LocationListShuffle
        | EffectType::EntityListShuffle
        | EffectType::PlayerListShuffle
        | EffectType::TeamListShuffle
        | EffectType::UnitListShuffle
        | EffectType::ProtoObjectListShuffle
        | EffectType::ObjectTypeListShuffle
        | EffectType::ProtoSquadListShuffle
        | EffectType::TechListShuffle => shuffle(effect, script, world),
        EffectType::SquadListDiff => difference(effect, script, EntityListKind::Squad),
        EffectType::UnitListDiff => difference(effect, script, EntityListKind::Unit),
        _ => EffectOutcome::Unsupported(effect.raw_type),
    }
}

pub(super) fn partition(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(source_id) = list_variable_id(effect, script, 1, kind) else {
        return EffectOutcome::Skipped;
    };
    let Some(first_output_id) = list_variable_id(effect, script, 4, kind) else {
        return EffectOutcome::Skipped;
    };
    let Some(second_output_id) = list_variable_id(effect, script, 5, kind) else {
        return EffectOutcome::Skipped;
    };
    let Some(source) = list_value(script, source_id, kind).cloned() else {
        return EffectOutcome::Skipped;
    };
    let Some(count_to_a) = partition_count(effect, script, source.len()) else {
        return EffectOutcome::Skipped;
    };

    if kind == EntityListKind::Unit {
        clear_list(script, source_id, kind);
    }
    clear_list(script, first_output_id, kind);
    clear_list(script, second_output_id, kind);

    for (index, entity_id) in source.into_iter().enumerate() {
        let destination_id = if index < count_to_a {
            first_output_id
        } else {
            second_output_id
        };
        add_to_list(script, destination_id, kind, entity_id);
    }
    EffectOutcome::Applied
}

pub(super) fn shuffle(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(list_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(length) = script
        .get_variable(list_id)
        .and_then(|variable| shuffle_length(effect.effect_type, &variable.value))
    else {
        return EffectOutcome::Skipped;
    };
    if length <= 1 {
        return EffectOutcome::Applied;
    }
    let Ok(maximum) = u32::try_from(length - 1) else {
        return EffectOutcome::Skipped;
    };

    let swaps = (0..length)
        .map(|_| world.trigger_random_index(maximum) as usize)
        .collect::<Vec<_>>();
    let Some(variable) = script.get_variable_mut(list_id) else {
        return EffectOutcome::Skipped;
    };
    if !apply_shuffle(effect.effect_type, &mut variable.value, &swaps) {
        return EffectOutcome::Skipped;
    }
    EffectOutcome::Applied
}

fn shuffle_length(effect_type: EffectType, value: &TriggerValue) -> Option<usize> {
    match (effect_type, value) {
        (EffectType::SquadListShuffle, TriggerValue::SquadList(values))
        | (EffectType::EntityListShuffle, TriggerValue::EntityList(values))
        | (EffectType::UnitListShuffle, TriggerValue::UnitList(values)) => Some(values.len()),
        (
            EffectType::LocationListShuffle,
            TriggerValue::LocationList(values) | TriggerValue::VectorList(values),
        ) => Some(values.len()),
        (EffectType::PlayerListShuffle, TriggerValue::PlayerList(values))
        | (EffectType::TeamListShuffle, TriggerValue::TeamList(values))
        | (EffectType::ProtoObjectListShuffle, TriggerValue::ProtoObjectList(values))
        | (EffectType::ProtoSquadListShuffle, TriggerValue::ProtoSquadList(values))
        | (EffectType::TechListShuffle, TriggerValue::TechList(values)) => Some(values.len()),
        (EffectType::ObjectTypeListShuffle, TriggerValue::ObjectTypeList(values)) => {
            Some(values.len())
        }
        _ => None,
    }
}

fn apply_shuffle(effect_type: EffectType, value: &mut TriggerValue, swaps: &[usize]) -> bool {
    match (effect_type, value) {
        (EffectType::SquadListShuffle, TriggerValue::SquadList(values))
        | (EffectType::EntityListShuffle, TriggerValue::EntityList(values))
        | (EffectType::UnitListShuffle, TriggerValue::UnitList(values)) => {
            shuffle_values(values, swaps);
        }
        (
            EffectType::LocationListShuffle,
            TriggerValue::LocationList(values) | TriggerValue::VectorList(values),
        ) => {
            shuffle_values(values, swaps);
        }
        (EffectType::PlayerListShuffle, TriggerValue::PlayerList(values))
        | (EffectType::TeamListShuffle, TriggerValue::TeamList(values))
        | (EffectType::ProtoObjectListShuffle, TriggerValue::ProtoObjectList(values))
        | (EffectType::ProtoSquadListShuffle, TriggerValue::ProtoSquadList(values))
        | (EffectType::TechListShuffle, TriggerValue::TechList(values)) => {
            shuffle_values(values, swaps);
        }
        (EffectType::ObjectTypeListShuffle, TriggerValue::ObjectTypeList(values)) => {
            shuffle_values(values, swaps);
        }
        _ => return false,
    }
    true
}

fn shuffle_values<T>(values: &mut [T], swaps: &[usize]) {
    for (index, random_index) in swaps.iter().copied().enumerate() {
        values.swap(index, random_index);
    }
}

pub(super) fn difference(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(list_a) = list_at(effect, script, 1, kind).cloned() else {
        return EffectOutcome::Skipped;
    };
    let Some(list_b) = list_at(effect, script, 2, kind).cloned() else {
        return EffectOutcome::Skipped;
    };
    let outputs = [
        optional_output(effect, script, 3, kind),
        optional_output(effect, script, 4, kind),
        optional_output(effect, script, 5, kind),
    ];
    if outputs.iter().any(Result::is_err) {
        return EffectOutcome::Skipped;
    }
    let [only_a, only_b, in_both] = outputs.map(Result::unwrap);

    for output_id in [only_a, only_b, in_both].into_iter().flatten() {
        clear_list(script, output_id, kind);
    }
    for entity_id in list_a.iter().copied() {
        let output_id = if list_b.contains(&entity_id) {
            in_both
        } else {
            only_a
        };
        if let Some(output_id) = output_id {
            add_to_list(script, output_id, kind, entity_id);
        }
    }
    for entity_id in list_b.iter().copied() {
        if !list_a.contains(&entity_id)
            && let Some(output_id) = only_b
        {
            add_to_list(script, output_id, kind, entity_id);
        }
    }
    EffectOutcome::Applied
}

fn partition_count(effect: &Effect, script: &TriggerScript, length: usize) -> Option<usize> {
    if variable_is_used(effect, script, 3) {
        let count = integer_at(effect, script, 3)?;
        let count = count.clamp(0, i32::try_from(length).unwrap_or(i32::MAX));
        return usize::try_from(count).ok();
    }
    if variable_is_used(effect, script, 2) {
        let percent = float_at(effect, script, 2)?.clamp(0.0, 1.0);
        let length_as_float = length.to_f32()?;
        return (percent * length_as_float).round_ties_even().to_usize();
    }
    Some(length)
}

fn optional_output(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Result<Option<VarId>, ()> {
    if !variable_is_used(effect, script, signature_id) {
        return Ok(None);
    }
    list_variable_id(effect, script, signature_id, kind)
        .map(Some)
        .ok_or(())
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<&'a Vec<EntityId>> {
    entity_list(kind, value_at(effect, script, signature_id)?)
}

fn list_variable_id(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, signature_id)?;
    list_value(script, variable_id, kind).map(|_| variable_id)
}

fn list_value(
    script: &TriggerScript,
    variable_id: VarId,
    kind: EntityListKind,
) -> Option<&Vec<EntityId>> {
    script
        .get_variable(variable_id)
        .and_then(|variable| entity_list(kind, &variable.value))
}

fn list_value_mut(
    script: &mut TriggerScript,
    variable_id: VarId,
    kind: EntityListKind,
) -> Option<&mut Vec<EntityId>> {
    script
        .get_variable_mut(variable_id)
        .and_then(|variable| entity_list_mut(kind, &mut variable.value))
}

fn clear_list(script: &mut TriggerScript, variable_id: VarId, kind: EntityListKind) {
    if let Some(values) = list_value_mut(script, variable_id, kind) {
        values.clear();
    }
}

fn add_to_list(
    script: &mut TriggerScript,
    variable_id: VarId,
    kind: EntityListKind,
    entity_id: EntityId,
) {
    if let Some(values) = list_value_mut(script, variable_id, kind) {
        unique_add(values, entity_id);
    }
}

#[cfg(test)]
mod tests;
