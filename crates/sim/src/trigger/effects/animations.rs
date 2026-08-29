//! Trigger-authored animations retained by authoritative entity state.

use super::{
    Effect, EffectOutcome, EffectType, GameplayCatalog, TriggerScript, TriggerValue, World,
    value_at, write_value,
};

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
) -> Option<EffectOutcome> {
    (effect.effect_type == EffectType::PlayAnimationObject)
        .then(|| play_object_animation(effect, script, world, gameplay))
}

fn play_object_animation(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Skipped;
    }
    let entity_id = value_at(effect, script, 1).and_then(TriggerValue::as_entity);
    let animation_type = match value_at(effect, script, 2) {
        Some(TriggerValue::String(value)) if !value.trim().is_empty() => {
            Some(value.trim().to_owned())
        }
        _ => None,
    };
    let explicit_duration = (effect.version >= 3)
        .then(|| value_at(effect, script, 7))
        .flatten()
        .and_then(|value| match value {
            TriggerValue::Time(value) => Some(*value),
            _ => None,
        });

    let mut duration_ms = 0;
    let mut applied = false;
    if let (Some(entity_id), Some(animation_type)) = (entity_id, animation_type)
        && let Some(prototype) = world.entity_proto_object_name(entity_id)
    {
        let clip = gameplay.and_then(|catalog| {
            catalog.scripted_animation_clip(prototype, animation_type.as_str())
        });
        let clip_duration = match clip {
            Some(clip) => clip.duration_ms(),
            None => 0,
        };
        duration_ms = explicit_duration.unwrap_or(clip_duration);
        let asset_path = clip.map(|clip| clip.asset_path().to_owned());
        applied = world.play_entity_animation(entity_id, animation_type, asset_path, duration_ms);
    }

    let output = effect
        .variable_id(6)
        .map(|output_id| write_value(script, output_id, TriggerValue::Time(duration_ms)));
    if applied || output == Some(EffectOutcome::Applied) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

#[cfg(test)]
#[path = "animations/tests.rs"]
mod tests;
