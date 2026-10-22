//! Retail scenario design-line lookup and list effects.

use super::support::used_variable_id;
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, TriggerVec3, VarId};
use crate::world::World;

#[derive(Debug, Clone, Copy)]
struct InvalidBinding;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::DesignLineGetPoints => get_points(effect, script, world),
        EffectType::DesignLineListAdd => list_add(effect, script),
        EffectType::DesignLineListRemove => list_remove(effect, script),
        EffectType::DesignLineListGetSize => list_size(effect, script),
        _ => return None,
    };
    Some(outcome)
}

fn get_points(effect: &Effect, script: &mut TriggerScript, world: &World) -> EffectOutcome {
    let Some(line_id) = design_line_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(destination_id) = vector_list_destination(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let points = world
        .design_line_points(line_id)
        .unwrap_or_default()
        .iter()
        .map(|point| TriggerVec3::new(point.x, point.y, point.z))
        .collect();
    write_value(script, destination_id, TriggerValue::VectorList(points))
}

fn list_add(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(destination_id) = destination_list_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(single) = optional_design_line(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Ok(additions) = optional_design_line_list(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Ok(clear) = optional_bool(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(destination) = design_line_list_mut(script, destination_id) else {
        return EffectOutcome::Skipped;
    };
    if clear.unwrap_or(false) {
        destination.clear();
    }
    destination.extend(single);
    destination.extend(additions.unwrap_or_default());
    EffectOutcome::Applied
}

fn list_remove(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(destination_id) = destination_list_id(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let Ok(remove_all) = optional_bool(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    if remove_all.unwrap_or(false) {
        let Some(destination) = design_line_list_mut(script, destination_id) else {
            return EffectOutcome::Skipped;
        };
        destination.clear();
        return EffectOutcome::Applied;
    }
    let Ok(remove_duplicates) = optional_bool(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(single) = optional_design_line(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Ok(removals) = optional_design_line_list(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(destination) = design_line_list_mut(script, destination_id) else {
        return EffectOutcome::Skipped;
    };
    for line_id in single.into_iter().chain(removals.unwrap_or_default()) {
        if remove_duplicates.unwrap_or(false) {
            destination.retain(|candidate| *candidate != line_id);
        } else {
            remove_first(destination, line_id);
        }
    }
    EffectOutcome::Applied
}

fn list_size(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(size) =
        design_line_list_at(effect, script, 1).and_then(|values| i32::try_from(values.len()).ok())
    else {
        return EffectOutcome::Skipped;
    };
    let Some(destination_id) = integer_destination(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, destination_id, TriggerValue::Int(size))
}

fn design_line_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<i32> {
    match value_at(effect, script, slot)? {
        TriggerValue::DesignLine(value) => Some(*value),
        _ => None,
    }
}

fn design_line_list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    slot: u16,
) -> Option<&'a Vec<i32>> {
    match value_at(effect, script, slot)? {
        TriggerValue::DesignLineList(values) => Some(values),
        _ => None,
    }
}

fn optional_design_line(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
) -> Result<Option<i32>, InvalidBinding> {
    let Some(variable_id) = effect.variable_id(slot) else {
        return Ok(None);
    };
    let variable = script.get_variable(variable_id).ok_or(InvalidBinding)?;
    if variable.is_null {
        return Ok(None);
    }
    match variable.value {
        TriggerValue::DesignLine(value) => Ok(Some(value)),
        _ => Err(InvalidBinding),
    }
}

/// Read an optional design-line list input as a copy.
///
/// The input can be the destination list, so the caller must read it before the change.
fn optional_design_line_list(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
) -> Result<Option<Vec<i32>>, InvalidBinding> {
    let Some(variable_id) = effect.variable_id(slot) else {
        return Ok(None);
    };
    let variable = script.get_variable(variable_id).ok_or(InvalidBinding)?;
    if variable.is_null {
        return Ok(None);
    }
    match &variable.value {
        TriggerValue::DesignLineList(values) => Ok(Some(values.clone())),
        _ => Err(InvalidBinding),
    }
}

fn optional_bool(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
) -> Result<Option<bool>, InvalidBinding> {
    let Some(variable_id) = effect.variable_id(slot) else {
        return Ok(None);
    };
    let variable = script.get_variable(variable_id).ok_or(InvalidBinding)?;
    if variable.is_null {
        return Ok(None);
    }
    match variable.value {
        TriggerValue::Bool(value) => Ok(Some(value)),
        _ => Err(InvalidBinding),
    }
}

fn destination_list_id(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, slot)?;
    matches!(
        script.get_variable(variable_id)?.value,
        TriggerValue::DesignLineList(_)
    )
    .then_some(variable_id)
}

fn design_line_list_mut(script: &mut TriggerScript, variable_id: VarId) -> Option<&mut Vec<i32>> {
    match &mut script.get_variable_mut(variable_id)?.value {
        TriggerValue::DesignLineList(values) => Some(values),
        _ => None,
    }
}

fn vector_list_destination(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, slot)?;
    matches!(
        script.get_variable(variable_id)?.value,
        TriggerValue::VectorList(_)
    )
    .then_some(variable_id)
}

fn integer_destination(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, slot)?;
    matches!(
        script.get_variable(variable_id)?.value,
        TriggerValue::Int(_)
    )
    .then_some(variable_id)
}

fn remove_first(values: &mut Vec<i32>, target: i32) {
    if let Some(index) = values.iter().position(|value| *value == target) {
        values.remove(index);
    }
}

#[cfg(test)]
mod tests;
