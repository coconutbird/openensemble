//! Initial per-player prototype forbids authored directly in an SCN file.

use super::ScenarioPlayer;
use crate::spawn::{object_prototype_id, squad_prototype_id};
use crate::world::{World, technology_prototype_id};
use pipeline::database::hw1::Database;

/// Apply the `ForbidObjects`, `ForbidSquads` and `ForbidTechs` lists of each scenario player.
///
/// `BScenario_loadPlayer` @ `0x1403808a0` looks up each untrimmed name and
/// keeps only the names that the database knows.
pub(super) fn apply_scenario_forbids(
    world: &mut World,
    database: &Database,
    players: &[ScenarioPlayer],
) {
    for (index, scenario_player) in players.iter().take(crate::world::MAX_PLAYERS).enumerate() {
        let player_id = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let Some(player) = world.get_player_mut(player_id) else {
            continue;
        };
        for name in scenario_player.forbidden_objects() {
            if let Some(id) = object_prototype_id(database, name) {
                let _changed = player.set_object_forbidden(database, id, true);
            }
        }
        for name in scenario_player.forbidden_squads() {
            if let Some(id) = squad_prototype_id(database, name) {
                let _changed = player.set_squad_forbidden(database, id, true);
            }
        }
        for name in scenario_player.forbidden_techs() {
            if let Some(id) = technology_prototype_id(database, name) {
                let _changed = player.set_technology_forbidden(database, id, true);
            }
        }
    }
}

#[cfg(test)]
mod tests;
