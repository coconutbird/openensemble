//! Population checks of a construction prototype.

use super::ConstructionError;
use crate::scenario::population::object_population_type_id;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::ProtoObject;

pub(super) fn validate_object_population(
    database: &Database,
    prototype: &ProtoObject,
) -> Result<(), ConstructionError> {
    for amount in &prototype.population {
        if !amount.amount.is_finite() || amount.amount < 0.0 {
            return Err(unknown_population(
                prototype,
                amount.population_type.as_deref(),
            ));
        }
        if amount.amount == 0.0 {
            continue;
        }
        if database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.pops.as_ref())
            .is_none()
        {
            return Err(ConstructionError::MissingPopulationTable(
                prototype.name.clone(),
            ));
        }
        if object_population_type_id(database, prototype, amount).is_none() {
            return Err(unknown_population(
                prototype,
                amount.population_type.as_deref(),
            ));
        }
    }
    Ok(())
}

fn unknown_population(prototype: &ProtoObject, population: Option<&str>) -> ConstructionError {
    ConstructionError::UnknownPopulation {
        prototype: prototype.name.clone(),
        population: population.unwrap_or("inferred default").to_owned(),
    }
}
