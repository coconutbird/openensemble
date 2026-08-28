//! Retail live per-unit data-scalar mutations.

use super::support::{
    EntityListKind, bool_at, float_at, scalar_and_list, unique_add, variable_is_used,
};
use super::{EffectOutcome, value_at};
use crate::entities::UnitDataScalar;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn modify_data_scalar(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !(1..=4).any(|signature_id| variable_is_used(effect, script, signature_id)) {
        return EffectOutcome::Skipped;
    }
    let scalar_signature = if effect.version == 1 { 5 } else { 8 };
    let Some(scalar) = data_scalar_at(effect, script, scalar_signature) else {
        return EffectOutcome::Skipped;
    };
    let Some(value) = float_at(effect, script, 6) else {
        return EffectOutcome::Skipped;
    };
    let adjust = bool_at(effect, script, 7).unwrap_or(false);
    let unit_ids = selected_units(effect, script, world);

    // DamageTaken was added by version 2. Retail version 1 simply has no
    // switch arm for that selector.
    if effect.version == 1 && scalar == UnitDataScalar::DamageTaken {
        return EffectOutcome::Applied;
    }
    for unit_id in unit_ids {
        if let Some(unit) = world.get_unit_mut(unit_id) {
            unit.modify_data_scalar(scalar, value, adjust);
        }
    }
    EffectOutcome::Applied
}

fn selected_units(effect: &Effect, script: &TriggerScript, world: &World) -> Vec<EntityId> {
    let mut units = scalar_and_list(effect, script, 1, 2, EntityListKind::Unit);
    for squad_id in scalar_and_list(effect, script, 3, 4, EntityListKind::Squad) {
        let Some(children) = world
            .get_squad(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            continue;
        };
        for unit_id in children {
            unique_add(&mut units, unit_id);
        }
    }
    units
}

fn data_scalar_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<UnitDataScalar> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::String(value) => UnitDataScalar::from_trigger_value(value),
        TriggerValue::Int(value) => UnitDataScalar::from_trigger_value(&value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
