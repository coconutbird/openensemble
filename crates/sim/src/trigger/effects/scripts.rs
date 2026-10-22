//! Retail `LaunchScript` (392): the request to start a trigger script from a file.

use super::{ControlAction, EffectOutcome, ScriptLaunch, value_at};
use crate::trigger::{Effect, TriggerExternals, TriggerScript, TriggerValue};

/// Read a `LaunchScript` request (`BTriggerEffect_teLaunchScriptV4`).
///
/// Only version 4 does work. Signature 15 is the file name. Each non-null input sets one
/// external: 2 unit, 3 unit list, 4 squad, 5 squad list, 6 player, 7 power, 8 cost,
/// 11 location, 12 location list and 13 float.
pub(super) fn launch(
    effect: &Effect,
    script: &TriggerScript,
) -> (EffectOutcome, Option<ControlAction>) {
    if effect.version != 4 {
        return (EffectOutcome::Unsupported(effect.raw_type), None);
    }
    // Retail reads the file name without a null check.
    let Some(file_name) = effect
        .variable_id(15)
        .and_then(|variable_id| script.get_variable(variable_id))
        .and_then(|variable| match &variable.value {
            TriggerValue::String(value) => Some(value.clone()),
            _ => None,
        })
    else {
        return (EffectOutcome::Skipped, None);
    };
    let externals = TriggerExternals {
        location: value_at(effect, script, 11).and_then(TriggerValue::as_location),
        location_list: match value_at(effect, script, 12) {
            Some(TriggerValue::LocationList(values) | TriggerValue::VectorList(values)) => {
                Some(values.clone())
            }
            _ => None,
        },
        squad_list: match value_at(effect, script, 5) {
            Some(TriggerValue::SquadList(values)) => Some(values.clone()),
            _ => None,
        },
        unit_list: match value_at(effect, script, 3) {
            Some(TriggerValue::UnitList(values)) => Some(values.clone()),
            _ => None,
        },
        cost: match value_at(effect, script, 8) {
            Some(TriggerValue::Cost(value)) => Some(*value),
            _ => None,
        },
        float: value_at(effect, script, 13).and_then(TriggerValue::as_float),
        squad: value_at(effect, script, 4).and_then(TriggerValue::as_entity),
        unit: value_at(effect, script, 2).and_then(TriggerValue::as_entity),
        player: match value_at(effect, script, 6) {
            Some(TriggerValue::Player(value) | TriggerValue::Int(value)) => Some(*value),
            _ => None,
        },
        power: match value_at(effect, script, 7) {
            Some(TriggerValue::Int(value)) => Some(*value),
            _ => None,
        },
    };
    (
        EffectOutcome::Applied,
        Some(ControlAction::Launch(ScriptLaunch {
            file_name,
            externals,
        })),
    )
}
