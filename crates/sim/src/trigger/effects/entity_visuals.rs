//! Retail entity highlighting and fog-memory policy effects.

use super::support::{EntityListKind, scalar_and_list, unique_add, variable_is_used};
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, EffectType, TriggerColor, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use num_traits::ToPrimitive;

/// The `FlashEntity` and `SetOverrideTint` target slots: unit, unit list, squad, squad list,
/// object, object list.
const FLASH_TARGET_SLOTS: TargetSlots = TargetSlots([1, 2, 3, 4, 5, 6]);
/// The `HintGlowToggle` v3 target slots.
const HINT_GLOW_V3_TARGET_SLOTS: TargetSlots = TargetSlots([7, 8, 9, 10, 6, 11]);

/// Target slots in the order unit, unit list, squad, squad list, object, object list.
#[derive(Debug, Clone, Copy)]
struct TargetSlots([u16; 6]);

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::FlashEntity => flash_entity(effect, script, world),
        EffectType::ResetDopple => reset_dopple(effect, script, world),
        EffectType::SetOverrideTint => set_override_tint(effect, script, world),
        EffectType::HintGlowToggle => hint_glow_toggle(effect, script, world),
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
    let targets = object_targets(effect, script, world, FLASH_TARGET_SLOTS);
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
    let targets = object_targets(effect, script, world, FLASH_TARGET_SLOTS);
    for entity_id in targets {
        let _changed = world.reset_entity_dopples(entity_id, gray_map_dopples, dopples);
    }
    EffectOutcome::Applied
}

/// `BTriggerEffect_teSetOverrideTint` (`0x140471E00`).
///
/// Slot 7 is the color and slot 8 the alpha (1 if not used). Retail multiplies each channel by
/// 255 and clamps it to a byte. Slot 9 enables the tint (false if not used).
fn set_override_tint(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(color) = color_at(effect, script, 7) else {
        return EffectOutcome::Skipped;
    };
    let alpha = if variable_is_used(effect, script, 8) {
        let Some(alpha) = float_at(effect, script, 8) else {
            return EffectOutcome::Skipped;
        };
        alpha
    } else {
        1.0
    };
    let enabled = if variable_is_used(effect, script, 9) {
        let Some(enabled) = bool_at(effect, script, 9) else {
            return EffectOutcome::Skipped;
        };
        enabled
    } else {
        false
    };
    let tint = [color.r, color.g, color.b, unit_float_to_byte(alpha)];
    for entity_id in object_targets(effect, script, world, FLASH_TARGET_SLOTS) {
        let _changed = world.set_entity_override_tint(entity_id, tint, enabled);
    }
    EffectOutcome::Presentation
}

/// `HintGlowToggle` (808): only v2 (`0x140473180`) and v3 (`0x1404732D0`) run.
///
/// Slot 2 turns the glow on (intensity 1) or off (intensity 0). v2 takes one entity in slot
/// 6: an object or unit, or the first unit of a squad. v3 takes the units of all target slots.
fn hint_glow_toggle(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(on) = bool_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let intensity = if on { 1.0 } else { 0.0 };
    let targets = match effect.version {
        2 => {
            let Some(entity_id) = value_at(effect, script, 6).and_then(TriggerValue::as_entity)
            else {
                return EffectOutcome::Skipped;
            };
            let target = world
                .get_squad(entity_id)
                .map_or(Some(entity_id), |squad| squad.unit_ids.first().copied());
            target.into_iter().collect()
        }
        3 => object_targets(effect, script, world, HINT_GLOW_V3_TARGET_SLOTS),
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    };
    for entity_id in targets {
        let _changed = world.set_entity_highlight_intensity(entity_id, intensity);
    }
    EffectOutcome::Presentation
}

/// Retail `(int)(value * 255)` clamped to a byte. NaN gives 0.
fn unit_float_to_byte(value: f32) -> u8 {
    (value * f32::from(u8::MAX))
        .trunc()
        .clamp(0.0, f32::from(u8::MAX))
        .to_u8()
        .unwrap_or(0)
}

fn object_targets(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
    TargetSlots(slots): TargetSlots,
) -> Vec<EntityId> {
    let mut references =
        scalar_and_list(effect, script, slots[4], slots[5], EntityListKind::Object);
    for entity_id in scalar_and_list(effect, script, slots[0], slots[1], EntityListKind::Unit) {
        references.push(entity_id);
    }
    let mut squad_references =
        scalar_and_list(effect, script, slots[2], slots[3], EntityListKind::Squad);
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
