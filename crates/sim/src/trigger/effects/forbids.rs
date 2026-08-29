//! Per-player prototype `Forbid` flag mutations.

use super::{EffectOutcome, value_at};
use crate::player::PlayerId;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use pipeline::database::hw1::Database;

pub(super) fn set_forbidden(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(forbidden) = value_at(effect, script, 1).and_then(TriggerValue::as_bool) else {
        return EffectOutcome::Skipped;
    };
    let Some(TriggerValue::PlayerList(player_values)) = value_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(technologies) = optional_technology_list(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(objects) = optional_object_list(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(squads) = optional_squad_list(effect, script) else {
        return EffectOutcome::Skipped;
    };

    for player_id in player_values
        .iter()
        .filter_map(|value| PlayerId::try_from(*value).ok())
    {
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        for &technology_id in technologies {
            let _changed = player.set_technology_forbidden(database, technology_id, forbidden);
        }
        for &prototype_id in objects {
            let _changed = player.set_object_forbidden(database, prototype_id, forbidden);
        }
        for &prototype_id in squads {
            let _changed = player.set_squad_forbidden(database, prototype_id, forbidden);
        }
    }
    EffectOutcome::Applied
}

fn optional_technology_list<'a>(effect: &Effect, script: &'a TriggerScript) -> Option<&'a [i32]> {
    match value_at(effect, script, 3) {
        Some(TriggerValue::TechList(values)) => Some(values),
        Some(_) => None,
        None => Some(&[]),
    }
}

fn optional_object_list<'a>(effect: &Effect, script: &'a TriggerScript) -> Option<&'a [i32]> {
    match value_at(effect, script, 4) {
        Some(TriggerValue::ProtoObjectList(values)) => Some(values),
        Some(_) => None,
        None => Some(&[]),
    }
}

fn optional_squad_list<'a>(effect: &Effect, script: &'a TriggerScript) -> Option<&'a [i32]> {
    match value_at(effect, script, 5) {
        Some(TriggerValue::ProtoSquadList(values)) => Some(values),
        Some(_) => None,
        None => Some(&[]),
    }
}

#[cfg(test)]
mod tests;
