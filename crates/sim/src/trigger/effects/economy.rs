//! Retail economy, prototype-cost, and population query effects.

use super::support::{float_at, integer_at, player_at, used_variable_id, variable_is_used};
use super::{EffectOutcome, value_at, write_value};
use crate::player::{MAX_RESOURCES, Player};
use crate::trigger::value::Cost;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::World;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad, Tech};

pub(super) fn get_player_economy(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(player) = world.get_player(player_id) else {
        return EffectOutcome::Skipped;
    };

    let supplies = named_resource(database, player, "Supplies");
    let supply_rate = named_rate(database, player, "Supplies");
    let power = named_resource(database, player, "Power");
    let power_rate = named_rate(database, player, "Power");
    let leader_power = named_resource(database, player, "LeaderPowerCharge");
    let leader_power_rate = named_rate(database, player, "LeaderPowerCharge");

    write_optional_float(effect, script, 2, supplies);
    write_optional_float(effect, script, 3, supply_rate);
    write_optional_float(effect, script, 4, power);
    write_optional_float(effect, script, 5, power_rate);
    write_optional_float(effect, script, 6, leader_power);
    write_optional_float(effect, script, 7, leader_power_rate);
    // No leader power can be granted until the authoritative PowerGrant effect
    // creates a player power entry, so retail's derived charge count is zero.
    write_optional_float(effect, script, 8, 0.0);
    EffectOutcome::Applied
}

pub(super) fn cost_to_float(
    effect: &Effect,
    script: &mut TriggerScript,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(TriggerValue::Cost(cost)) = value_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(supply_coefficient) = optional_coefficient(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(power_coefficient) = optional_coefficient(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(charges_coefficient) = optional_coefficient(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };

    let mut result = 0.0;
    for (name, coefficient) in [
        ("Supplies", supply_coefficient),
        ("Power", power_coefficient),
        ("LeaderPowerCharge", charges_coefficient),
    ] {
        if let Some(resource_id) = named_resource_id(database, name) {
            result += cost.get(resource_id) * coefficient;
        }
    }
    write_value(script, output_id, TriggerValue::Float(result))
}

pub(super) fn get_cost(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let player = optional_player(effect, script, world);

    let cost = if variable_is_used(effect, script, 2) {
        integer_at(effect, script, 2)
            .and_then(|id| squad_by_id(database, id))
            .map(|prototype| effective_squad(database, player, prototype))
            .map(|prototype| squad_cost(database, prototype))
    } else if variable_is_used(effect, script, 3) {
        integer_at(effect, script, 3)
            .and_then(|id| tech_by_id(database, id))
            .map(|prototype| tech_cost(database, prototype))
    } else if variable_is_used(effect, script, 4) {
        integer_at(effect, script, 4)
            .and_then(|id| object_by_id(database, id))
            .map(|prototype| object_cost(database, prototype))
    } else {
        return EffectOutcome::Skipped;
    };
    let Some(cost) = cost else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Cost(cost))
}

pub(super) fn get_pop(
    effect: &Effect,
    script: &mut TriggerScript,
    database: Option<&Database>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    if effect.version == 2 {
        get_pop_v2(effect, script, database)
    } else {
        get_pop_v1(effect, script, database)
    }
}

fn get_pop_v1(effect: &Effect, script: &mut TriggerScript, database: &Database) -> EffectOutcome {
    let unit_population = crate::scenario::population::population_type_id(database, "Unit");
    let mut population = 0.0;
    if variable_is_used(effect, script, 1)
        && let Some(prototype) =
            integer_at(effect, script, 1).and_then(|id| object_by_id(database, id))
    {
        population = unit_population_amount(
            &crate::scenario::population::object_population_costs(database, prototype),
            unit_population,
        );
    }
    if variable_is_used(effect, script, 2)
        && let Some(prototype) =
            integer_at(effect, script, 2).and_then(|id| squad_by_id(database, id))
    {
        population = unit_population_amount(
            &crate::scenario::population::squad_population_costs(database, prototype),
            unit_population,
        );
    }
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Float(population))
}

fn get_pop_v2(effect: &Effect, script: &mut TriggerScript, database: &Database) -> EffectOutcome {
    let unit_population = crate::scenario::population::population_type_id(database, "Unit");
    let mut result = PopulationQuery::default();
    if variable_is_used(effect, script, 1)
        && let Some(prototype) =
            integer_at(effect, script, 1).and_then(|id| object_by_id(database, id))
    {
        result.include(
            &crate::scenario::population::object_population_costs(database, prototype),
            unit_population,
        );
    }
    if variable_is_used(effect, script, 2)
        && let Some(prototype) =
            integer_at(effect, script, 2).and_then(|id| squad_by_id(database, id))
    {
        result.include(
            &crate::scenario::population::squad_population_costs(database, prototype),
            unit_population,
        );
    }

    write_optional_float(effect, script, 3, result.value);
    write_optional_bool(effect, script, 4, result.is_unit_population);
    write_optional_int(effect, script, 5, result.population_id);
    EffectOutcome::Applied
}

#[derive(Debug)]
struct PopulationQuery {
    value: f32,
    is_unit_population: bool,
    population_id: i32,
}

impl Default for PopulationQuery {
    fn default() -> Self {
        Self {
            value: 0.0,
            is_unit_population: false,
            population_id: -1,
        }
    }
}

impl PopulationQuery {
    fn include(
        &mut self,
        populations: &[crate::player::PopulationCost],
        unit_population: Option<usize>,
    ) {
        for population in populations {
            if population.amount == 0.0 {
                continue;
            }
            if Some(population.population_type) == unit_population {
                self.is_unit_population = true;
            }
            self.population_id = i32::try_from(population.population_type).unwrap_or(-1);
            self.value = population.amount;
        }
    }
}

fn optional_coefficient(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<f32> {
    if variable_is_used(effect, script, slot) {
        float_at(effect, script, slot)
    } else {
        Some(1.0)
    }
}

fn optional_player<'world>(
    effect: &Effect,
    script: &TriggerScript,
    world: &'world World,
) -> Option<&'world Player> {
    variable_is_used(effect, script, 1)
        .then(|| integer_at(effect, script, 1))
        .flatten()
        .and_then(|id| u8::try_from(id).ok())
        .and_then(|id| world.get_player(id))
}

fn named_resource(database: &Database, player: &Player, name: &str) -> f32 {
    named_resource_id(database, name).map_or(0.0, |id| player.get_resource(id))
}

fn named_rate(database: &Database, player: &Player, name: &str) -> f32 {
    let resource_count = resource_count(database);
    database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.rates.as_ref())
        .and_then(|rates| {
            rates
                .entries
                .iter()
                .position(|candidate| candidate.trim().eq_ignore_ascii_case(name.trim()))
        })
        .filter(|rate_id| *rate_id < resource_count)
        .map_or(0.0, |rate_id| player.get_rate(rate_id))
}

fn named_resource_id(database: &Database, name: &str) -> Option<usize> {
    database
        .game_data
        .as_ref()?
        .resources
        .as_ref()?
        .entries
        .iter()
        .position(|resource| resource.name.trim().eq_ignore_ascii_case(name.trim()))
        .filter(|resource_id| *resource_id < MAX_RESOURCES)
}

fn resource_count(database: &Database) -> usize {
    database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .map_or(0, |resources| resources.entries.len().min(MAX_RESOURCES))
}

fn squad_by_id(database: &Database, id: i32) -> Option<&ProtoSquad> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == id)
        .map(|(_, prototype)| prototype)
}

fn object_by_id(database: &Database, id: i32) -> Option<&ProtoObject> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == id)
        .map(|(_, prototype)| prototype)
}

fn tech_by_id(database: &Database, id: i32) -> Option<&Tech> {
    usize::try_from(id)
        .ok()
        .and_then(|index| database.techs.get(index))
}

fn effective_squad<'database>(
    database: &'database Database,
    player: Option<&Player>,
    logical: &'database ProtoSquad,
) -> &'database ProtoSquad {
    let Some(player) = player else {
        return logical;
    };
    let effective_name = player.technologies.resolved_squad_prototype(&logical.name);
    database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(effective_name))
        .unwrap_or(logical)
}

fn squad_cost(database: &Database, prototype: &ProtoSquad) -> Cost {
    let mut cost = Cost::default();
    for entry in &prototype.costs {
        add_named_cost(database, &mut cost, &entry.resource_type, entry.amount);
    }
    cost
}

fn tech_cost(database: &Database, prototype: &Tech) -> Cost {
    let mut cost = Cost::default();
    for entry in &prototype.costs {
        add_named_cost(database, &mut cost, &entry.resource_type, entry.amount);
    }
    cost
}

fn object_cost(database: &Database, prototype: &ProtoObject) -> Cost {
    let mut cost = Cost::default();
    for entry in &prototype.costs {
        add_named_cost(database, &mut cost, &entry.resource_type, entry.amount);
    }
    cost
}

fn add_named_cost(database: &Database, cost: &mut Cost, resource: &str, amount: f32) {
    if let Some(resource_id) = named_resource_id(database, resource) {
        cost.add(resource_id, amount);
    }
}

fn unit_population_amount(
    populations: &[crate::player::PopulationCost],
    unit_population: Option<usize>,
) -> f32 {
    unit_population
        .and_then(|unit_population| {
            populations
                .iter()
                .find(|population| population.population_type == unit_population)
        })
        .map_or(0.0, |population| population.amount)
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn write_optional_float(effect: &Effect, script: &mut TriggerScript, slot: u16, value: f32) {
    if let Some(output_id) = used_variable_id(effect, script, slot) {
        let _outcome = write_value(script, output_id, TriggerValue::Float(value));
    }
}

fn write_optional_bool(effect: &Effect, script: &mut TriggerScript, slot: u16, value: bool) {
    if let Some(output_id) = used_variable_id(effect, script, slot) {
        let _outcome = write_value(script, output_id, TriggerValue::Bool(value));
    }
}

fn write_optional_int(effect: &Effect, script: &mut TriggerScript, slot: u16, value: i32) {
    if let Some(output_id) = used_variable_id(effect, script, slot) {
        let _outcome = write_value(script, output_id, TriggerValue::Int(value));
    }
}

#[cfg(test)]
mod tests;
