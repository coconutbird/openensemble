//! Database population resolution shared by scenario and production spawns.

use crate::player::PopulationCost;
use crate::{EntityId, World};
use pipeline::database::hw1::objects::PopulationAmount;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

pub(crate) fn object_population_costs(
    database: &Database,
    prototype: &ProtoObject,
) -> Vec<PopulationCost> {
    resolve_object_amounts(database, prototype, &prototype.population, false)
}

pub(crate) fn object_population_cap_additions(
    database: &Database,
    prototype: &ProtoObject,
) -> Vec<PopulationCost> {
    resolve_object_amounts(
        database,
        prototype,
        &prototype.population_cap_additions,
        false,
    )
}

pub(crate) fn squad_population_costs(
    database: &Database,
    prototype: &ProtoSquad,
) -> Vec<PopulationCost> {
    let slot_count = population_names(database).len();
    let mut totals = vec![0.0; slot_count];
    for member in prototype
        .units
        .as_ref()
        .map_or(&[][..], |units| units.entries.as_slice())
    {
        let Some(member_prototype) = database
            .objects
            .iter()
            .find(|object| object.name.eq_ignore_ascii_case(member.proto_object.trim()))
        else {
            continue;
        };
        let Ok(count) = u16::try_from(member.count.max(0)) else {
            continue;
        };
        let count = f32::from(count);
        for cost in object_population_costs(database, member_prototype) {
            totals[cost.population_type] += cost.amount * count;
        }
    }
    totals
        .into_iter()
        .enumerate()
        .filter_map(|(population_type, amount)| {
            let rounded = (amount + 0.5).floor();
            (rounded > 0.0).then_some(PopulationCost::new(population_type, rounded))
        })
        .collect()
}

pub(crate) fn apply_object_population(
    world: &mut World,
    unit_id: EntityId,
    database: &Database,
    prototype: &ProtoObject,
) {
    initialize_object_population(world, unit_id, database, prototype, true);
}

pub(crate) fn initialize_object_population(
    world: &mut World,
    unit_id: EntityId,
    database: &Database,
    prototype: &ProtoObject,
    apply_cap_additions: bool,
) {
    let costs = object_population_costs(database, prototype);
    let cap_additions = object_population_cap_additions(database, prototype);
    let Some(player_id) = world.get_unit(unit_id).map(|unit| unit.base.player_id) else {
        return;
    };
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.population_costs.clone_from(&costs);
        unit.population_cap_additions.clone_from(&cap_additions);
    }
    if let Some(player) = world.get_player_mut(player_id) {
        if apply_cap_additions {
            player.adjust_population_cap(&cap_additions, true);
        }
        player.add_population(&costs);
    }
}

pub(crate) fn complete_object_population(world: &mut World, unit_id: EntityId) -> bool {
    let Some((player_id, additions, built)) = world.get_unit(unit_id).map(|unit| {
        (
            unit.base.player_id,
            unit.population_cap_additions.clone(),
            unit.built,
        )
    }) else {
        return false;
    };
    if built {
        return false;
    }
    if let Some(player) = world.get_player_mut(player_id) {
        player.adjust_population_cap(&additions, true);
    }
    if let Some(unit) = world.get_unit_mut(unit_id) {
        unit.built = true;
    }
    true
}

pub(crate) fn apply_squad_population(
    world: &mut World,
    squad_id: EntityId,
    database: &Database,
    prototype: &ProtoSquad,
) {
    let costs = squad_population_costs(database, prototype);
    let Some(player_id) = world.get_squad(squad_id).map(|squad| squad.base.player_id) else {
        return;
    };
    if let Some(squad) = world.get_squad_mut(squad_id) {
        squad.population_costs.clone_from(&costs);
    }
    if let Some(player) = world.get_player_mut(player_id) {
        player.add_population(&costs);
    }
}

pub(crate) fn population_type_id(database: &Database, name: &str) -> Option<usize> {
    population_names(database)
        .iter()
        .position(|candidate| candidate.trim().eq_ignore_ascii_case(name.trim()))
}

pub(crate) fn object_population_type_id(
    database: &Database,
    prototype: &ProtoObject,
    amount: &PopulationAmount,
) -> Option<usize> {
    let population_name = amount
        .population_type
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| inferred_population_name(prototype));
    population_type_id(database, population_name)
}

fn resolve_object_amounts(
    database: &Database,
    prototype: &ProtoObject,
    amounts: &[PopulationAmount],
    round: bool,
) -> Vec<PopulationCost> {
    let slot_count = population_names(database).len();
    let mut totals = vec![0.0; slot_count];
    for amount in amounts {
        if !amount.amount.is_finite() || amount.amount <= 0.0 {
            continue;
        }
        let Some(population_type) = object_population_type_id(database, prototype, amount) else {
            continue;
        };
        totals[population_type] += amount.amount;
    }
    totals
        .into_iter()
        .enumerate()
        .filter_map(|(population_type, amount)| {
            let amount = if round {
                (amount + 0.5).floor()
            } else {
                amount
            };
            (amount > 0.0).then_some(PopulationCost::new(population_type, amount))
        })
        .collect()
}

fn population_names(database: &Database) -> &[String] {
    database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.pops.as_ref())
        .map_or(&[][..], |pops| pops.entries.as_slice())
}

fn inferred_population_name(prototype: &ProtoObject) -> &'static str {
    if prototype.name.to_ascii_lowercase().contains("spartan") {
        "Spartan"
    } else if prototype.name.to_ascii_lowercase().contains("rhino") {
        "Rhino"
    } else if prototype
        .object_types
        .iter()
        .any(|object_type| object_type.eq_ignore_ascii_case("Temple"))
    {
        "Temple"
    } else if prototype.object_types.iter().any(|object_type| {
        object_type.eq_ignore_ascii_case("Leader") || object_type.eq_ignore_ascii_case("_Leader")
    }) {
        "Leader"
    } else {
        "Unit"
    }
}
