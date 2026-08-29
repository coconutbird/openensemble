//! Retail entity highlighting and fog-memory policy effects.

use super::support::{EntityListKind, scalar_and_list, unique_add, variable_is_used};
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, EffectType, TriggerColor, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::FlashEntity => flash_entity(effect, script, world),
        EffectType::ResetDopple => reset_dopple(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

fn flash_entity(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let (Some(interval_ms), Some(duration_ms), Some(color)) = (
        time_at(effect, script, 7),
        time_at(effect, script, 8),
        color_at(effect, script, 9),
    ) else {
        return EffectOutcome::Skipped;
    };
    let intensity = if effect.version > 1 && variable_is_used(effect, script, 10) {
        let Some(intensity) = float_at(effect, script, 10) else {
            return EffectOutcome::Skipped;
        };
        intensity
    } else {
        20.0
    };
    let targets = object_targets(effect, script, world);
    for entity_id in targets {
        let _changed = world.flash_entity(
            entity_id,
            interval_ms,
            duration_ms,
            [color.r, color.g, color.b, color.a],
            intensity,
        );
    }
    EffectOutcome::Applied
}

fn reset_dopple(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let (Some(gray_map_dopples), Some(dopples)) =
        (bool_at(effect, script, 7), bool_at(effect, script, 8))
    else {
        return EffectOutcome::Skipped;
    };
    let targets = object_targets(effect, script, world);
    for entity_id in targets {
        let _changed = world.reset_entity_dopples(entity_id, gray_map_dopples, dopples);
    }
    EffectOutcome::Applied
}

fn object_targets(effect: &Effect, script: &TriggerScript, world: &World) -> Vec<EntityId> {
    let mut references = scalar_and_list(effect, script, 5, 6, EntityListKind::Object);
    for entity_id in scalar_and_list(effect, script, 1, 2, EntityListKind::Unit) {
        references.push(entity_id);
    }
    let mut squad_references = scalar_and_list(effect, script, 3, 4, EntityListKind::Squad);
    let mut targets = Vec::new();
    for entity_id in references {
        if world.get_squad(entity_id).is_some() {
            unique_add(&mut squad_references, entity_id);
        } else if world.entity_object_state(entity_id).is_some() {
            unique_add(&mut targets, entity_id);
        }
    }
    for squad_id in squad_references {
        let Some(unit_ids) = world
            .get_squad(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            continue;
        };
        for unit_id in unit_ids {
            unique_add(&mut targets, unit_id);
        }
    }
    targets
}

fn time_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<u32> {
    let TriggerValue::Time(value) = value_at(effect, script, slot)? else {
        return None;
    };
    Some(*value)
}

fn color_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<TriggerColor> {
    let TriggerValue::Color(value) = value_at(effect, script, slot)? else {
        return None;
    };
    Some(*value)
}

fn float_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<f32> {
    let TriggerValue::Float(value) = value_at(effect, script, slot)? else {
        return None;
    };
    Some(*value)
}

fn bool_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<bool> {
    let TriggerValue::Bool(value) = value_at(effect, script, slot)? else {
        return None;
    };
    Some(*value)
}

#[cfg(test)]
#[path = "entity_visuals/tests.rs"]
mod tests;
