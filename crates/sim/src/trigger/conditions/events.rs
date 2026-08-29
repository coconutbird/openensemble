//! Retail general-event and presentation-completion conditions.

use super::value_at;
use crate::trigger::{Condition, TriggerScript, TriggerValue};
use crate::world::World;

pub(super) fn event_triggered(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(subscriber_id) = subscriber_id_at(condition, script, 1) else {
        return false;
    };
    world.general_event_fired(subscriber_id) || world.general_event_fire_count(subscriber_id) > 0
}

pub(super) fn chat_completed(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some((fired, fire_time)) = world.chat_completed() else {
        return false;
    };
    if !fired {
        return false;
    }
    match condition.version {
        1 => true,
        2 => time_at(condition, script, 2)
            .is_none_or(|delay| world.game_time_ms.wrapping_sub(fire_time) >= delay),
        _ => false,
    }
}

pub(super) fn cinematic_completed(world: &World) -> bool {
    world.cinematic_completed()
}

pub(super) fn fade_completed(world: &World) -> bool {
    world.screen_fade_completed()
}

fn subscriber_id_at(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<u32> {
    match value_at(condition, script, signature_id)? {
        TriggerValue::Int(value) => Some(u32::from_ne_bytes(value.to_ne_bytes())),
        _ => None,
    }
}

fn time_at(condition: &Condition, script: &TriggerScript, signature_id: u16) -> Option<u32> {
    match value_at(condition, script, signature_id)? {
        TriggerValue::Time(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
