//! Queries for per-player prototype `Forbid` flags.

use super::{Condition, TriggerScript, as_i32, as_player_id, value_at};
use crate::world::World;
use pipeline::database::hw1::Database;

pub(super) fn is_forbidden(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> bool {
    let Some(database) = database else {
        return false;
    };
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(player) = world.get_player(player_id) else {
        return false;
    };

    value_at(condition, script, 2)
        .and_then(as_i32)
        .is_some_and(|id| player.is_squad_forbidden(database, id))
        || value_at(condition, script, 3)
            .and_then(as_i32)
            .is_some_and(|id| player.is_object_forbidden(database, id))
        || value_at(condition, script, 4)
            .and_then(as_i32)
            .is_some_and(|id| player.is_technology_forbidden(database, id))
}

#[cfg(test)]
mod tests;
