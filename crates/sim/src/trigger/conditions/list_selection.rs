//! Retail first/last/random entity-list selection conditions.

use super::{as_i32, value_at, write_trigger_value};
use crate::trigger::{Condition, TriggerScript, TriggerValue};
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
        TriggerValue::ObjectTypeList(values) => Some(values.clone()),
        _ => None,
    }) else {
        return false;
    };
    let Some(selected) = select_value(&values, condition, script, world) else {
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
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => {
            Some(values.clone())
        }
        _ => None,
    }) else {
        return false;
    };
    let Some(selected) = select_value(&values, condition, script, world) else {
        return false;
    };
    if let Some(output_id) = condition.variable_id(3) {
        write_trigger_value(script, output_id, TriggerValue::Vector(selected));
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
    let selected = select_value(&values, condition, script, world)
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
    let Some(selected) = select_value(&values, condition, script, world) else {
        return false;
    };
    if let Some(output_id) = condition.variable_id(output_slot) {
        write_trigger_value(script, output_id, scalar_value(kind, selected));
    }
    true
}

fn scalar_list_at(
    condition: &Condition,
    script: &TriggerScript,
    kind: ScalarSelectionKind,
) -> Option<Vec<i32>> {
    match (kind, value_at(condition, script, 1)?) {
        (ScalarSelectionKind::Player, TriggerValue::PlayerList(values))
        | (ScalarSelectionKind::Team, TriggerValue::TeamList(values))
        | (ScalarSelectionKind::ProtoObject, TriggerValue::ProtoObjectList(values))
        | (ScalarSelectionKind::ProtoSquad, TriggerValue::ProtoSquadList(values))
        | (ScalarSelectionKind::Tech, TriggerValue::TechList(values))
        | (ScalarSelectionKind::Integer, TriggerValue::IntegerList(values))
        | (ScalarSelectionKind::DesignLine, TriggerValue::DesignLineList(values)) => {
            Some(values.clone())
        }
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

fn list_at(
    condition: &Condition,
    script: &TriggerScript,
    kind: SelectionKind,
) -> Option<Vec<EntityId>> {
    match (kind, value_at(condition, script, 1)?) {
        (SelectionKind::Unit, TriggerValue::UnitList(values))
        | (SelectionKind::Squad, TriggerValue::SquadList(values)) => Some(values.clone()),
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
