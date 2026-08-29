//! Scenario-database resource cost resolution for construction.

use super::ConstructionError;
use crate::player::{MAX_RESOURCES, Resources};
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::ProtoObject;

pub(super) fn construction_cost(
    database: &Database,
    prototype: &ProtoObject,
) -> Result<Resources, ConstructionError> {
    if prototype.costs.is_empty() {
        return Ok(Resources::new());
    }
    let resources = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .ok_or_else(|| ConstructionError::MissingResourceTable(prototype.name.clone()))?;
    let mut cost = Resources::new();
    for entry in &prototype.costs {
        if !entry.amount.is_finite() || entry.amount < 0.0 {
            return Err(ConstructionError::InvalidCost {
                prototype: prototype.name.clone(),
                resource: entry.resource_type.clone(),
            });
        }
        let resource_id = resources
            .entries
            .iter()
            .position(|resource| {
                resource
                    .name
                    .eq_ignore_ascii_case(entry.resource_type.trim())
            })
            .filter(|index| *index < MAX_RESOURCES)
            .ok_or_else(|| ConstructionError::UnknownResource {
                prototype: prototype.name.clone(),
                resource: entry.resource_type.clone(),
            })?;
        cost.add(resource_id, entry.amount);
    }
    Ok(cost)
}
