//! Retail trigger-list iterator conditions.

use crate::EntityId;
use crate::trigger::{Condition, TriggerScript, TriggerValue, VarType};

#[derive(Debug, Clone, Copy)]
enum IteratorKind {
    Unit,
    Squad,
    Object,
}

#[derive(Debug, Clone, Copy)]
enum ScalarIteratorKind {
    Player,
    Team,
}

pub(super) fn next_player(condition: &Condition, script: &mut TriggerScript) -> bool {
    next_scalar(condition, script, ScalarIteratorKind::Player)
}

pub(super) fn next_team(condition: &Condition, script: &mut TriggerScript) -> bool {
    next_scalar(condition, script, ScalarIteratorKind::Team)
}

pub(super) fn next_unit(condition: &Condition, script: &mut TriggerScript) -> bool {
    next_entity(condition, script, IteratorKind::Unit)
}

pub(super) fn next_squad(condition: &Condition, script: &mut TriggerScript) -> bool {
    next_entity(condition, script, IteratorKind::Squad)
}

pub(super) fn next_object(condition: &Condition, script: &mut TriggerScript) -> bool {
    next_entity(condition, script, IteratorKind::Object)
}

pub(super) fn next_location(condition: &Condition, script: &mut TriggerScript) -> bool {
    let Some(iterator_id) = condition.variable_id(1) else {
        return false;
    };
    let Some(iterator) = iterator_value(script, iterator_id) else {
        return false;
    };
    let Some(source_id) = iterator.source_list_id() else {
        return false;
    };
    let Some(source) = vector_source_values(script, source_id) else {
        return false;
    };
    let Some(next) = source
        .iter()
        .copied()
        .find(|vector| !iterator.is_vector_visited(*vector))
    else {
        return false;
    };
    let Some(output_id) = condition.variable_id(2) else {
        return false;
    };
    if !script
        .get_variable(output_id)
        .is_some_and(|variable| variable.var_type == VarType::Vector)
    {
        return false;
    }
    let Some(iterator_variable) = script.get_variable_mut(iterator_id) else {
        return false;
    };
    let TriggerValue::Iterator(iterator) = &mut iterator_variable.value else {
        return false;
    };
    iterator.visit_vector(next);
    let Some(output) = script.get_variable_mut(output_id) else {
        return false;
    };
    output.value = TriggerValue::Vector(next);
    output.is_null = false;
    true
}

fn next_entity(condition: &Condition, script: &mut TriggerScript, kind: IteratorKind) -> bool {
    let Some(iterator_id) = condition.variable_id(1) else {
        return false;
    };
    let Some(iterator) = script
        .get_variable(iterator_id)
        .filter(|variable| !variable.is_null)
        .and_then(|variable| match &variable.value {
            TriggerValue::Iterator(iterator) => Some(iterator.clone()),
            _ => None,
        })
    else {
        return false;
    };
    let Some(source_id) = iterator.source_list_id() else {
        return false;
    };
    let Some(source) = source_values(script, source_id, kind) else {
        return false;
    };
    let next = source.iter().copied().find(|entity_id| match kind {
        IteratorKind::Unit => !iterator.is_unit_visited(*entity_id),
        IteratorKind::Squad => !iterator.is_squad_visited(*entity_id),
        IteratorKind::Object => !iterator.is_object_visited(*entity_id),
    });
    let Some(next) = next else {
        return false;
    };
    let Some(output_id) = condition.variable_id(2) else {
        return false;
    };
    if !output_has_type(script, output_id, kind) {
        return false;
    }

    let Some(iterator_variable) = script.get_variable_mut(iterator_id) else {
        return false;
    };
    let TriggerValue::Iterator(iterator) = &mut iterator_variable.value else {
        return false;
    };
    match kind {
        IteratorKind::Unit => iterator.visit_unit(next),
        IteratorKind::Squad => iterator.visit_squad(next),
        IteratorKind::Object => iterator.visit_object(next),
    }

    let Some(output) = script.get_variable_mut(output_id) else {
        return false;
    };
    output.value = match kind {
        IteratorKind::Unit => TriggerValue::Unit(next),
        IteratorKind::Squad => TriggerValue::Squad(next),
        IteratorKind::Object => TriggerValue::Object(next),
    };
    output.is_null = false;
    true
}

fn source_values(
    script: &TriggerScript,
    source_id: u32,
    kind: IteratorKind,
) -> Option<Vec<EntityId>> {
    let variable = script
        .get_variable(source_id)
        .filter(|variable| !variable.is_null)?;
    match (kind, &variable.value) {
        (IteratorKind::Unit, TriggerValue::UnitList(values))
        | (IteratorKind::Squad, TriggerValue::SquadList(values))
        | (IteratorKind::Object, TriggerValue::ObjectList(values)) => Some(values.clone()),
        _ => None,
    }
}

fn output_has_type(script: &TriggerScript, output_id: u32, kind: IteratorKind) -> bool {
    let expected = match kind {
        IteratorKind::Unit => VarType::Unit,
        IteratorKind::Squad => VarType::Squad,
        IteratorKind::Object => VarType::Object,
    };
    script
        .get_variable(output_id)
        .is_some_and(|variable| variable.var_type == expected)
}

fn next_scalar(
    condition: &Condition,
    script: &mut TriggerScript,
    kind: ScalarIteratorKind,
) -> bool {
    let Some(iterator_id) = condition.variable_id(1) else {
        return false;
    };
    let Some(iterator) = iterator_value(script, iterator_id) else {
        return false;
    };
    let Some(source_id) = iterator.source_list_id() else {
        return false;
    };
    let Some(source) = scalar_source_values(script, source_id, kind) else {
        return false;
    };
    let next = source.iter().copied().find(|value| match kind {
        ScalarIteratorKind::Player => !iterator.is_player_visited(*value),
        ScalarIteratorKind::Team => !iterator.is_team_visited(*value),
    });
    let Some(next) = next else {
        return false;
    };
    let Some(output_id) = condition.variable_id(2) else {
        return false;
    };
    if !scalar_output_has_type(script, output_id, kind) {
        return false;
    }

    let Some(iterator_variable) = script.get_variable_mut(iterator_id) else {
        return false;
    };
    let TriggerValue::Iterator(iterator) = &mut iterator_variable.value else {
        return false;
    };
    match kind {
        ScalarIteratorKind::Player => iterator.visit_player(next),
        ScalarIteratorKind::Team => iterator.visit_team(next),
    }

    let Some(output) = script.get_variable_mut(output_id) else {
        return false;
    };
    output.value = match kind {
        ScalarIteratorKind::Player => TriggerValue::Player(next),
        ScalarIteratorKind::Team => TriggerValue::Team(next),
    };
    output.is_null = false;
    true
}

fn iterator_value(
    script: &TriggerScript,
    iterator_id: u32,
) -> Option<crate::trigger::TriggerIterator> {
    script
        .get_variable(iterator_id)
        .filter(|variable| !variable.is_null)
        .and_then(|variable| match &variable.value {
            TriggerValue::Iterator(iterator) => Some(iterator.clone()),
            _ => None,
        })
}

fn scalar_source_values(
    script: &TriggerScript,
    source_id: u32,
    kind: ScalarIteratorKind,
) -> Option<Vec<i32>> {
    let variable = script
        .get_variable(source_id)
        .filter(|variable| !variable.is_null)?;
    match (kind, &variable.value) {
        (ScalarIteratorKind::Player, TriggerValue::PlayerList(values))
        | (ScalarIteratorKind::Team, TriggerValue::TeamList(values)) => Some(values.clone()),
        _ => None,
    }
}

fn vector_source_values(
    script: &TriggerScript,
    source_id: u32,
) -> Option<Vec<crate::trigger::TriggerVec3>> {
    let variable = script
        .get_variable(source_id)
        .filter(|variable| !variable.is_null)?;
    match &variable.value {
        TriggerValue::LocationList(values) | TriggerValue::VectorList(values) => {
            Some(values.clone())
        }
        _ => None,
    }
}

fn scalar_output_has_type(
    script: &TriggerScript,
    output_id: u32,
    kind: ScalarIteratorKind,
) -> bool {
    let expected = match kind {
        ScalarIteratorKind::Player => VarType::Player,
        ScalarIteratorKind::Team => VarType::Team,
    };
    script
        .get_variable(output_id)
        .is_some_and(|variable| variable.var_type == expected)
}
