//! Retail game-time, team, and player-list effects.

use super::support::{bool_at, float_at, integer_at, used_variable_id, variable_is_used};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, VarId};
use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdListKind {
    Player,
    Team,
}

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::GetTeams => get_teams(effect, script, world),
        EffectType::GetTeamPlayers => get_team_players(effect, script, world),
        EffectType::PlayerListAdd => list_add(effect, script, IdListKind::Player),
        EffectType::PlayerListRemove => list_remove(effect, script, IdListKind::Player),
        EffectType::TeamListAdd => list_add(effect, script, IdListKind::Team),
        EffectType::TeamListRemove => list_remove(effect, script, IdListKind::Team),
        EffectType::GetPlayers2 => get_players_2(effect, script, world),
        EffectType::GetGameTime => get_game_time(effect, script, world),
        EffectType::GetGameTimeRemaining => get_game_time_remaining(effect, script, world),
        EffectType::SetScenarioScoreInfo => set_scenario_score_info(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

fn set_scenario_score_info(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let (
        Some(combat_bonus_min),
        Some(combat_bonus_max),
        Some(mission_min_par_time),
        Some(mission_max_par_time),
        Some(grade_gold),
        Some(grade_silver),
        Some(grade_bronze),
        Some(_grade_tin),
    ) = (
        float_at(effect, script, 1),
        float_at(effect, script, 2),
        time_at(effect, script, 3),
        time_at(effect, script, 4),
        integer_at(effect, script, 5),
        integer_at(effect, script, 6),
        integer_at(effect, script, 7),
        integer_at(effect, script, 8),
    )
    else {
        return EffectOutcome::Skipped;
    };
    world.set_scenario_score_info(
        [combat_bonus_min, combat_bonus_max],
        [mission_min_par_time, mission_max_par_time],
        [grade_gold, grade_silver, grade_bronze],
    );
    EffectOutcome::Applied
}

pub(super) fn get_players_2(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(output_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(include_gaia) = bool_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(include_self) = bool_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(test_player) = integer_at(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let Some(relation_type) = integer_at(effect, script, 6) else {
        return EffectOutcome::Skipped;
    };
    let player_state = if variable_is_used(effect, script, 7) {
        let Some(player_state) = integer_at(effect, script, 7) else {
            return EffectOutcome::Skipped;
        };
        Some(player_state)
    } else {
        None
    };

    let found = world
        .players()
        .filter(|player| include_gaia || player.id != crate::player::GAIA_PLAYER)
        .filter(|player| include_self || i32::from(player.id) != test_player)
        .filter(|player| player_state.is_none_or(|state| i32::from(player.state as u8) == state))
        .filter(|player| relation_matches(world, player.id, test_player, relation_type))
        .map(|player| i32::from(player.id))
        .collect();
    write_value(script, output_id, TriggerValue::PlayerList(found))
}

fn relation_matches(world: &World, player: u8, test_player: i32, relation_type: i32) -> bool {
    if !matches!(relation_type, 2..=4) {
        return true;
    }
    let Ok(test_player) = u8::try_from(test_player) else {
        return false;
    };
    world
        .player_relation(player, test_player)
        .is_some_and(|relation| i32::from(relation as u8) == relation_type)
}

pub(super) fn get_teams(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let mut teams = Vec::new();
    for player in world.players() {
        unique_add(&mut teams, i32::from(player.team_id));
    }
    write_value(script, output_id, TriggerValue::TeamList(teams))
}

pub(super) fn get_team_players(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(team_id) = id_at(effect, script, 1, IdListKind::Team) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let players = world
        .players()
        .filter(|player| i32::from(player.team_id) == team_id)
        .map(|player| i32::from(player.id))
        .collect();
    write_value(script, output_id, TriggerValue::PlayerList(players))
}

pub(super) fn list_add(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: IdListKind,
) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(destination_id) = used_variable_id(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let clear_existing = bool_at(effect, script, 5).unwrap_or(false);
    let add_value = id_at(effect, script, 2, kind);
    let add_values = list_at(effect, script, 3, kind).cloned();
    let Some(destination) = list_mut(script, destination_id, kind) else {
        return EffectOutcome::Skipped;
    };

    if clear_existing {
        destination.clear();
    }
    if let Some(value) = add_value {
        unique_add(destination, value);
    }
    for value in add_values.unwrap_or_default() {
        unique_add(destination, value);
    }
    EffectOutcome::Applied
}

pub(super) fn list_remove(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: IdListKind,
) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(destination_id) = used_variable_id(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    if list_at_id(script, destination_id, kind).is_none() {
        return EffectOutcome::Skipped;
    }
    if bool_at(effect, script, 4).unwrap_or(false) {
        list_mut(script, destination_id, kind)
            .expect("destination type was checked")
            .clear();
        return EffectOutcome::Applied;
    }

    let remove_value = id_at(effect, script, 2, kind);
    let remove_list_id = used_variable_id(effect, script, 3);
    let remove_values = remove_list_id
        .filter(|variable_id| *variable_id != destination_id)
        .and_then(|variable_id| list_at_id(script, variable_id, kind))
        .cloned();
    let destination = list_mut(script, destination_id, kind).expect("destination type was checked");
    if let Some(value) = remove_value {
        remove_first(destination, value);
    }
    if remove_list_id == Some(destination_id) {
        remove_self_list(destination);
    } else {
        for value in remove_values.unwrap_or_default() {
            remove_first(destination, value);
        }
    }
    EffectOutcome::Applied
}

pub(super) fn get_game_time(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let delta = if variable_is_used(effect, script, 1) {
        let Some(delta) = time_at(effect, script, 1) else {
            return EffectOutcome::Skipped;
        };
        delta
    } else {
        0
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(
        script,
        output_id,
        TriggerValue::Time(world.game_time_ms.wrapping_add(delta)),
    )
}

pub(super) fn get_game_time_remaining(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(test_time) = time_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(
        script,
        output_id,
        TriggerValue::Time(test_time.saturating_sub(world.game_time_ms)),
    )
}

fn id_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: IdListKind,
) -> Option<i32> {
    match (kind, value_at(effect, script, signature_id)?) {
        (IdListKind::Player, TriggerValue::Player(value))
        | (IdListKind::Team, TriggerValue::Team(value)) => Some(*value),
        _ => None,
    }
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
    kind: IdListKind,
) -> Option<&'a Vec<i32>> {
    let variable_id = used_variable_id(effect, script, signature_id)?;
    list_at_id(script, variable_id, kind)
}

fn list_at_id(script: &TriggerScript, variable_id: VarId, kind: IdListKind) -> Option<&Vec<i32>> {
    match (kind, &script.get_variable(variable_id)?.value) {
        (IdListKind::Player, TriggerValue::PlayerList(values))
        | (IdListKind::Team, TriggerValue::TeamList(values)) => Some(values),
        _ => None,
    }
}

fn list_mut(
    script: &mut TriggerScript,
    variable_id: VarId,
    kind: IdListKind,
) -> Option<&mut Vec<i32>> {
    match (kind, &mut script.get_variable_mut(variable_id)?.value) {
        (IdListKind::Player, TriggerValue::PlayerList(values))
        | (IdListKind::Team, TriggerValue::TeamList(values)) => Some(values),
        _ => None,
    }
}

fn time_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Time(value) => Some(*value),
        _ => None,
    }
}

fn unique_add(values: &mut Vec<i32>, value: i32) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn remove_first(values: &mut Vec<i32>, value: i32) {
    if let Some(index) = values.iter().position(|candidate| *candidate == value) {
        values.remove(index);
    }
}

fn remove_self_list(values: &mut Vec<i32>) {
    let mut index = 0;
    while index < values.len() {
        let value = values[index];
        remove_first(values, value);
        index += 1;
    }
}

#[cfg(test)]
mod tests;
