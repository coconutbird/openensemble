//! Retail objective counter mutation and query effects.

use super::support::{used_variable_id, variable_is_used};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, EffectType, ObjectiveId, TriggerScript, TriggerValue};
use crate::world::World;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::ObjectiveIncrementCounter => adjust_counter(effect, script, world, true),
        EffectType::ObjectiveDecrementCounter => adjust_counter(effect, script, world, false),
        EffectType::ObjectiveGetCurrentCounter => query_counter(effect, script, world, false),
        EffectType::ObjectiveGetFinalCounter => query_counter(effect, script, world, true),
        _ => return None,
    };
    Some(outcome)
}

fn adjust_counter(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    increment: bool,
) -> EffectOutcome {
    let Some(objective_id) = objective_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let amount = if variable_is_used(effect, script, 2) {
        let Some(amount) = count_at(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        amount
    } else {
        1
    };
    let current = world.objective_current_count(objective_id);
    let next = if increment {
        current
            .max(0)
            .wrapping_add(amount)
            .min(world.objective_final_count(objective_id))
    } else {
        current.wrapping_sub(amount).max(0)
    };
    let _found = world.set_objective_current_count(objective_id, next);
    write_optional_current_count(effect, script, next)
}

fn query_counter(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    final_count: bool,
) -> EffectOutcome {
    let Some(objective_id) = objective_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let count = if final_count {
        world.objective_final_count(objective_id)
    } else {
        world.objective_current_count(objective_id)
    };
    write_value(script, output_id, TriggerValue::Int(count))
}

fn write_optional_current_count(
    effect: &Effect,
    script: &mut TriggerScript,
    count: i32,
) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Applied;
    };
    write_value(script, output_id, TriggerValue::Int(count))
}

fn objective_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<ObjectiveId> {
    let TriggerValue::Objective(objective_id) = value_at(effect, script, signature_id)? else {
        return None;
    };
    Some(*objective_id)
}

fn count_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<i32> {
    let TriggerValue::Int(count) = value_at(effect, script, signature_id)? else {
        return None;
    };
    Some(*count)
}

#[cfg(test)]
#[path = "objectives/tests.rs"]
mod tests;
