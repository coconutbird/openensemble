//! Conditions backed by authoritative command state.

use super::{Condition, TriggerScript, TriggerValue, World, write_trigger_value};
use crate::EntityId;

pub(super) fn building_command_done(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let Some(TriggerValue::BuildingCommandState(state)) =
        super::value_at(condition, script, 1).cloned()
    else {
        return false;
    };
    let live_squads = state
        .trained_squads()
        .iter()
        .copied()
        .filter(|squad_id| world.get_squad(*squad_id).is_some())
        .collect::<Vec<_>>();

    write_optional(
        condition,
        script,
        2,
        TriggerValue::Squad(live_squads.last().copied().unwrap_or(EntityId::INVALID)),
    );
    write_optional(condition, script, 3, TriggerValue::SquadList(live_squads));
    state.is_done()
}

pub(super) fn custom_command_check(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let Some(command_id) = super::value_at(condition, script, 1).and_then(super::as_i32) else {
        return false;
    };
    let (finished, count) = if let Some(command) = world.custom_command(command_id) {
        (command.finished_count > 0, command.finished_count.max(0))
    } else if command_id >= 0 && command_id < world.next_custom_command_id() {
        (true, 1)
    } else {
        (false, 0)
    };
    write_optional(condition, script, 2, TriggerValue::Int(count));
    finished
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
