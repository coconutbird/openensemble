//! Retail command-state trigger effects.

use super::support::{bool_at, float_at, integer_at, used_variable_id};
use super::{EffectOutcome, TriggerValue, value_at, write_value};
use crate::entities::units::TriggerCommandStateRef;
use crate::player::Resources;
use crate::trigger::{Effect, TriggerScript};
use crate::world::{
    CustomCommand, CustomCommandFlags, ResearchQueueResult, TriggerTrainingRequest, World,
};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerBuildingWork {
    TrainSquads { prototype_id: i32, count: u32 },
    Research { technology_id: i32 },
    BuildOther { prototype_id: i32 },
}

pub(super) fn building_command(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Skipped;
    }
    let state_ref = begin_building_command_state(effect, script);
    let Some(database) = database else {
        finish_building_command_state(script, state_ref);
        return EffectOutcome::Skipped;
    };
    let Some(building_id) = value_at(effect, script, 1).and_then(TriggerValue::as_entity) else {
        finish_building_command_state(script, state_ref);
        return EffectOutcome::Applied;
    };
    let player_id = if effect.version >= 4 {
        super::support::player_at(effect, script, 8)
    } else {
        None
    }
    .or_else(|| {
        world
            .get_building(building_id)
            .map(|building| building.base.player_id)
    });
    let Some(player_id) = player_id else {
        finish_building_command_state(script, state_ref);
        return EffectOutcome::Applied;
    };
    let no_cost = bool_at(effect, script, 4).unwrap_or(false);
    let waiting = match building_work(effect, script) {
        Some(TriggerBuildingWork::TrainSquads {
            prototype_id,
            count,
        }) => world
            .queue_trigger_training(TriggerTrainingRequest {
                player_id,
                building_id,
                database,
                prototype_id,
                count,
                no_cost,
                trigger_state: state_ref,
            })
            .is_ok_and(|result| result.accepted > 0),
        Some(TriggerBuildingWork::Research { technology_id }) => world
            .queue_trigger_research(
                player_id,
                building_id,
                database,
                technology_id,
                no_cost,
                state_ref,
            )
            .is_ok_and(|result| result == ResearchQueueResult::Queued),
        Some(TriggerBuildingWork::BuildOther { prototype_id }) => world
            .queue_trigger_build_other(
                player_id,
                building_id,
                database,
                prototype_id,
                no_cost,
                state_ref,
            )
            .is_ok(),
        None => false,
    };
    if !waiting {
        finish_building_command_state(script, state_ref);
    }
    EffectOutcome::Applied
}

fn building_work(effect: &Effect, script: &TriggerScript) -> Option<TriggerBuildingWork> {
    if let Some(prototype_id) = nonnegative_integer_at(effect, script, 2) {
        let count = integer_at(effect, script, 3).unwrap_or(1);
        return Some(TriggerBuildingWork::TrainSquads {
            prototype_id,
            count: u32::try_from(count).unwrap_or_default(),
        });
    }
    if let Some(technology_id) = nonnegative_integer_at(effect, script, 6) {
        return Some(TriggerBuildingWork::Research { technology_id });
    }
    (effect.version >= 4)
        .then(|| nonnegative_integer_at(effect, script, 7))
        .flatten()
        .map(|prototype_id| TriggerBuildingWork::BuildOther { prototype_id })
}

fn nonnegative_integer_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<i32> {
    integer_at(effect, script, signature_id).filter(|value| *value >= 0)
}

fn begin_building_command_state(
    effect: &Effect,
    script: &mut TriggerScript,
) -> Option<TriggerCommandStateRef> {
    let variable_id = used_variable_id(effect, script, 5)?;
    let variable = script.get_variable_mut(variable_id)?;
    let TriggerValue::BuildingCommandState(state) = &mut variable.value else {
        return None;
    };
    state.begin();
    variable.is_null = false;
    Some(TriggerCommandStateRef::new(script.id, variable_id))
}

fn finish_building_command_state(
    script: &mut TriggerScript,
    state_ref: Option<TriggerCommandStateRef>,
) {
    let Some(state_ref) = state_ref else {
        return;
    };
    let Some(variable) = script.get_variable_mut(state_ref.variable_id) else {
        return;
    };
    if let TriggerValue::BuildingCommandState(state) = &mut variable.value {
        state.finish();
        variable.is_null = false;
    }
}

pub(super) fn clear_building_command_state(
    effect: &Effect,
    script: &mut TriggerScript,
) -> EffectOutcome {
    let Some(variable_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(variable) = script.get_variable_mut(variable_id) else {
        return EffectOutcome::Skipped;
    };
    let TriggerValue::BuildingCommandState(state) = &mut variable.value else {
        return EffectOutcome::Skipped;
    };
    state.begin();
    state.finish();
    variable.is_null = false;
    EffectOutcome::Applied
}

pub(super) fn custom_command_add(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 17) else {
        return EffectOutcome::Skipped;
    };
    let Some(unit_id) = custom_command_unit(effect, script, world) else {
        return write_value(script, output_id, TriggerValue::Int(-1));
    };
    let Some(icon_position) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let command = CustomCommand {
        unit_id,
        icon_position,
        icon_name: string_at(effect, script, 3),
        cost: cost_at(effect, script, 4).unwrap_or_default(),
        timer_seconds: float_at(effect, script, 5).unwrap_or(0.0),
        limit: integer_at(effect, script, 6).unwrap_or(0),
        name_string_id: integer_at(effect, script, 8).unwrap_or(-1),
        info_string_id: integer_at(effect, script, 9).unwrap_or(-1),
        help_string_id: integer_at(effect, script, 10).unwrap_or(-1),
        flags: CustomCommandFlags::default()
            .with_queue(bool_at(effect, script, 11).unwrap_or(false))
            .with_allow_multiple(bool_at(effect, script, 12).unwrap_or(false))
            .with_show_limit(bool_at(effect, script, 13).unwrap_or(false))
            .with_close_menu(bool_at(effect, script, 14).unwrap_or(false))
            .with_persistent(bool_at(effect, script, 15).unwrap_or(false))
            .with_unavailable(bool_at(effect, script, 16).unwrap_or(false))
            .with_allow_cancel(bool_at(effect, script, 18).unwrap_or(false)),
        ..CustomCommand::default()
    };
    let command_id = world.add_custom_command(command);
    write_value(script, output_id, TriggerValue::Int(command_id))
}

pub(super) fn custom_command_remove(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(command_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let _removed = world.remove_custom_command(command_id);
    EffectOutcome::Applied
}

fn custom_command_unit(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
) -> Option<crate::EntityId> {
    let entity_id = value_at(effect, script, 1)?.as_entity()?;
    match effect.version {
        1 => world
            .get_squad(entity_id)?
            .unit_ids
            .iter()
            .copied()
            .find(|unit_id| world.get_unit(*unit_id).is_some()),
        2 => Some(entity_id),
        _ => None,
    }
}

fn string_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<String> {
    match value_at(effect, script, slot)? {
        TriggerValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn cost_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<Resources> {
    let TriggerValue::Cost(value) = value_at(effect, script, slot)? else {
        return None;
    };
    Some(Resources {
        amounts: value.amounts(),
    })
}

#[cfg(test)]
mod tests;
