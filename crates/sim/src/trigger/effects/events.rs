//! Retail general-event subscriptions and presentation-producing effects.

use super::support::{
    bool_at, float_at, integer_at, used_variable_id, variable_is_used, vector_at,
};
use super::{EffectOutcome, value_at, write_value};
use crate::EntityId;
use crate::trigger::{Effect, EffectType, EntityFilterSet, TriggerScript, TriggerValue};
use crate::world::{ChatRequest, CinematicRequest, EventEntityParameter, GeneralEventType, World};

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::EventSubscribe => subscribe(effect, script, world, false),
        EffectType::EventSubscribeUseCount => subscribe(effect, script, world, true),
        EffectType::EventSetFilter => reset(effect, script, world, false),
        EffectType::EventReset => reset(effect, script, world, true),
        EffectType::EventFilterCamera => filter_camera(effect, script, world),
        EffectType::EventFilterEntity => filter_entity(effect, script, world),
        EffectType::EventFilterEntityList => filter_entity_list(effect, script, world),
        EffectType::EventDelete => EffectOutcome::Applied,
        EffectType::EventClearFilters => clear_filters(effect, script, world),
        EffectType::PlayChat => play_chat(effect, script, world),
        EffectType::LaunchCinematic => launch_cinematic(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

fn subscribe(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    use_count: bool,
) -> EffectOutcome {
    if !use_count && !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(event_type) = event_type_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let player_filter = if variable_is_used(effect, script, 3) {
        let Some(player_id) = integer_at(effect, script, 3) else {
            return EffectOutcome::Skipped;
        };
        Some(player_id)
    } else {
        None
    };
    let subscriber_id = world.subscribe_general_event(event_type, player_filter, use_count);
    if !use_count && effect.version == 1 {
        append_optional_entity_filter(
            effect,
            script,
            world,
            subscriber_id,
            4,
            EventEntityParameter::Source,
        );
        append_optional_entity_filter(
            effect,
            script,
            world,
            subscriber_id,
            5,
            EventEntityParameter::Target,
        );
    }
    write_value(
        script,
        output_id,
        TriggerValue::Int(i32::from_ne_bytes(subscriber_id.to_ne_bytes())),
    )
}

fn append_optional_entity_filter(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    subscriber_id: u32,
    signature_id: u16,
    parameter: EventEntityParameter,
) {
    let Some(filter_set) = filter_set_at(effect, script, signature_id).cloned() else {
        return;
    };
    let _added = world.add_general_event_entity_filter(subscriber_id, parameter, filter_set);
}

fn reset(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    consume_count: bool,
) -> EffectOutcome {
    let Some(subscriber_id) = subscriber_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.reset_general_event(subscriber_id, consume_count);
    EffectOutcome::Applied
}

fn clear_filters(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(subscriber_id) = subscriber_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.clear_general_event_filters(subscriber_id);
    EffectOutcome::Applied
}

fn filter_entity(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(subscriber_id) = subscriber_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    for (signature_id, parameter) in [
        (2, EventEntityParameter::Source),
        (3, EventEntityParameter::Target),
    ] {
        append_optional_entity_filter(
            effect,
            script,
            world,
            subscriber_id,
            signature_id,
            parameter,
        );
    }
    EffectOutcome::Applied
}

fn filter_entity_list(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(subscriber_id) = subscriber_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let filter_set = filter_set_at(effect, script, 2)
        .cloned()
        .unwrap_or_default();
    let minimum_matches = if variable_is_used(effect, script, 3) {
        integer_at(effect, script, 3)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(u32::MAX)
    } else {
        1
    };
    let all_must_match = bool_at(effect, script, 4).unwrap_or(false);
    if world.add_general_event_entity_list_filter(
        subscriber_id,
        filter_set,
        minimum_matches,
        all_must_match,
    ) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn filter_camera(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(subscriber_id) = subscriber_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let radius = if variable_is_used(effect, script, 2) {
        let Some(radius) = float_at(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        radius
    } else {
        100.0
    };
    let location = variable_is_used(effect, script, 5)
        .then(|| vector_at(effect, script, 5))
        .flatten();
    let unit = entity_at(effect, script, 3);
    let object = entity_at(effect, script, 4);
    let entity = object.or(unit);
    let invert = effect.version == 2 && bool_at(effect, script, 6).unwrap_or(false);
    if world.add_general_event_camera_filter(subscriber_id, radius, location, entity, invert) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn play_chat(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let sound_cue = string_at(effect, script, 1).unwrap_or_default().to_owned();
    let queue_sound = bool_at(effect, script, 2).unwrap_or(false);
    let string_id = scalar_i32_at(effect, script, 3).unwrap_or(-1);
    let duration_ms = time_at(effect, script, 6).unwrap_or(10_000);
    let talking_head_id = (effect.version == 4)
        .then(|| scalar_i32_at(effect, script, 7))
        .flatten();
    world.request_chat(ChatRequest {
        id: 0,
        sound_cue,
        queue_sound,
        string_id,
        duration_ms,
        talking_head_id,
    });
    EffectOutcome::Presentation
}

fn launch_cinematic(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(cinematic_id) = scalar_i32_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let mut possessed_squads = Vec::new();
    for signature_id in 2..=7 {
        let Some(squad_id) = entity_at(effect, script, signature_id) else {
            continue;
        };
        if !possessed_squads.contains(&squad_id) {
            possessed_squads.push(squad_id);
        }
    }
    let pre_rendered = false;
    if world
        .request_cinematic(CinematicRequest {
            id: 0,
            cinematic_id,
            possessed_squads,
            pre_rendered,
        })
        .is_none()
    {
        return EffectOutcome::Skipped;
    }
    if effect.version == 4
        && let Some(output_id) = used_variable_id(effect, script, 14)
    {
        let _outcome = write_value(script, output_id, TriggerValue::Bool(pre_rendered));
    }
    EffectOutcome::Presentation
}

fn event_type_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<GeneralEventType> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Int(value) => u16::try_from(*value)
            .ok()
            .and_then(GeneralEventType::from_u16),
        TriggerValue::String(value) => GeneralEventType::from_name(value),
        _ => None,
    }
}

fn subscriber_id_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u32> {
    integer_at(effect, script, signature_id).map(|value| u32::from_ne_bytes(value.to_ne_bytes()))
}

fn scalar_i32_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<i32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Int(value) => Some(*value),
        TriggerValue::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn string_at<'a>(effect: &Effect, script: &'a TriggerScript, signature_id: u16) -> Option<&'a str> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::String(value) => Some(value),
        _ => None,
    }
}

fn time_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Time(value) => Some(*value),
        _ => None,
    }
}

fn entity_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    value_at(effect, script, signature_id).and_then(TriggerValue::as_entity)
}

fn filter_set_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a EntityFilterSet> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::EntityFilterSet(filter_set) => Some(filter_set),
        _ => None,
    }
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
