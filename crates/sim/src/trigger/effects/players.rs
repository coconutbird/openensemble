//! Retail player-state and player-query effects of the skirmish scripts.

use super::support::{integer_at, variable_is_used};
use super::{EffectOutcome, write_value};
use crate::player::{PlayerState, PlayerType};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::world::World;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::SetPlayerState => set_player_state(effect, script, world),
        EffectType::GetPlayers => get_players(effect, script, world),
        EffectType::PlayerListGetSize => player_list_get_size(effect, script),
        EffectType::GetNPCPlayersByName => get_npc_players_by_name(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

/// `SetPlayerState` (130), retail effect case `0x82`.
///
/// Only version 2 does work. Signature 1 is the player and signature 3 is the state. Retail
/// reads both values without a null check and ignores a player ID outside the player list.
fn set_player_state(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let (Some(player_id), Some(state)) = (
        raw_integer(effect, script, 1),
        raw_integer(effect, script, 3),
    ) else {
        return EffectOutcome::Skipped;
    };
    let (Ok(player_id), Some(state)) = (u8::try_from(player_id), player_state(state)) else {
        return EffectOutcome::Skipped;
    };
    if world.get_player(player_id).is_none() {
        return EffectOutcome::Skipped;
    }
    let _changed = world.set_player_state(player_id, state);
    EffectOutcome::Applied
}

/// `GetPlayers` (173) version 2 (`sub_14045B6E0`).
///
/// Signature 2 includes Gaia. A non-null signature 4 keeps only the players in that state.
/// The list in signature 1 has the players in player-ID order.
fn get_players(effect: &Effect, script: &mut TriggerScript, world: &World) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(output_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let Some(include_gaia) = raw_value(effect, script, 2).and_then(TriggerValue::as_bool) else {
        return EffectOutcome::Skipped;
    };
    let state = if variable_is_used(effect, script, 4) {
        let Some(state) = integer_at(effect, script, 4) else {
            return EffectOutcome::Skipped;
        };
        Some(state)
    } else {
        None
    };
    let players = world
        .players()
        .filter(|player| include_gaia || player.id != crate::player::GAIA_PLAYER)
        .filter(|player| state.is_none_or(|state| i32::from(player.state as u8) == state))
        .map(|player| i32::from(player.id))
        .collect();
    write_value(script, output_id, TriggerValue::PlayerList(players))
}

/// `PlayerListGetSize` (970, `BTriggerEffect_tePlayerListGetSize`): signature 2 gets the size
/// of the list in signature 1.
fn player_list_get_size(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(TriggerValue::PlayerList(players)) = raw_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let size = i32::try_from(players.len()).unwrap_or(i32::MAX);
    let Some(output_id) = effect.variable_id(2) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Int(size))
}

/// `GetNPCPlayersByName` (943, `BTriggerEffect_teGetNPCPlayersByName`).
///
/// The list in signature 3 has each NPC player whose name is equal to signature 1, in
/// player-ID order. The name test ignores ASCII case. A non-null signature 2 keeps only the
/// players in that state.
fn get_npc_players_by_name(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(TriggerValue::String(name)) = raw_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let state = if variable_is_used(effect, script, 2) {
        let Some(state) = integer_at(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        Some(state)
    } else {
        None
    };
    let players = world
        .players()
        .filter(|player| player.player_type == PlayerType::Npc)
        .filter(|player| state.is_none_or(|state| i32::from(player.state as u8) == state))
        .filter(|player| player.name.eq_ignore_ascii_case(name))
        .map(|player| i32::from(player.id))
        .collect();
    let Some(output_id) = effect.variable_id(3) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::PlayerList(players))
}

/// Read a signature value without the null check, like the retail effects in this file.
fn raw_value<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a TriggerValue> {
    let variable_id = effect.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .map(|variable| &variable.value)
}

fn raw_integer(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<i32> {
    match raw_value(effect, script, signature_id)? {
        TriggerValue::Int(value) | TriggerValue::Player(value) => Some(*value),
        _ => None,
    }
}

fn player_state(value: i32) -> Option<PlayerState> {
    Some(match value {
        0 => PlayerState::Playing,
        1 => PlayerState::Resigned,
        2 => PlayerState::Defeated,
        3 => PlayerState::Disconnected,
        4 => PlayerState::Won,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
