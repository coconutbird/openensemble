//! Authoritative implementations of supported retail trigger effects.

mod ai;
mod commands;
mod dispatch;
mod economy;
mod entities;
mod entity_flags;
mod events;
mod filters;
mod game_state;
mod health;
mod iterators;
mod list_processing;
mod lists;
mod math;
mod orders;
mod ownership;
mod powers;
mod proto_data;
mod relationships;
mod resources;
mod spatial;
mod support;
mod unit_data;
mod value_lists;

pub(super) use dispatch::execute_effect;

use super::{Effect, EffectType, TriggerId, TriggerScript, TriggerValue, VarId};
use crate::gameplay::GameplayCatalog;
use crate::world::World;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EffectOutcome {
    Applied,
    Presentation,
    Skipped,
    Unsupported(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlAction {
    Activate(TriggerId),
    Deactivate(TriggerId),
}

fn execute_control_effect(
    effect: &Effect,
    script: &TriggerScript,
) -> Option<(EffectOutcome, Option<ControlAction>)> {
    let control = match effect.effect_type {
        EffectType::TriggerActivate => read_trigger(effect, script).map(ControlAction::Activate),
        EffectType::TriggerDeactivate => {
            read_trigger(effect, script).map(ControlAction::Deactivate)
        }
        _ => return None,
    };
    let outcome = if control.is_some() {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    };
    Some((outcome, control))
}

fn read_trigger(effect: &Effect, script: &TriggerScript) -> Option<TriggerId> {
    let variable_id = effect.variable_id(1)?;
    match &script.get_variable(variable_id)?.value {
        TriggerValue::Trigger(trigger_id) => Some(*trigger_id),
        _ => None,
    }
}

fn adjust_count(effect: &Effect, script: &mut TriggerScript, adjustment: i32) -> EffectOutcome {
    let Some(source_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let Some(destination_id) = effect.variable_id(2) else {
        return EffectOutcome::Skipped;
    };
    let Some(value) = script
        .get_variable(source_id)
        .and_then(|variable| variable.value.as_int())
    else {
        return EffectOutcome::Skipped;
    };
    write_value(
        script,
        destination_id,
        TriggerValue::Int(value.wrapping_add(adjustment)),
    )
}

fn copy_value(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(source_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let Some(destination_id) = effect.variable_id(2) else {
        return EffectOutcome::Skipped;
    };
    let Some(value) = script
        .get_variable(source_id)
        .filter(|variable| !variable.is_null)
        .map(|variable| variable.value.clone())
    else {
        return EffectOutcome::Skipped;
    };
    write_value(script, destination_id, value)
}

pub(super) fn write_value(
    script: &mut TriggerScript,
    destination_id: VarId,
    value: TriggerValue,
) -> EffectOutcome {
    let Some(destination) = script.get_variable_mut(destination_id) else {
        return EffectOutcome::Skipped;
    };
    destination.value = value;
    destination.is_null = false;
    EffectOutcome::Applied
}

pub(super) fn value_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a TriggerValue> {
    let variable_id = effect.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .filter(|variable| !variable.is_null)
        .map(|variable| &variable.value)
}

fn set_teleporter(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(source) = read_entity(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(target) = read_entity(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    if world.get_squad(target).is_none() {
        return EffectOutcome::Skipped;
    }
    let Some(source_squad) = world.get_squad_mut(source) else {
        return EffectOutcome::Skipped;
    };
    source_squad.set_teleporter_destination(target);
    EffectOutcome::Applied
}

fn read_entity(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<crate::EntityId> {
    let variable_id = effect.variable_id(signature_id)?;
    script.get_variable(variable_id)?.value.as_entity()
}

fn is_copy_effect(raw_type: u16) -> bool {
    matches!(
        raw_type,
        85..=103
            | 142..=145
            | 168
            | 169
            | 261
            | 360
            | 455
            | 481..=484
            | 501
            | 562
            | 609
            | 818
            | 921
    )
}
