//! Retail trigger grants and revocations for player-owned power entries.

use super::support::{bool_at, integer_at, player_at, unique_add, variable_is_used};
use super::{EffectOutcome, value_at};
use crate::EntityId;
use crate::player::{PlayerId, PowerGrant};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::{World, power_prototype_id};
use pipeline::database::hw1::Database;

pub(super) fn grant(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !has_player_input(effect, script) {
        return EffectOutcome::Skipped;
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(proto_power_id) = power_at(effect, script, database) else {
        return EffectOutcome::Skipped;
    };
    let players = selected_players(effect, script);
    if players.is_empty() {
        return EffectOutcome::Applied;
    }
    let power_squad = if effect.version == 3 {
        live_squad_at(effect, script, world, 9)
    } else {
        EntityId::INVALID
    };
    let grant = PowerGrant {
        proto_power_id,
        squad_id: power_squad,
        uses: optional_integer(effect, script, 5, 1),
        icon_location: optional_integer(effect, script, 3, -1),
        ignore_cost: optional_bool(effect, script, 6, false),
        ignore_tech_prerequisites: optional_bool(effect, script, 7, false),
        ignore_population: optional_bool(effect, script, 8, false),
    };
    for player_id in players {
        let _granted = world.grant_player_power(player_id, database, grant);
    }
    EffectOutcome::Applied
}

pub(super) fn revoke(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !has_player_input(effect, script) {
        return EffectOutcome::Skipped;
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(proto_power_id) = power_at(effect, script, database) else {
        return EffectOutcome::Skipped;
    };
    let players = selected_players(effect, script);
    if players.is_empty() {
        return EffectOutcome::Applied;
    }
    let power_squad = if effect.version == 3 {
        live_squad_at(effect, script, world, 5)
    } else {
        EntityId::INVALID
    };
    for player_id in players {
        let _revoked = world.revoke_player_power(player_id, database, proto_power_id, power_squad);
    }
    EffectOutcome::Applied
}

fn has_player_input(effect: &Effect, script: &TriggerScript) -> bool {
    variable_is_used(effect, script, 1) || variable_is_used(effect, script, 4)
}

fn selected_players(effect: &Effect, script: &TriggerScript) -> Vec<PlayerId> {
    let mut players = match value_at(effect, script, 4) {
        Some(TriggerValue::PlayerList(values)) => values
            .iter()
            .filter_map(|value| PlayerId::try_from(*value).ok())
            .collect(),
        _ => Vec::new(),
    };
    if let Some(player_id) = player_at(effect, script, 1) {
        unique_add(&mut players, player_id);
    }
    players
}

fn power_at(effect: &Effect, script: &TriggerScript, database: &Database) -> Option<i32> {
    match value_at(effect, script, 2)? {
        TriggerValue::String(name) => power_prototype_id(database, name),
        TriggerValue::Int(id) => power_prototype_id(database, &id.to_string()),
        _ => None,
    }
}

fn live_squad_at(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
    signature_id: u16,
) -> EntityId {
    value_at(effect, script, signature_id)
        .and_then(TriggerValue::as_entity)
        .filter(|squad_id| world.get_squad(*squad_id).is_some())
        .unwrap_or(EntityId::INVALID)
}

fn optional_integer(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    default: i32,
) -> i32 {
    if variable_is_used(effect, script, signature_id) {
        integer_at(effect, script, signature_id).unwrap_or(default)
    } else {
        default
    }
}

fn optional_bool(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    default: bool,
) -> bool {
    if variable_is_used(effect, script, signature_id) {
        bool_at(effect, script, signature_id).unwrap_or(default)
    } else {
        default
    }
}

#[cfg(test)]
mod tests;
