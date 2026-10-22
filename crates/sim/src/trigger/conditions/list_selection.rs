//! Retail first/last/random entity-list selection conditions.

use super::{as_i32, value_at, write_trigger_value};
use crate::trigger::{Condition, TriggerScript, TriggerValue, TriggerVec3};
use crate::{EntityId, World};

#[derive(Debug, Clone, Copy)]
enum SelectionKind {
    Unit,
    Squad,
}

#[derive(Debug, Clone, Copy)]
enum ScalarSelectionKind {
    Player,
    Team,
    ProtoObject,
    ProtoSquad,
    Tech,
    Integer,
    DesignLine,
}

pub(super) fn can_get_one_unit(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_entity(condition, script, world, SelectionKind::Unit)
}

pub(super) fn can_get_one_squad(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_entity(condition, script, world, SelectionKind::Squad)
}

pub(super) fn can_get_one_player(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::Player, 3)
}

pub(super) fn can_get_one_team(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::Team, 3)
}

pub(super) fn can_get_one_proto_object(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(
        condition,
        script,
        world,
        ScalarSelectionKind::ProtoObject,
        4,
    )
}

pub(super) fn can_get_one_proto_squad(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::ProtoSquad, 3)
}

pub(super) fn can_get_one_object_type(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    let Some(values) = value_at(condition, script, 1).and_then(|value| match value {
        TriggerValue::ObjectTypeList(values) => Some(values),
        _ => None,
    }) else {
        return false;
    };
    let Some(selected) = select_value(values, condition, script, world) else {
        return false;
    };
    if let Some(output_id) = condition.variable_id(3) {
        write_trigger_value(script, output_id, TriggerValue::ObjectType(selected));
    }
    true
}

pub(super) fn can_get_one_tech(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::Tech, 3)
}

pub(super) fn can_get_one_integer(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::Integer, 3)
}

pub(super) fn can_get_one_design_line(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    select_i32(condition, script, world, ScalarSelectionKind::DesignLine, 3)
}

pub(super) fn can_get_one_location(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    let Some(values) = value_at(condition, script, 1).and_then(|value| match value {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => Some(values),
        _ => None,
    }) else {
        return false;
    };
    let Some(selected) = select_value(values, condition, script, world) else {
        return false;
    };
    if let Some(output_id) = condition.variable_id(3) {
        write_trigger_value(script, output_id, TriggerValue::Vector(selected));
    }
    true
}

/// `CanRemoveOneLocation` (566, `BTriggerCondition_tcCanRemoveOneLocation`).
pub(super) fn can_remove_one_location(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    remove_one(condition, script, world, |value| match value {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => {
            Some(RemovableList::Vectors(values))
        }
        _ => None,
    })
}

/// `CanRemoveOneFloat` (975, `BTriggerCondition_tcCanRemoveOneFloat`).
pub(super) fn can_remove_one_float(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
) -> bool {
    remove_one(condition, script, world, |value| match value {
        TriggerValue::FloatList(values) => Some(RemovableList::Floats(values)),
        _ => None,
    })
}

enum RemovableList<'a> {
    Vectors(&'a mut Vec<TriggerVec3>),
    Floats(&'a mut Vec<f32>),
}

impl RemovableList<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Vectors(values) => values.len(),
            Self::Floats(values) => values.len(),
        }
    }

    fn remove(self, index: usize, ordered: bool) -> TriggerValue {
        match self {
            Self::Vectors(values) => TriggerValue::Vector(take_at(values, index, ordered)),
            Self::Floats(values) => TriggerValue::Float(take_at(values, index, ordered)),
        }
    }
}

fn take_at<T>(values: &mut Vec<T>, index: usize, ordered: bool) -> T {
    if ordered {
        values.remove(index)
    } else {
        values.swap_remove(index)
    }
}

/// Remove one value from the list in signature 1 and write it to signature 3.
///
/// Retail reads the list and the list position in signature 2 without a null check. An empty
/// list or an unknown position gives false. `First` and `Last` keep the order of the other
/// values. `Random` takes a random index from the trigger stream and moves the last value into
/// the free slot.
fn remove_one(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
    list: impl FnOnce(&mut TriggerValue) -> Option<RemovableList<'_>>,
) -> bool {
    let raw = |script: &TriggerScript, signature_id| {
        condition
            .variable_id(signature_id)
            .and_then(|variable_id| script.get_variable(variable_id))
            .map(|variable| variable.value.clone())
    };
    let Some(TriggerValue::Int(position)) = raw(script, 2) else {
        return false;
    };
    let Some(list) = condition
        .variable_id(1)
        .and_then(|variable_id| script.get_variable_mut(variable_id))
        .and_then(|variable| list(&mut variable.value))
    else {
        return false;
    };
    let count = list.len();
    if count == 0 {
        return false;
    }
    let removed = match position {
        0 => list.remove(0, true),
        1 => list.remove(count - 1, true),
        2 => {
            let maximum = u32::try_from(count - 1).unwrap_or(u32::MAX);
            let index = world.trigger_random_index(maximum) as usize;
            list.remove(index, false)
        }
        _ => return false,
    };
    if let Some(output_id) = condition.variable_id(3) {
        write_trigger_value(script, output_id, removed);
    }
    true
}

fn select_entity(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
    kind: SelectionKind,
) -> bool {
    let values = list_at(condition, script, kind).unwrap_or_default();
    let selected = select_value(values, condition, script, world)
        .filter(|entity_id| entity_exists(world, *entity_id, kind));
    if let Some(output_id) = condition.variable_id(3) {
        let value = selected.unwrap_or(EntityId::INVALID);
        write_trigger_value(
            script,
            output_id,
            match kind {
                SelectionKind::Unit => TriggerValue::Unit(value),
                SelectionKind::Squad => TriggerValue::Squad(value),
            },
        );
    }
    selected.is_some()
}

fn select_i32(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
    kind: ScalarSelectionKind,
    output_slot: u16,
) -> bool {
    let Some(values) = scalar_list_at(condition, script, kind) else {
        return false;
    };
    let Some(selected) = select_value(values, condition, script, world) else {
        return false;
    };
    if let Some(output_id) = condition.variable_id(output_slot) {
        write_trigger_value(script, output_id, scalar_value(kind, selected));
    }
    true
}

fn scalar_list_at<'a>(
    condition: &Condition,
    script: &'a TriggerScript,
    kind: ScalarSelectionKind,
) -> Option<&'a [i32]> {
    match (kind, value_at(condition, script, 1)?) {
        (ScalarSelectionKind::Player, TriggerValue::PlayerList(values))
        | (ScalarSelectionKind::Team, TriggerValue::TeamList(values))
        | (ScalarSelectionKind::ProtoObject, TriggerValue::ProtoObjectList(values))
        | (ScalarSelectionKind::ProtoSquad, TriggerValue::ProtoSquadList(values))
        | (ScalarSelectionKind::Tech, TriggerValue::TechList(values))
        | (ScalarSelectionKind::Integer, TriggerValue::IntegerList(values))
        | (ScalarSelectionKind::DesignLine, TriggerValue::DesignLineList(values)) => Some(values),
        _ => None,
    }
}

fn scalar_value(kind: ScalarSelectionKind, value: i32) -> TriggerValue {
    match kind {
        ScalarSelectionKind::Player => TriggerValue::Player(value),
        ScalarSelectionKind::Team => TriggerValue::Team(value),
        ScalarSelectionKind::ProtoObject => TriggerValue::ProtoObject(value),
        ScalarSelectionKind::ProtoSquad => TriggerValue::ProtoSquad(value),
        ScalarSelectionKind::Tech => TriggerValue::Tech(value),
        ScalarSelectionKind::Integer => TriggerValue::Int(value),
        ScalarSelectionKind::DesignLine => TriggerValue::DesignLine(value),
    }
}

fn list_at<'a>(
    condition: &Condition,
    script: &'a TriggerScript,
    kind: SelectionKind,
) -> Option<&'a [EntityId]> {
    match (kind, value_at(condition, script, 1)?) {
        (SelectionKind::Unit, TriggerValue::UnitList(values))
        | (SelectionKind::Squad, TriggerValue::SquadList(values)) => Some(values),
        _ => None,
    }
}

fn select_value<T: Clone>(
    values: &[T],
    condition: &Condition,
    script: &TriggerScript,
    world: &mut World,
) -> Option<T> {
    let position = value_at(condition, script, 2)
        .and_then(as_i32)
        .unwrap_or(-1);
    match position {
        0 => values.first().cloned(),
        1 => values.last().cloned(),
        2 => {
            let maximum = u32::try_from(values.len().checked_sub(1)?).ok()?;
            values
                .get(world.trigger_random_index(maximum) as usize)
                .cloned()
        }
        _ => None,
    }
}

fn entity_exists(world: &World, entity_id: EntityId, kind: SelectionKind) -> bool {
    match kind {
        SelectionKind::Unit => world.get_unit(entity_id).is_some(),
        SelectionKind::Squad => world.get_squad(entity_id).is_some(),
    }
}

#[cfg(test)]
mod tests;
