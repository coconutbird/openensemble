//! Retail hint-callout creation and destruction effects.

use super::super::support::{integer_at, variable_is_used, vector_at};
use super::super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn create(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(output_id) = effect.variable_id(4) else {
        return EffectOutcome::Skipped;
    };
    if script.get_variable(output_id).is_none() {
        return EffectOutcome::Skipped;
    }
    let Some(string_id) = integer_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };

    let callout_id = if variable_is_used(effect, script, 5) {
        entity_at(effect, script, 5).map_or(-1, |entity_id| {
            world.create_entity_hint_callout(entity_id, string_id, false)
        })
    } else if variable_is_used(effect, script, 6) {
        entity_at(effect, script, 6).map_or(-1, |entity_id| {
            world.create_entity_hint_callout(entity_id, string_id, true)
        })
    } else if variable_is_used(effect, script, 3) {
        vector_at(effect, script, 3).map_or(-1, |location| {
            world.create_location_hint_callout(location, string_id)
        })
    } else {
        -1
    };

    if write_value(script, output_id, TriggerValue::Int(callout_id)) == EffectOutcome::Skipped {
        EffectOutcome::Skipped
    } else {
        EffectOutcome::Presentation
    }
}

pub(super) fn destroy(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(callout_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let _removed = world.remove_hint_callout(callout_id);
    EffectOutcome::Presentation
}

fn entity_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    value_at(effect, script, slot).and_then(TriggerValue::as_entity)
}

#[cfg(test)]
#[path = "callouts/tests.rs"]
mod tests;
