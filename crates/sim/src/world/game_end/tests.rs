use super::*;
use crate::command::Command;
use crate::command_queue::CommandEntry;
use crate::commands::{CommandPayload, GameCommand, GameCommandType};
use crate::executor::CommandExecutor;
use crate::player::{PlayerStateEvent, PlayerType};
use crate::simulation::Simulation;
use crate::trigger::{
    Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar, VarType,
};

/// Create a world with one player per `(team, type)` entry, in player ID order from 1.
fn world_with(players: &[(TeamId, PlayerType)]) -> World {
    let mut world = World::new();
    world.init_players(u8::try_from(players.len()).unwrap());
    for (index, &(team, player_type)) in players.iter().enumerate() {
        let player = world
            .get_player_mut(u8::try_from(index + 1).unwrap())
            .unwrap();
        player.team_id = team;
        player.player_type = player_type;
    }
    world
}

fn state(world: &World, player_id: PlayerId) -> PlayerState {
    world.get_player(player_id).unwrap().state
}

fn alive(world: &World, unit_id: crate::EntityId) -> bool {
    world.get_unit(unit_id).is_some_and(Entity::is_alive)
}

fn game_command(player_id: i32, game_type: GameCommandType) -> CommandEntry {
    CommandEntry {
        command: CommandPayload::Game(GameCommand {
            base: Command {
                player_id,
                ..Command::default()
            },
            game_type,
            ..GameCommand::default()
        }),
        exec_time: 0,
        sequence: 0,
        source_client: 0,
    }
}

/// `BWorld_notify` runs `BWorld_checkForGameOver` for event 54. In a 1v1 skirmish team 2 has no
/// playing player left, so team 1 wins in the same call: player 1 changes to `Won` (event 56),
/// and the defeated player keeps its units because the cleanup does not run after the game
/// ended (`BWorld_updatePlayerUnitCleanup`).
#[test]
fn defeat_in_one_versus_one_ends_the_game_at_once() {
    let mut world = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    world.game_time_ms = 7_000;
    let loser_unit = world.create_unit(2);

    assert!(world.set_player_state(2, PlayerState::Defeated));

    assert_eq!(
        world.game_result(),
        Some(GameResult {
            winning_team: Some(1),
            game_time_ms: 7_000,
        })
    );
    assert_eq!(state(&world, 1), PlayerState::Won);
    assert_eq!(world.get_player(1).unwrap().end_time_seconds(), Some(7));
    let events: Vec<_> = world
        .take_player_state_changes()
        .iter()
        .map(|change| (change.player_id, change.event))
        .collect();
    assert_eq!(
        events,
        [
            (2, Some(PlayerStateEvent::Defeated)),
            (1, Some(PlayerStateEvent::Won)),
        ]
    );
    world.update_player_unit_cleanup();
    assert!(alive(&world, loser_unit));
    assert!(world.get_player(2).unwrap().units_cleanup_pending());
}

/// A skirmish team stays alive while one member plays. The defeated player's units die at the
/// next update except a prototype with `UngarrisonToGaia`. The game ends when the second member
/// is defeated, and both members of team 2 win.
#[test]
fn two_versus_two_needs_the_whole_team() {
    let mut world = world_with(&[
        (1, PlayerType::Human),
        (1, PlayerType::ComputerAi),
        (2, PlayerType::ComputerAi),
        (2, PlayerType::Human),
    ]);
    let first_unit = world.create_unit(1);
    let kept_unit = world.create_unit(1);
    world
        .get_unit_mut(kept_unit)
        .unwrap()
        .set_ungarrison_to_gaia(true);
    let ally_unit = world.create_unit(2);

    assert!(world.set_player_state(1, PlayerState::Defeated));
    assert_eq!(world.game_result(), None);
    world.update_player_unit_cleanup();
    assert!(!alive(&world, first_unit));
    assert!(alive(&world, kept_unit));
    assert!(alive(&world, ally_unit));
    assert!(!world.get_player(1).unwrap().units_cleanup_pending());

    assert!(world.set_player_state(2, PlayerState::Defeated));
    assert_eq!(world.game_result().unwrap().winning_team, Some(2));
    assert_eq!(state(&world, 3), PlayerState::Won);
    assert_eq!(state(&world, 4), PlayerState::Won);
}

/// The skirmish rule also ends the game when no human player plays. Team 1 keeps a playing AI,
/// so both teams are alive, and retail declares the last alive team (2) the winner.
#[test]
fn skirmish_ends_when_no_human_plays() {
    let mut world = world_with(&[
        (1, PlayerType::Human),
        (1, PlayerType::ComputerAi),
        (2, PlayerType::ComputerAi),
    ]);
    assert!(world.resign_player(1));
    assert_eq!(world.game_result().unwrap().winning_team, Some(2));
    assert_eq!(state(&world, 2), PlayerState::Playing);
    assert_eq!(state(&world, 3), PlayerState::Won);
}

/// Only teams 1 and 2 count. A third team does not keep the game going, and a team-0 player
/// cannot win the skirmish.
#[test]
fn skirmish_counts_only_teams_one_and_two() {
    let mut world = world_with(&[
        (1, PlayerType::Human),
        (2, PlayerType::ComputerAi),
        (3, PlayerType::Npc),
        (0, PlayerType::ComputerAi),
    ]);
    assert!(world.set_player_state(2, PlayerState::Defeated));
    assert_eq!(world.game_result().unwrap().winning_team, Some(1));
    assert_eq!(state(&world, 3), PlayerState::Playing);
    assert_eq!(state(&world, 4), PlayerState::Playing);
}

/// Game commands 7 and 8 (`BPlayer_resign`, `BPlayer_disconnect`) act only on a playing player.
/// Both post event 53; disconnect also sets the disconnected flag.
#[test]
fn resign_and_disconnect_commands_change_only_playing_players() {
    let mut world = world_with(&[
        (1, PlayerType::Human),
        (1, PlayerType::Human),
        (2, PlayerType::Human),
    ]);
    let executor = CommandExecutor::new();
    executor.execute_all(&mut world, &[game_command(1, GameCommandType::Resign)]);
    assert_eq!(state(&world, 1), PlayerState::Resigned);
    assert!(!world.get_player(1).unwrap().disconnected());
    assert_eq!(world.game_result(), None);

    executor.execute_all(&mut world, &[game_command(1, GameCommandType::Disconnect)]);
    assert!(!world.get_player(1).unwrap().disconnected());

    executor.execute_all(&mut world, &[game_command(2, GameCommandType::Disconnect)]);
    assert_eq!(state(&world, 2), PlayerState::Resigned);
    assert!(world.get_player(2).unwrap().disconnected());
    assert_eq!(world.game_result().unwrap().winning_team, Some(2));
    let events: Vec<_> = world
        .player_state_changes()
        .iter()
        .map(|change| (change.player_id, change.event))
        .collect();
    assert_eq!(
        events,
        [
            (1, Some(PlayerStateEvent::Resigned)),
            (2, Some(PlayerStateEvent::Resigned)),
            (3, Some(PlayerStateEvent::Won)),
        ]
    );
}

/// Campaign and scenario games end as soon as a human player stops playing. A human who won
/// makes the team win; otherwise retail uses team 0, so team-0 players also change to `Won`.
#[test]
fn campaign_ends_when_a_human_leaves() {
    let mut world = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    world.set_game_type(GameType::Campaign);
    assert!(world.set_player_state(2, PlayerState::Defeated));
    assert_eq!(world.game_result(), None);
    assert!(world.set_player_state(1, PlayerState::Won));
    assert_eq!(world.game_result().unwrap().winning_team, Some(1));

    let mut world = world_with(&[
        (1, PlayerType::Human),
        (2, PlayerType::ComputerAi),
        (0, PlayerType::Npc),
    ]);
    world.set_game_type(GameType::Scenario);
    assert!(world.set_player_state(1, PlayerState::Defeated));
    assert_eq!(world.game_result().unwrap().winning_team, Some(0));
    assert_eq!(state(&world, 2), PlayerState::Playing);
    assert_eq!(state(&world, 3), PlayerState::Won);
}

/// `BWorld_update` skips the trigger update once the game ended, but the update that ended it
/// runs to its end: T1 still fires after T0 defeated player 2. T2 waits for a late time, so the
/// script stays alive but is not evaluated again.
#[test]
fn triggers_stop_after_the_update_that_ended_the_game() {
    let mut world = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    let mut script = TriggerScript::default();
    for (id, var_type, value) in [
        (0, VarType::Player, TriggerValue::Player(2)),
        (
            1,
            VarType::PlayerState,
            TriggerValue::Int(PlayerState::Defeated as i32),
        ),
        (2, VarType::Integer, TriggerValue::Int(1)),
        (3, VarType::Integer, TriggerValue::Int(0)),
        (4, VarType::Time, TriggerValue::Time(1_000_000)),
    ] {
        script.add_variable(TriggerVar::new(id, var_type).with_value(value));
    }
    let mut defeat = Effect::new(0, EffectType::SetPlayerState)
        .with_input_at(1, 0)
        .with_input_at(3, 1);
    defeat.version = 2;
    let count = Effect::new(0, EffectType::CountIncrement)
        .with_input_at(1, 2)
        .with_output_at(2, 3);
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(defeat));
    script.add_trigger(Trigger::new(1).starts_active().with_effect_on_true(count));
    script.add_trigger(
        Trigger::new(2).starts_active().with_condition(
            crate::trigger::Condition::new(0, crate::trigger::ConditionType::GameTimeReached)
                .with_input_at(1, 4),
        ),
    );
    let engine = world.trigger_engine_mut();
    let script_id = engine.add_script(script);
    engine.activate_script(script_id, 0);

    let first = world.update_triggers();
    assert_eq!(first.triggers_fired, 2);
    assert!(world.is_game_over());
    let counter = |world: &World| {
        world
            .trigger_engine()
            .get_script(script_id)
            .unwrap()
            .get_variable(3)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(
        counter(&world),
        TriggerValue::Int(2),
        "CountIncrement wrote 1 + 1"
    );
    assert_eq!(
        world.update_triggers(),
        crate::trigger::TriggerUpdate::default()
    );
}

/// `BTimeManager_getUpdateTime` subtracts each update from `GameOverDelay` and keeps updating
/// while more than 1e-6 s remain. With 0.1 s and 50 ms ticks, one more tick runs.
#[test]
fn game_over_delay_limits_the_ticks_after_the_game_ended() {
    let mut world = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    world.set_game_over_delay(Some(0.1));
    let mut simulation = Simulation::new();
    let _tick = simulation.tick_with_world(&mut world);
    assert_eq!(world.game_time_ms, 50);
    assert!(world.set_player_state(2, PlayerState::Defeated));

    let _after = simulation.tick_with_world(&mut world);
    assert_eq!(world.game_time_ms, 100);
    let _stopped = simulation.tick_with_world(&mut world);
    let _still_stopped = simulation.tick_with_world(&mut world);
    assert_eq!(world.game_time_ms, 100);

    let mut no_delay = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    assert!(no_delay.set_player_state(1, PlayerState::Resigned));
    assert!(!no_delay.advance_game_over_delay(0.05));
}

/// The game type, the delay and the result are synchronized state.
#[test]
fn checksum_covers_the_game_end_state() {
    let mut world = world_with(&[(1, PlayerType::Human), (2, PlayerType::ComputerAi)]);
    let start = world.checksum();
    world.set_game_type(GameType::Scenario);
    let scenario = world.checksum();
    assert_ne!(start, scenario);
    world.set_game_over_delay(Some(5.0));
    let delayed = world.checksum();
    assert_ne!(scenario, delayed);
    let _changes = world.set_player_state(1, PlayerState::Resigned);
    let _taken = world.take_player_state_changes();
    assert_ne!(delayed, world.checksum());
}
