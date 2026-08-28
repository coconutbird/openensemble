//! Retail hitch relationship trigger conditions.

use super::{Condition, TriggerScript, TriggerValue, World, write_trigger_value};
use crate::EntityId;

pub(super) fn is_hitched(condition: &Condition, script: &mut TriggerScript, world: &World) -> bool {
    let Some(squad_id) = super::value_at(condition, script, 2).and_then(as_squad_id) else {
        return false;
    };
    let towing_id = world
        .get_squad(squad_id)
        .and_then(crate::entities::Squad::hitched_to_squad);
    if let Some(towing_id) = towing_id {
        write_optional(condition, script, 3, TriggerValue::Squad(towing_id));
        true
    } else {
        false
    }
}

pub(super) fn has_hitched(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let trailer_id = super::value_at(condition, script, 1)
        .and_then(as_squad_id)
        .and_then(|squad_id| world.get_squad(squad_id))
        .and_then(crate::entities::Squad::hitched_squad);
    write_optional(
        condition,
        script,
        2,
        TriggerValue::Squad(trailer_id.unwrap_or(EntityId::INVALID)),
    );
    trailer_id.is_some()
}

fn as_squad_id(value: &TriggerValue) -> Option<EntityId> {
    let TriggerValue::Squad(squad_id) = value else {
        return None;
    };
    (!squad_id.is_invalid()).then_some(*squad_id)
}

fn write_optional(
    condition: &Condition,
    script: &mut TriggerScript,
    signature_id: u16,
    value: TriggerValue,
) {
    let Some(variable_id) = condition.variable_id(signature_id) else {
        return;
    };
    if script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
    {
        write_trigger_value(script, variable_id, value);
    }
}

#[cfg(test)]
mod tests;
