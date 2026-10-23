//! Retail camera follow, lightset animation and positional sound effects.

use super::super::support::variable_is_used;
use super::super::{EffectOutcome, value_at};
use super::{int_value, optional_bool, optional_vector, time_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

/// `BTriggerEffect_teSetFollowCam` (`0x14046A060`).
///
/// The entity is the first used of slot 1 (unit), slot 2 (squad) and slot 3 (object). A
/// missing entity changes nothing. Slot 4 is an optional direction. Slot 5 true selects the
/// follow mode (user mode 12); otherwise the users go to mode 0.
pub(super) fn set_follow_cam(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(slot) = [1, 2, 3]
        .into_iter()
        .find(|slot| variable_is_used(effect, script, *slot))
    else {
        return EffectOutcome::Skipped;
    };
    let Some(entity) =
        entity_at(effect, script, slot).filter(|entity| entity_exists(world, *entity))
    else {
        return EffectOutcome::Skipped;
    };
    let (Ok(direction), Ok(follow)) = (
        optional_vector(effect, script, 4),
        optional_bool(effect, script, 5),
    ) else {
        return EffectOutcome::Skipped;
    };
    world.set_follow_camera(entity, direction, follow.unwrap_or(false));
    EffectOutcome::Presentation
}

/// `BTriggerEffect_teLightsetAnimate` (`0x14046A880`): slot 1 is the scenario lightset ID
/// and slot 2 the time. An unknown ID changes nothing.
pub(super) fn lightset_animate(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let (Some(lightset_id), Some(duration_ms)) =
        (int_value(effect, script, 1), time_value(effect, script, 2))
    else {
        return EffectOutcome::Skipped;
    };
    if world.animate_lightset(lightset_id, duration_ms) {
        EffectOutcome::Presentation
    } else {
        EffectOutcome::Skipped
    }
}

/// `PlayWorldSoundOnEntity` (515).
///
/// Slot 1 is the sound cue. v1 (`0x140469850`) takes the entity in slot 2 and does not accept
/// a squad. v2 (`0x140469900`) takes the first used of slots 2, 4 and 5.
pub(super) fn play_world_sound_on_entity(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let entity = match effect.version {
        1 => entity_at(effect, script, 2)
            .filter(|entity| world.entity_object_state(*entity).is_some()),
        2 => [2, 4, 5]
            .into_iter()
            .find(|slot| variable_is_used(effect, script, *slot))
            .and_then(|slot| entity_at(effect, script, slot))
            .filter(|entity| entity_exists(world, *entity)),
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    };
    let Some(entity) = entity else {
        return EffectOutcome::Skipped;
    };
    let sound_cue = match value_at(effect, script, 1) {
        Some(TriggerValue::String(sound_cue)) => sound_cue.clone(),
        _ => String::new(),
    };
    world.play_world_sound(sound_cue, entity);
    EffectOutcome::Presentation
}

fn entity_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    value_at(effect, script, slot).and_then(TriggerValue::as_entity)
}

fn entity_exists(world: &World, entity: EntityId) -> bool {
    world.entity_object_state(entity).is_some() || world.get_squad(entity).is_some()
}

#[cfg(test)]
#[path = "scene/tests.rs"]
mod tests;
