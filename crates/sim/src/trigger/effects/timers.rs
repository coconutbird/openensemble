//! Retail trigger game-timer creation and destruction.

use super::support::{bool_at, integer_at, player_at, unique_add, variable_is_used};
use super::{EffectOutcome, value_at, write_value};
use crate::player::PlayerId;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::{GameTimerAudience, World};

pub(super) fn create(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 4 | 5) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(count_up) = bool_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(start_time_ms) = time_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(stop_time_ms) = time_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = effect.variable_id(4) else {
        return EffectOutcome::Skipped;
    };
    let label_string_id = variable_is_used(effect, script, 6)
        .then(|| integer_at(effect, script, 6))
        .flatten();
    let audience = audience(effect, script);
    let timer_id = world.create_game_timer(
        count_up,
        start_time_ms,
        stop_time_ms,
        label_string_id,
        audience,
    );
    write_value(script, output_id, TriggerValue::Int(timer_id))
}

pub(super) fn destroy(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(timer_id) = integer_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let _destroyed = world.destroy_game_timer(timer_id);
    EffectOutcome::Applied
}

fn audience(effect: &Effect, script: &TriggerScript) -> GameTimerAudience {
    if effect.version == 4 {
        return GameTimerAudience::PrimaryUser;
    }
    let mut players = match value_at(effect, script, 8) {
        Some(TriggerValue::PlayerList(values)) => values
            .iter()
            .filter_map(|value| PlayerId::try_from(*value).ok())
            .collect(),
        _ => Vec::new(),
    };
    if let Some(player_id) = player_at(effect, script, 7) {
        unique_add(&mut players, player_id);
    }
    if players.is_empty() {
        GameTimerAudience::PrimaryUser
    } else {
        GameTimerAudience::Players(players)
    }
}

fn time_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Time(value) => Some(*value),
        TriggerValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "timers/tests.rs"]
mod tests;
