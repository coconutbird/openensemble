//! Authoritative per-player resource updates and resource-cost resolution.

use super::World;
use crate::player::{MAX_RESOURCES, Resources};
use pipeline::database::hw1::Database;

impl World {
    /// Apply each playing player's configured per-second resource trickle.
    pub fn update_player_resources(&mut self, elapsed_seconds: f32) {
        for player in &mut self.players {
            player.update_resource_trickle(elapsed_seconds);
        }
    }
}

/// Failure to resolve a list of named resource costs.
///
/// Each caller maps this error into its own error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceCostError<'a> {
    /// The costs are not empty, but the database has no resource table.
    MissingResourceTable,
    /// The amount for this resource name is negative or not finite.
    InvalidCost(&'a str),
    /// The database has no usable resource slot for this resource name.
    UnknownResource(&'a str),
}

/// Resolve named resource costs into one `Resources` value.
///
/// An empty cost list gives zero costs, also without a resource table. The
/// function examines each entry in order:
///
/// 1. Reject a negative or nonfinite amount.
/// 2. Find the first resource with the trimmed name, without case.
/// 3. Reject a resource index at or above `MAX_RESOURCES`.
/// 4. Add the amount to the resource slot.
pub(super) fn resolve_resource_cost<'a>(
    database: &Database,
    entries: impl IntoIterator<Item = (&'a str, f32)>,
) -> Result<Resources, ResourceCostError<'a>> {
    let mut entries = entries.into_iter().peekable();
    if entries.peek().is_none() {
        return Ok(Resources::new());
    }
    let resources = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .ok_or(ResourceCostError::MissingResourceTable)?;
    let mut cost = Resources::new();
    for (resource, amount) in entries {
        if !amount.is_finite() || amount < 0.0 {
            return Err(ResourceCostError::InvalidCost(resource));
        }
        let resource_id = resources
            .entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(resource.trim()))
            .filter(|index| *index < MAX_RESOURCES)
            .ok_or(ResourceCostError::UnknownResource(resource))?;
        cost.add(resource_id, amount);
    }
    Ok(cost)
}

#[cfg(test)]
mod tests;
