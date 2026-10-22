//! Retail conditions that read the externals of their own script.

use super::{used_variable_id, write_trigger_value};
use crate::trigger::{Condition, TriggerScript, TriggerValue};

/// `CanRetrieveExternals` (192), versions 2 and 3 (`BTriggerCondition_tcCanRetrieveExternals`).
///
/// For each non-null output, copy the external if the launcher set it. Otherwise the result is
/// false, but the other outputs still get their values. The outputs are: 1 player, 2 power,
/// 3 squad, 7 squad list, 4 unit, 6 unit list and 5 cost, in this order.
pub(super) fn can_retrieve_externals(condition: &Condition, script: &mut TriggerScript) -> bool {
    if !matches!(condition.version, 2 | 3) {
        return false;
    }
    let externals = script.externals.clone();
    let outputs = [
        (1, externals.player.map(TriggerValue::Player)),
        (2, externals.power.map(TriggerValue::Int)),
        (3, externals.squad.map(TriggerValue::Squad)),
        (7, externals.squad_list.map(TriggerValue::SquadList)),
        (4, externals.unit.map(TriggerValue::Unit)),
        (6, externals.unit_list.map(TriggerValue::UnitList)),
        (5, externals.cost.map(TriggerValue::Cost)),
    ];
    let mut all_set = true;
    for (signature_id, value) in outputs {
        let Some(variable_id) = used_variable_id(condition, script, signature_id) else {
            continue;
        };
        if let Some(value) = value {
            write_trigger_value(script, variable_id, value);
        } else {
            all_set = false;
        }
    }
    all_set
}

/// `CanRetrieveExternalLocation` (440, `BTriggerCondition_tcCanRetrieveExternalLocation`).
///
/// If the launcher set the location, write it to signature 1 and return true. Retail writes the
/// output without a null check.
pub(super) fn can_retrieve_external_location(
    condition: &Condition,
    script: &mut TriggerScript,
) -> bool {
    let Some(location) = script.externals.location else {
        return false;
    };
    if let Some(variable_id) = condition.variable_id(1) {
        write_trigger_value(script, variable_id, TriggerValue::Vector(location));
    }
    true
}

/// `CanRetrieveExternalFloat` (977, `BTriggerCondition_tcCanRetrieveExternalFloat`).
///
/// If the launcher set the float, write it to signature 1 and return true. Retail writes the
/// output without a null check.
pub(super) fn can_retrieve_external_float(
    condition: &Condition,
    script: &mut TriggerScript,
) -> bool {
    let Some(value) = script.externals.float else {
        return false;
    };
    if let Some(variable_id) = condition.variable_id(1) {
        write_trigger_value(script, variable_id, TriggerValue::Float(value));
    }
    true
}

#[cfg(test)]
mod tests;
