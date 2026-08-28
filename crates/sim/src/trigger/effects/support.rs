//! Shared retail trigger-signature readers.

use super::value_at;
use crate::EntityId;
use crate::trigger::{Effect, TriggerScript, TriggerValue, VarId};
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntityListKind {
    Unit,
    Squad,
    Object,
}

pub(super) fn entities_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<Vec<EntityId>> {
    let value = value_at(effect, script, signature_id)?;
    let entities = match (kind, value) {
        (EntityListKind::Unit, TriggerValue::Unit(value))
        | (EntityListKind::Squad, TriggerValue::Squad(value))
        | (EntityListKind::Object, TriggerValue::Object(value)) => vec![*value],
        (EntityListKind::Unit, TriggerValue::UnitList(values))
        | (EntityListKind::Squad, TriggerValue::SquadList(values))
        | (EntityListKind::Object, TriggerValue::ObjectList(values)) => values.clone(),
        _ => Vec::new(),
    };
    Some(entities)
}

pub(super) fn combine_entities(
    first: Option<Vec<EntityId>>,
    second: Option<Vec<EntityId>>,
) -> Vec<EntityId> {
    let mut combined = first.unwrap_or_default();
    for entity_id in second.unwrap_or_default() {
        unique_add(&mut combined, entity_id);
    }
    combined
}

pub(super) fn scalar_and_list(
    effect: &Effect,
    script: &TriggerScript,
    scalar_signature_id: u16,
    list_signature_id: u16,
    kind: EntityListKind,
) -> Vec<EntityId> {
    combine_entities(
        entities_at(effect, script, list_signature_id, kind),
        entities_at(effect, script, scalar_signature_id, kind),
    )
}

pub(super) fn unique_add<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

pub(super) fn used_variable_id(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<VarId> {
    let variable_id = effect.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
        .then_some(variable_id)
}

pub(super) fn variable_is_used(effect: &Effect, script: &TriggerScript, signature_id: u16) -> bool {
    used_variable_id(effect, script, signature_id).is_some()
}

pub(super) fn integer_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<i32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Int(value)
        | TriggerValue::Player(value)
        | TriggerValue::ProtoObject(value)
        | TriggerValue::ProtoSquad(value)
        | TriggerValue::Tech(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn player_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u8> {
    integer_at(effect, script, signature_id).and_then(|value| u8::try_from(value).ok())
}

pub(super) fn vector_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec3> {
    value_at(effect, script, signature_id)
        .and_then(TriggerValue::as_location)
        .map(|value| Vec3::new(value.x, value.y, value.z))
}

pub(super) fn bool_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<bool> {
    value_at(effect, script, signature_id).and_then(TriggerValue::as_bool)
}

pub(super) fn float_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<f32> {
    value_at(effect, script, signature_id).and_then(TriggerValue::as_float)
}

pub(super) fn object_type_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<String> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::ObjectType(value) => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn entity_list(kind: EntityListKind, value: &TriggerValue) -> Option<&Vec<EntityId>> {
    match (kind, value) {
        (EntityListKind::Unit, TriggerValue::UnitList(values))
        | (EntityListKind::Squad, TriggerValue::SquadList(values))
        | (EntityListKind::Object, TriggerValue::ObjectList(values)) => Some(values),
        _ => None,
    }
}

pub(super) fn entity_list_mut(
    kind: EntityListKind,
    value: &mut TriggerValue,
) -> Option<&mut Vec<EntityId>> {
    match (kind, value) {
        (EntityListKind::Unit, TriggerValue::UnitList(values))
        | (EntityListKind::Squad, TriggerValue::SquadList(values))
        | (EntityListKind::Object, TriggerValue::ObjectList(values)) => Some(values),
        _ => None,
    }
}
