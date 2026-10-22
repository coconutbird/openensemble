//! Game-mode scripts of a scenario load.
//!
//! This is the `GG` block of `BScenario_load` @ `0x140377BE0`. It runs after the scenario
//! trigger systems, the `.sc2` and `.sc3` objects, the starting units and the simulation bounds.

use pipeline::database::hw1::Database;

use super::{LoadedScenario, player_setup};
use crate::player::{PlayerId, PlayerType};
use crate::trigger::{LoadResult, TriggerExternals, TriggerScriptId};
use crate::world::{GameType, World};

/// Config that makes player 2 of a scenario game a computer AI player with an AI script.
pub(super) const SKIRMISH_AI_ON_SCENARIO_CONFIG: &str = "SkirmishAIOnScenario";

/// Config that stops the game-mode player scripts (`gConfigNoVictoryCondition`).
const NO_VICTORY_CONDITION_CONFIG: &str = "NoVictoryCondition";

/// Parse a trigger script file into the script library if necessary.
///
/// Returns `Ok(false)` if the file does not exist.
pub(super) type ScriptLoader<'a> = dyn FnMut(&mut LoadedScenario, &str) -> LoadResult<bool> + 'a;

/// Start the game mode like the `GG` block of `BScenario_load`.
///
/// Only a skirmish (game type 0) has a game mode. A scenario game (game type 2) with the
/// `SkirmishAIOnScenario` config makes player 2 a computer AI player instead. For the game mode
/// named by `game_mode` the block, in order:
///
/// 1. Adds the mode `NPC` player if no NPC player has its name (case-insensitive).
/// 2. Activates the mode `Tech` for each human and computer AI player.
/// 3. Launches the `WorldScript` once.
/// 4. Unless the `NoVictoryCondition` config is defined, launches the `PlayerScript` for each
///    human and computer AI player in player ID order, with the Player external
///    (`BTriggerManager_setScriptPlayer` @ `0x140489390`).
///
/// # Errors
///
/// Returns an error if a script file does not parse.
pub(super) fn start_game_mode(
    scenario: &mut LoadedScenario,
    database: &Database,
    game_mode: &str,
    load: &mut ScriptLoader<'_>,
) -> LoadResult<()> {
    let world = &mut scenario.world;
    match world.game_type() {
        GameType::Skirmish => {}
        GameType::Scenario => {
            if world.is_config_defined(SKIRMISH_AI_ON_SCENARIO_CONFIG)
                && let Some(player) = world.get_player_mut(2)
            {
                player.player_type = PlayerType::ComputerAi;
            }
            return Ok(());
        }
        GameType::Campaign => return Ok(()),
    }
    let Some(mode) = database
        .game_modes
        .iter()
        .find(|mode| mode.name.eq_ignore_ascii_case(game_mode))
    else {
        return Ok(());
    };
    if let Some(npc) = mode.npc.as_deref().filter(|name| !name.is_empty())
        && !has_npc_player(world, npc)
    {
        let _npc = player_setup::add_npc_player(world, database, npc);
    }
    if let Some(tech) = mode.tech_name() {
        for player_id in script_players(world) {
            // `BGameMode_getTechID` is -1 for an unknown tech and the block then skips it.
            let _activation = world.activate_technology(player_id, database, tech);
        }
    }
    if let Some(world_script) = mode.world_script.as_deref().filter(|name| !name.is_empty()) {
        let _script = launch(scenario, load, world_script, TriggerExternals::default())?;
    }
    if let Some(player_script) = mode
        .player_script
        .as_deref()
        .filter(|name| !name.is_empty())
        && !scenario
            .world
            .is_config_defined(NO_VICTORY_CONDITION_CONFIG)
    {
        for player_id in script_players(&scenario.world) {
            let externals = TriggerExternals {
                player: Some(i32::from(player_id)),
                ..TriggerExternals::default()
            };
            let _script = launch(scenario, load, player_script, externals)?;
        }
    }
    Ok(())
}

/// Return the human and computer AI players in player ID order.
fn script_players(world: &World) -> Vec<PlayerId> {
    world
        .active_players()
        .filter(|player| {
            matches!(
                player.player_type,
                PlayerType::Human | PlayerType::ComputerAi
            )
        })
        .map(|player| player.id)
        .collect()
}

/// Return whether an NPC player has `name` (`BString` compare, case-insensitive).
fn has_npc_player(world: &World, name: &str) -> bool {
    world.active_players().any(|player| {
        player.player_type == PlayerType::Npc && player.name.eq_ignore_ascii_case(name)
    })
}

/// Load a script file, then create and activate it with its externals.
pub(super) fn launch(
    scenario: &mut LoadedScenario,
    load: &mut ScriptLoader<'_>,
    file_name: &str,
    externals: TriggerExternals,
) -> LoadResult<Option<TriggerScriptId>> {
    if !load(scenario, file_name)? {
        return Ok(None);
    }
    let current_time = scenario.world.game_time_ms;
    Ok(scenario
        .world
        .trigger_engine_mut()
        .launch_script(file_name, externals, current_time))
}

#[cfg(test)]
#[path = "game_mode/tests.rs"]
mod tests;
