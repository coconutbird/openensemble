//! Retail trigger effect 285 (`Revealer`) versions 2 and 3.

use super::support::{bool_at, float_at, player_at, used_variable_id, variable_is_used, vector_at};
use super::{EffectOutcome, value_at, write_value};
use crate::entity_id::EntityId;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::Database;

pub(super) fn create(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    match effect.version {
        2 => create_v2(effect, script, world, database),
        3 => create_v3(effect, script, world, database),
        _ => EffectOutcome::Unsupported(effect.raw_type),
    }
}

fn create_v2(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(location) = vector_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(line_of_sight) = float_at(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(lifespan) = optional_lifespan(effect, script) else {
        return EffectOutcome::Skipped;
    };
    if !valid_object_list_output(effect, script, 6) {
        return EffectOutcome::Skipped;
    }

    let created = player_team(world, player_id).and_then(|team_id| {
        world.create_revealer(database, team_id, location, line_of_sight, lifespan)
    });
    write_created_revealer(effect, script, created);
    if let Some(created) = created {
        update_v2_object_list(effect, script, created);
    }
    EffectOutcome::Applied
}

fn create_v3(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(line_of_sight) = float_at(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(mut locations) = optional_locations(effect, script) else {
        return EffectOutcome::Skipped;
    };
    if variable_is_used(effect, script, 2) {
        let Some(location) = vector_at(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        locations.push(location);
    }
    let Ok(lifespan) = optional_lifespan(effect, script) else {
        return EffectOutcome::Skipped;
    };
    if !valid_object_list_output(effect, script, 6) {
        return EffectOutcome::Skipped;
    }
    let lifespan = lifespan.filter(|&lifespan| lifespan != 0);
    let team_id = player_team(world, player_id);
    let created = locations
        .into_iter()
        .filter_map(|location| {
            team_id.and_then(|team_id| {
                world.create_revealer(database, team_id, location, line_of_sight, lifespan)
            })
        })
        .collect::<Vec<_>>();

    write_created_revealer(effect, script, created.first().copied());
    update_v3_object_list(effect, script, &created);
    EffectOutcome::Applied
}

fn optional_lifespan(effect: &Effect, script: &TriggerScript) -> Result<Option<u32>, ()> {
    if !variable_is_used(effect, script, 3) {
        return Ok(None);
    }
    match value_at(effect, script, 3) {
        Some(TriggerValue::Time(value)) => Ok(Some(*value)),
        _ => Err(()),
    }
}

fn optional_locations(effect: &Effect, script: &TriggerScript) -> Result<Vec<Vec3>, ()> {
    match value_at(effect, script, 8) {
        Some(TriggerValue::LocationList(values) | TriggerValue::VectorList(values)) => Ok(values
            .iter()
            .map(|value| Vec3::new(value.x, value.y, value.z))
            .collect()),
        Some(_) => Err(()),
        None => Ok(Vec::new()),
    }
}

fn player_team(world: &World, player_id: u8) -> Option<u8> {
    world.get_player(player_id).map(|player| player.team_id)
}

fn write_created_revealer(effect: &Effect, script: &mut TriggerScript, created: Option<EntityId>) {
    let Some(variable_id) = used_variable_id(effect, script, 5) else {
        return;
    };
    let _outcome = write_value(
        script,
        variable_id,
        TriggerValue::Object(created.unwrap_or(EntityId::INVALID)),
    );
}

fn valid_object_list_output(effect: &Effect, script: &TriggerScript, slot: u16) -> bool {
    let Some(variable_id) = used_variable_id(effect, script, slot) else {
        return true;
    };
    script
        .get_variable(variable_id)
        .is_some_and(|variable| matches!(variable.value, TriggerValue::ObjectList(_)))
}

fn update_v2_object_list(effect: &Effect, script: &mut TriggerScript, created: EntityId) {
    let Some(variable_id) = used_variable_id(effect, script, 6) else {
        return;
    };
    let clear = bool_at(effect, script, 7).unwrap_or(false);
    let Some(variable) = script.get_variable_mut(variable_id) else {
        return;
    };
    let TriggerValue::ObjectList(values) = &mut variable.value else {
        return;
    };
    if clear {
        values.clear();
    }
    if !values.contains(&created) {
        values.push(created);
    }
    variable.is_null = false;
}

fn update_v3_object_list(effect: &Effect, script: &mut TriggerScript, created: &[EntityId]) {
    let Some(variable_id) = used_variable_id(effect, script, 6) else {
        return;
    };
    let clear = bool_at(effect, script, 7).unwrap_or(false);
    let Some(variable) = script.get_variable_mut(variable_id) else {
        return;
    };
    let TriggerValue::ObjectList(values) = &mut variable.value else {
        return;
    };
    if clear {
        values.clear();
    }
    values.extend_from_slice(created);
    variable.is_null = false;
}

#[cfg(test)]
mod tests;
