//! Retail resource and player-technology trigger effects.

use super::{EffectOutcome, value_at, write_value};
use crate::player::Resources;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use pipeline::database::hw1::Database;

pub(super) fn pay_cost(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some((player_id, cost)) = player_and_cost(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player_mut(player_id) else {
        return EffectOutcome::Skipped;
    };
    let cost = resources_from_cost(cost);
    if player.resources.can_afford(&cost) {
        player.resources.pay(&cost);
    }
    EffectOutcome::Applied
}

pub(super) fn refund_cost(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some((player_id, cost)) = player_and_cost(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player_mut(player_id) else {
        return EffectOutcome::Skipped;
    };
    player.resources.refund(&resources_from_cost(cost));
    EffectOutcome::Applied
}

pub(super) fn set_resources(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    totals: bool,
) -> EffectOutcome {
    let Some((player_id, cost)) = player_and_cost(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player_mut(player_id) else {
        return EffectOutcome::Skipped;
    };
    if totals {
        player.total_resources = resources_from_cost(cost);
    } else {
        player.resources = resources_from_cost(cost);
    }
    EffectOutcome::Applied
}

pub(super) fn get_resources(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    totals: bool,
) -> EffectOutcome {
    let Some(player_id) = player_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player(player_id) else {
        return EffectOutcome::Skipped;
    };
    let resources = if totals {
        player.total_resources
    } else {
        player.resources
    };
    write_value(
        script,
        output_id,
        TriggerValue::Cost(cost_from_resources(&resources)),
    )
}

pub(super) fn set_trickle_rate(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(TriggerValue::Cost(cost)) = value_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let rate = resources_from_cost(cost);
    let mut player_ids = match value_at(effect, script, 2) {
        Some(TriggerValue::PlayerList(player_ids)) => player_ids
            .iter()
            .filter_map(|player_id| u8::try_from(*player_id).ok())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if let Some(player_id) = player_id_at(effect, script, 1)
        && !player_ids.contains(&player_id)
    {
        player_ids.push(player_id);
    }
    for player_id in player_ids {
        if let Some(player) = world.get_player_mut(player_id) {
            player.set_resource_trickle_rate(rate);
        }
    }
    EffectOutcome::Applied
}

pub(super) fn get_trickle_rate(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let rate = player_id_at(effect, script, 1)
        .and_then(|player_id| world.get_player(player_id))
        .map_or_else(Resources::new, crate::player::Player::resource_trickle_rate);
    write_value(
        script,
        output_id,
        TriggerValue::Cost(cost_from_resources(&rate)),
    )
}

pub(super) fn get_player_pop(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player(player_id) else {
        return EffectOutcome::Skipped;
    };
    let Some(population_type) = crate::scenario::population::population_type_id(database, "Unit")
    else {
        return EffectOutcome::Skipped;
    };
    let Some(population) = player.get_population(population_type) else {
        return EffectOutcome::Skipped;
    };

    write_optional_float(effect, script, 2, population.future + population.count);
    write_optional_float(effect, script, 3, population.cap.min(population.max));
    write_optional_float(effect, script, 4, population.future);
    write_optional_float(effect, script, 5, population.count);
    EffectOutcome::Applied
}

pub(super) fn set_player_pop(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(population_type) = crate::scenario::population::population_type_id(database, "Unit")
    else {
        return EffectOutcome::Skipped;
    };
    let Some(population) = world
        .get_player_mut(player_id)
        .and_then(|player| player.get_population_mut(population_type))
    else {
        return EffectOutcome::Skipped;
    };

    if let Some(value) = optional_float_at(effect, script, 2) {
        population.cap = value;
    }
    if let Some(value) = optional_float_at(effect, script, 3) {
        population.max = value;
    }
    if let Some(value) = optional_float_at(effect, script, 4) {
        population.future = value;
    }
    if let Some(value) = optional_float_at(effect, script, 5) {
        population.count = value;
    }
    EffectOutcome::Applied
}

pub(super) fn change_technology(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
    activate: bool,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_id_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(technology_id) = value_at(effect, script, 2).and_then(as_i32) else {
        return EffectOutcome::Skipped;
    };
    let Some(technology) = usize::try_from(technology_id)
        .ok()
        .and_then(|index| database.techs.get(index))
    else {
        return EffectOutcome::Skipped;
    };
    let result = if activate {
        world.activate_technology(player_id, database, &technology.name)
    } else {
        world.deactivate_technology(player_id, database, &technology.name)
    };
    if result.is_ok() {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn player_and_cost<'script>(
    effect: &Effect,
    script: &'script TriggerScript,
) -> Option<(u8, &'script crate::trigger::value::Cost)> {
    let player_id = player_id_at(effect, script, 1)?;
    let TriggerValue::Cost(cost) = value_at(effect, script, 2)? else {
        return None;
    };
    Some((player_id, cost))
}

fn player_id_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<u8> {
    value_at(effect, script, signature_id)
        .and_then(as_i32)
        .and_then(|value| u8::try_from(value).ok())
}

fn used_variable_id(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<crate::trigger::VarId> {
    let variable_id = effect.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
        .then_some(variable_id)
}

fn as_i32(value: &TriggerValue) -> Option<i32> {
    match value {
        TriggerValue::Int(value) | TriggerValue::Player(value) | TriggerValue::Tech(value) => {
            Some(*value)
        }
        _ => None,
    }
}

fn optional_float_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<f32> {
    used_variable_id(effect, script, signature_id).and_then(|_| {
        let TriggerValue::Float(value) = value_at(effect, script, signature_id)? else {
            return None;
        };
        Some(*value)
    })
}

fn resources_from_cost(cost: &crate::trigger::value::Cost) -> Resources {
    Resources {
        amounts: cost.amounts(),
    }
}

fn cost_from_resources(resources: &Resources) -> crate::trigger::value::Cost {
    crate::trigger::value::Cost::from_amounts(resources.amounts)
}

fn write_optional_float(
    effect: &Effect,
    script: &mut TriggerScript,
    signature_id: u16,
    value: f32,
) {
    if let Some(output_id) = used_variable_id(effect, script, signature_id) {
        let _outcome = write_value(script, output_id, TriggerValue::Float(value));
    }
}

#[cfg(test)]
mod tests;
