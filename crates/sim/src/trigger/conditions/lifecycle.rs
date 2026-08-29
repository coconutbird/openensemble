//! Retail entity lifecycle and idle-action conditions.

use super::{entity_at, entity_inputs, write_time};
use crate::entities::UnitState;
use crate::entity_id::EntityId;
use crate::trigger::{Condition, TriggerScript};
use crate::world::World;

pub(super) fn entity_liveness(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    expected_alive: bool,
) -> bool {
    let entities = entity_inputs(condition, script);
    if expected_alive {
        entities.iter().any(|entity_id| is_alive(world, *entity_id))
    } else {
        entities
            .iter()
            .all(|entity_id| !is_alive(world, *entity_id))
    }
}

pub(super) fn is_built(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    if let Some(unit_id) = entity_at(condition, script, 1) {
        return world.get_unit(unit_id).is_some_and(|unit| unit.built);
    }
    let Some(squad_id) = entity_at(condition, script, 2) else {
        return false;
    };
    world.get_squad(squad_id).is_some_and(|squad| {
        squad
            .unit_ids
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).is_none_or(|unit| unit.built))
    })
}

pub(super) fn is_squad_at_max_size(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    entity_at(condition, script, 1).is_some_and(|squad_id| world.squad_is_at_max_size(squad_id))
}

pub(super) fn is_moving(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    if let Some(unit_id) = entity_at(condition, script, 1) {
        return world
            .get_unit(unit_id)
            .is_some_and(|unit| unit.state == UnitState::Moving);
    }
    entity_at(condition, script, 2)
        .and_then(|squad_id| world.get_squad(squad_id))
        .is_some_and(crate::entities::Squad::is_moving)
}

pub(super) fn is_idle(condition: &Condition, script: &mut TriggerScript, world: &World) -> bool {
    let output_id = used_variable_id(condition, script, 3);
    if variable_is_used(condition, script, 1)
        && let Some(unit) = entity_at(condition, script, 1).and_then(|id| world.get_unit(id))
    {
        if let Some(output_id) = output_id {
            write_time(script, output_id, unit.idle_duration());
        }
        return unit.has_idle_action();
    }
    if variable_is_used(condition, script, 2)
        && let Some(squad) = entity_at(condition, script, 2).and_then(|id| world.get_squad(id))
    {
        if let Some(output_id) = output_id {
            write_time(script, output_id, squad.idle_duration());
        }
        return squad.has_idle_action();
    }
    if let Some(output_id) = output_id {
        write_time(script, output_id, 0);
    }
    false
}

fn variable_is_used(condition: &Condition, script: &TriggerScript, signature_id: u16) -> bool {
    used_variable_id(condition, script, signature_id).is_some()
}

fn used_variable_id(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<u32> {
    let variable_id = condition.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
        .then_some(variable_id)
}

fn is_alive(world: &World, entity_id: EntityId) -> bool {
    world.is_entity_trigger_alive(entity_id)
}
