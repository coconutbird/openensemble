//! Retail game-timer completion query.

use super::value_at;
use crate::trigger::{Condition, TriggerScript, TriggerValue};
use crate::world::World;

pub(super) fn is_done(condition: &Condition, script: &mut TriggerScript, world: &World) -> bool {
    let timer_id = match value_at(condition, script, 1) {
        Some(TriggerValue::Int(value)) => Some(*value),
        _ => None,
    };
    let (current_time_ms, done) = timer_id
        .and_then(|timer_id| world.game_timer(timer_id))
        .map_or((0, false), |timer| {
            (timer.current_time_ms(), timer.is_done())
        });
    write_optional_current_time(condition, script, current_time_ms);
    done
}

fn write_optional_current_time(
    condition: &Condition,
    script: &mut TriggerScript,
    current_time_ms: u32,
) {
    let Some(variable_id) = condition.variable_id(2) else {
        return;
    };
    let Some(variable) = script
        .get_variable_mut(variable_id)
        .filter(|variable| !variable.is_null)
    else {
        return;
    };
    variable.value = TriggerValue::Time(current_time_ms);
    variable.is_null = false;
}

#[cfg(test)]
#[path = "timers/tests.rs"]
mod tests;
