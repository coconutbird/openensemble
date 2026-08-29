//! Player-specific prototype mutations from retail trigger effect 237.

use super::support::{
    bool_at, float_at, integer_at, object_type_at, player_at, unique_add, variable_is_used,
};
use super::{EffectOutcome, value_at};
use crate::player::{PlayerId, ProtoDataModification, ProtoDataRelativity, ProtoDataType};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use pipeline::database::hw1::{Database, ProtoObject};

const COMMAND_TYPES: [&str; 21] = [
    "Research",
    "TrainUnit",
    "Build",
    "TrainSquad",
    "Unload",
    "Reinforce",
    "ChangeMode",
    "Ability",
    "Kill",
    "CancelKill",
    "Tribute",
    "CustomCommand",
    "Power",
    "BuildOther",
    "TrainLock",
    "TrainUnlock",
    "RallyPoint",
    "ClearRallyPoint",
    "DestroyBase",
    "CancelDestroyBase",
    "ReverseHotDrop",
];

pub(super) fn modify_proto_data(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 4 | 5) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    if !(variable_is_used(effect, script, 1) || variable_is_used(effect, script, 2)) {
        return EffectOutcome::Skipped;
    }
    let Some(amount) = amount_at(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let (Some(object_type), Some(data_type), Some(relativity)) = (
        object_type_at(effect, script, 3),
        data_type_at(effect, script, 5),
        relativity_at(effect, script, 6),
    ) else {
        return EffectOutcome::Unsupported(effect.raw_type);
    };
    let players = selected_players(effect, script);
    if players.is_empty() {
        // Retail validates that a player variable is present, then treats an
        // empty resolved list as a successfully executed no-op.
        return EffectOutcome::Applied;
    }
    if data_type == ProtoDataType::Level {
        // Retail's proto-object switch has no Level arm.
        return EffectOutcome::Applied;
    }

    let command_type = command_type_at(effect, script, 11);
    let command_data = command_data_at(effect, script, database, command_type.as_deref());
    let modification = ProtoDataModification {
        data_type,
        amount,
        relativity,
        all_actions: bool_at(effect, script, 7).unwrap_or(false),
        name: string_at(effect, script, 9),
        invert: bool_at(effect, script, 10).unwrap_or(false),
        command_type,
        command_data,
    };
    let prototypes = database
        .objects
        .iter()
        .filter(|prototype| prototype_matches_type(prototype, &object_type))
        .collect::<Vec<_>>();
    for prototype in prototypes {
        for &player_id in &players {
            let _changed =
                world.modify_player_proto_data(player_id, prototype, &modification, database);
        }
    }
    EffectOutcome::Applied
}

fn amount_at(effect: &Effect, script: &TriggerScript) -> Option<f32> {
    let signature_id = if variable_is_used(effect, script, 4) {
        4
    } else if variable_is_used(effect, script, 8) {
        8
    } else {
        return None;
    };
    float_at(effect, script, signature_id).filter(|value| value.is_finite())
}

fn selected_players(effect: &Effect, script: &TriggerScript) -> Vec<PlayerId> {
    let mut players = match value_at(effect, script, 2) {
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

fn data_type_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<ProtoDataType> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::String(value) => ProtoDataType::from_trigger_value(value),
        TriggerValue::Int(value) => ProtoDataType::from_trigger_value(&value.to_string()),
        _ => None,
    }
}

fn relativity_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<ProtoDataRelativity> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::String(value) => ProtoDataRelativity::from_trigger_value(value),
        TriggerValue::Int(value) => ProtoDataRelativity::from_trigger_value(&value.to_string()),
        _ => None,
    }
}

fn string_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<String> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::String(value) => (!value.trim().is_empty()).then(|| value.trim().to_owned()),
        _ => None,
    }
}

fn command_type_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<String> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Int(value) => usize::try_from(*value)
            .ok()
            .and_then(|index| COMMAND_TYPES.get(index))
            .map(|name| (*name).to_owned()),
        TriggerValue::String(value) => COMMAND_TYPES
            .iter()
            .find(|name| name.eq_ignore_ascii_case(value.trim()))
            .map(|name| (*name).to_owned()),
        _ => None,
    }
}

fn command_data_at(
    effect: &Effect,
    script: &TriggerScript,
    database: &Database,
    command_type: Option<&str>,
) -> Option<String> {
    if effect.version == 5
        && variable_is_used(effect, script, 13)
        && let Some(id) = integer_at(effect, script, 13)
    {
        return runtime_name(&database.techs, id, |technology| &technology.name);
    }
    let id = integer_at(effect, script, 12)?;
    match command_type {
        Some(kind) if kind.eq_ignore_ascii_case("Research") => {
            runtime_name(&database.techs, id, |technology| &technology.name)
        }
        Some(kind)
            if kind.eq_ignore_ascii_case("TrainSquad")
                || kind.eq_ignore_ascii_case("Reinforce") =>
        {
            runtime_name(&database.squads, id, |squad| &squad.name)
        }
        Some(kind)
            if kind.eq_ignore_ascii_case("TrainUnit")
                || kind.eq_ignore_ascii_case("Build")
                || kind.eq_ignore_ascii_case("BuildOther") =>
        {
            runtime_proto_object_name(database, id)
        }
        _ => Some(id.to_string()),
    }
}

fn runtime_name<T>(entries: &[T], id: i32, name: impl Fn(&T) -> &str) -> Option<String> {
    usize::try_from(id)
        .ok()
        .and_then(|index| entries.get(index))
        .map(|entry| name(entry).to_owned())
}

fn runtime_proto_object_name(database: &Database, id: i32) -> Option<String> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == id)
        .map(|(_, prototype)| prototype.name.clone())
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn prototype_matches_type(prototype: &ProtoObject, object_type: &str) -> bool {
    prototype.name.eq_ignore_ascii_case(object_type)
        || prototype
            .object_class
            .as_deref()
            .is_some_and(|class| class.eq_ignore_ascii_case(object_type))
        || prototype
            .object_types
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case(object_type))
}

#[cfg(test)]
mod tests;
