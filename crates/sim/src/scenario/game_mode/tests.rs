use pipeline::database::hw1::{GameMode, Tech};

use super::*;
use crate::player::TeamRelation;
use crate::scenario::{
    GameSettings, PlayerSettings, ScenarioData, SlotType, load_scenario_into_world,
};
use crate::trigger::{INVALID_TRIGGER_SCRIPT_ID, TriggerScript};

const WORLD_SCRIPT: &str = "SkirmishWorld.triggerscript";
const PLAYER_SCRIPT: &str = "SkirmishPlayer.triggerscript";

/// A human in slot 1, a computer AI in slot 2 and the NPC `Creeps`.
fn skirmish(mode: GameMode) -> (LoadedScenario, Database) {
    let database = Database {
        game_modes: vec![
            GameMode {
                name: "Other".to_owned(),
                ..GameMode::default()
            },
            mode,
        ],
        techs: vec![Tech {
            name: "ModeTech".to_owned(),
            ..Tech::default()
        }],
        ..Database::default()
    };
    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario><Players>
            <Player Name="Player1" UsePlayerSettings="true" Team="1" />
            <Player Name="Creeps" Controllable="false" Team="3" />
            <Player Name="Player2" UsePlayerSettings="true" Team="2" />
        </Players></Scenario>"#,
    )
    .unwrap();
    let settings = GameSettings::skirmish(
        vec![
            PlayerSettings {
                slot_type: SlotType::Human,
                ..PlayerSettings::default()
            },
            PlayerSettings {
                slot_type: SlotType::ComputerAi,
                ..PlayerSettings::default()
            },
        ],
        0,
    );
    let mut loaded = load_scenario_into_world(&scenario, &database, &settings).unwrap();
    let library = loaded.world.trigger_engine_mut().script_library_mut();
    library.insert(WORLD_SCRIPT, TriggerScript::new(INVALID_TRIGGER_SCRIPT_ID));
    library.insert(PLAYER_SCRIPT, TriggerScript::new(INVALID_TRIGGER_SCRIPT_ID));
    (loaded, database)
}

fn skirmish_mode() -> GameMode {
    GameMode {
        name: "Skirmish".to_owned(),
        world_script: Some(WORLD_SCRIPT.to_owned()),
        player_script: Some(PLAYER_SCRIPT.to_owned()),
        ..GameMode::default()
    }
}

/// Start the game mode with a loader that finds the scripts of the library.
fn start(loaded: &mut LoadedScenario, database: &Database, game_mode: &str) {
    let mut load = |scenario: &mut LoadedScenario, file_name: &str| {
        Ok(scenario
            .world
            .trigger_engine()
            .script_library()
            .contains(file_name))
    };
    start_game_mode(loaded, database, game_mode, &mut load).unwrap();
}

/// The file name and Player external of each script, in script ID order.
fn scripts(loaded: &LoadedScenario) -> Vec<(String, Option<i32>)> {
    loaded
        .world
        .trigger_engine()
        .scripts()
        .map(|(_, script)| (script.name.clone(), script.externals.player))
        .collect()
}

#[test]
fn skirmish_launches_world_script_then_player_scripts_in_player_order() {
    // `BScenario_load` GG (0x14037B63F-0x14037B72A): the WorldScript once, then the
    // PlayerScript with `BTriggerManager_setScriptPlayer` for each player of type 1 or 2.
    let (mut loaded, database) = skirmish(skirmish_mode());
    let world = &loaded.world;
    assert_eq!(world.get_player(1).unwrap().player_type, PlayerType::Human);
    assert_eq!(world.get_player(2).unwrap().player_type, PlayerType::Npc);
    assert_eq!(
        world.get_player(3).unwrap().player_type,
        PlayerType::ComputerAi
    );

    start(&mut loaded, &database, "skirmish");

    assert_eq!(
        scripts(&loaded),
        [
            (WORLD_SCRIPT.to_owned(), None),
            (PLAYER_SCRIPT.to_owned(), Some(1)),
            (PLAYER_SCRIPT.to_owned(), Some(3)),
        ]
    );
    assert!(
        loaded
            .world
            .trigger_engine()
            .scripts()
            .all(|(_, script)| script.is_active)
    );
}

#[test]
fn no_victory_condition_config_skips_only_the_player_scripts() {
    // The PlayerScript loop tests `gConfigNoVictoryCondition` (`dword_1414B0530`, registered as
    // "NoVictoryCondition" by `sub_1401CB990`); the WorldScript does not.
    let (mut loaded, database) = skirmish(skirmish_mode());
    let _defined = loaded.world.define_config("NoVictoryCondition");

    start(&mut loaded, &database, "Skirmish");

    assert_eq!(scripts(&loaded), [(WORLD_SCRIPT.to_owned(), None)]);
}

#[test]
fn mode_tech_goes_to_human_and_computer_ai_players_only() {
    // `BTechTree_activateTech(BPlayer_getTechTree(p), BGameMode_getTechID(mode), -1, 0, 0)` for
    // `BPlayer_isComputerAI || BPlayer_isHuman`.
    let (mut loaded, database) = skirmish(GameMode {
        tech: Some("ModeTech".to_owned()),
        ..skirmish_mode()
    });

    start(&mut loaded, &database, "Skirmish");

    let has_tech = |player_id| {
        loaded
            .world
            .get_player(player_id)
            .unwrap()
            .technologies
            .active_technologies()
            .any(|tech| tech == "ModeTech")
    };
    assert!(has_tech(1));
    assert!(!has_tech(2));
    assert!(has_tech(3));
}

#[test]
fn mode_npc_player_is_added_in_a_new_enemy_team_unless_an_npc_has_its_name() {
    // GG: `BWorld_addPlayer(-1, NPC, ..., civ 0, leader 0, default difficulty, type 0)`, then
    // `BWorld_addTeam` with relations ally to itself, neutral (4) to team 0, enemy (3) else.
    let (mut loaded, database) = skirmish(GameMode {
        npc: Some("Rebels".to_owned()),
        ..skirmish_mode()
    });

    start(&mut loaded, &database, "Skirmish");

    let world = &loaded.world;
    assert_eq!(world.player_count(), 5);
    let rebels = world.get_player(4).unwrap();
    assert_eq!(rebels.name, "Rebels");
    assert_eq!(rebels.player_type, PlayerType::Npc);
    assert_eq!(rebels.team_id, 4);
    assert_eq!(world.team_relation(4, 4), TeamRelation::Ally);
    assert_eq!(world.team_relation(4, 0), TeamRelation::Neutral);
    assert_eq!(world.team_relation(0, 4), TeamRelation::Neutral);
    assert_eq!(world.team_relation(1, 4), TeamRelation::Enemy);
    assert_eq!(world.team_relation(4, 3), TeamRelation::Enemy);
    // The NPC gets no player script.
    assert_eq!(scripts(&loaded).len(), 3);

    let (mut loaded, database) = skirmish(GameMode {
        npc: Some("CREEPS".to_owned()),
        ..skirmish_mode()
    });
    start(&mut loaded, &database, "Skirmish");
    assert_eq!(loaded.world.player_count(), 4);
}

#[test]
fn other_game_types_and_unknown_modes_launch_nothing() {
    // GG runs only for `GameType == 0`, and stops when `BDatabase_getGameMode` finds no mode.
    let (mut loaded, database) = skirmish(skirmish_mode());
    start(&mut loaded, &database, "CapFlag");
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);

    let (mut loaded, database) = skirmish(skirmish_mode());
    loaded.world.set_game_type(GameType::Campaign);
    start(&mut loaded, &database, "Skirmish");
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);

    // Scenario game type: only `SkirmishAIOnScenario` makes player 2 a computer AI
    // (`BPlayer_setTypeComputerAI`).
    let (mut loaded, database) = skirmish(skirmish_mode());
    loaded.world.set_game_type(GameType::Scenario);
    let _defined = loaded.world.define_config("skirmishaionscenario");
    start(&mut loaded, &database, "Skirmish");
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);
    assert_eq!(
        loaded.world.get_player(2).unwrap().player_type,
        PlayerType::ComputerAi
    );
}

#[test]
fn missing_script_file_launches_nothing() {
    let (mut loaded, database) = skirmish(GameMode {
        world_script: Some("Missing.triggerscript".to_owned()),
        ..skirmish_mode()
    });

    start(&mut loaded, &database, "Skirmish");

    assert_eq!(
        scripts(&loaded),
        [
            (PLAYER_SCRIPT.to_owned(), Some(1)),
            (PLAYER_SCRIPT.to_owned(), Some(3)),
        ]
    );
}
