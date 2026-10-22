//! Skirmish AI scripts of a scenario load.
//!
//! This is the `HH` step of `BScenario_load` @ `0x140377BE0`. It runs after the game-mode
//! scripts of the `GG` block.

use pipeline::database::hw1::Database;

use super::LoadedScenario;
use super::game_mode::{SKIRMISH_AI_ON_SCENARIO_CONFIG, ScriptLoader, launch};
use crate::player::PlayerType;
use crate::trigger::{LoadResult, TriggerCost, TriggerExternals};
use crate::world::{GameType, World};

/// Trigger script file of the skirmish AI (`BAI_startSkirmishAIScript` @ `0x14014E4D0`).
pub(super) const SKIRMISH_AI_SCRIPT: &str = "SkirmishAI_raw_.triggerscript";

/// Config that stops the skirmish AI scripts (`gConfigAIDisable`).
const AI_DISABLE_CONFIG: &str = "AIDisable";

/// Config that sets every resource of the skirmish AI `Cost` external to 1.
const AI_NO_ATTACK_CONFIG: &str = "AINoAttack";

/// Start the skirmish AI scripts like `BWorld_initPlayerAIs` @ `0x1405D1A30`.
///
/// `BWorld_initPlayerAIs` creates a `BAI` for each player other than Gaia that is not human, in
/// player ID order. The `BAI` constructor (`BAI_construct` @ `0x14014E220`) calls
/// `BAI_startSkirmishAIScript` @ `0x14014E4D0`. That launches [`SKIRMISH_AI_SCRIPT`] for a
/// computer AI player if:
///
/// - the `AIDisable` config is not defined, and
/// - the game is a skirmish, or a scenario game with the `SkirmishAIOnScenario` config.
///
/// The script gets the Player external and the Cost external. The cost is zero, or 1 for each
/// gamedata resource with the `AINoAttack` config.
///
/// # Errors
///
/// Returns an error if the script file does not parse.
pub(super) fn start_skirmish_ai_scripts(
    scenario: &mut LoadedScenario,
    database: &Database,
    load: &mut ScriptLoader<'_>,
) -> LoadResult<()> {
    let world = &scenario.world;
    let game_type_allows = match world.game_type() {
        GameType::Skirmish => true,
        GameType::Scenario => world.is_config_defined(SKIRMISH_AI_ON_SCENARIO_CONFIG),
        GameType::Campaign => false,
    };
    if !game_type_allows || world.is_config_defined(AI_DISABLE_CONFIG) {
        return Ok(());
    }
    let cost = skirmish_ai_cost(world, database);
    let players = world
        .active_players()
        .filter(|player| player.player_type == PlayerType::ComputerAi)
        .map(|player| player.id)
        .collect::<Vec<_>>();
    for player_id in players {
        let externals = TriggerExternals {
            player: Some(i32::from(player_id)),
            cost: Some(cost),
            ..TriggerExternals::default()
        };
        let _script = launch(scenario, load, SKIRMISH_AI_SCRIPT, externals)?;
    }
    Ok(())
}

/// The Cost external of a skirmish AI script.
fn skirmish_ai_cost(world: &World, database: &Database) -> TriggerCost {
    let mut cost = TriggerCost::default();
    if world.is_config_defined(AI_NO_ATTACK_CONFIG) {
        let resource_count = database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.resources.as_ref())
            .map_or(0, |resources| resources.entries.len());
        // `BCost` keeps four resources; `TriggerCost::set` ignores the others.
        for resource_id in 0..resource_count {
            cost.set(resource_id, 1.0);
        }
    }
    cost
}

#[cfg(test)]
#[path = "player_ais/tests.rs"]
mod tests;
