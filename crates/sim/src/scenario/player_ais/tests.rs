use pipeline::database::hw1::GameData;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};

use super::*;
use crate::scenario::{
    GameSettings, PlayerSettings, ScenarioData, SlotType, load_scenario_into_world,
};
use crate::trigger::{INVALID_TRIGGER_SCRIPT_ID, TriggerScript};

/// A human in slot 1, the NPC `Creeps`, and computer AIs in slots 2 and 3.
fn skirmish() -> (LoadedScenario, Database) {
    let database = Database {
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: ["Supplies", "Power"]
                    .map(|name| ResourceDef {
                        name: name.to_owned(),
                        ..ResourceDef::default()
                    })
                    .into(),
            }),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario><Players>
            <Player Name="Player1" UsePlayerSettings="true" Team="1" />
            <Player Name="Creeps" Controllable="false" Team="3" />
            <Player Name="Player2" UsePlayerSettings="true" Team="2" />
            <Player Name="Player3" UsePlayerSettings="true" Team="2" />
        </Players></Scenario>"#,
    )
    .unwrap();
    let slot = |slot_type| PlayerSettings {
        slot_type,
        ..PlayerSettings::default()
    };
    let settings = GameSettings::skirmish(
        vec![
            slot(SlotType::Human),
            slot(SlotType::ComputerAi),
            slot(SlotType::ComputerAi),
        ],
        0,
    );
    let mut loaded = load_scenario_into_world(&scenario, &database, &settings).unwrap();
    loaded
        .world
        .trigger_engine_mut()
        .script_library_mut()
        .insert(
            SKIRMISH_AI_SCRIPT,
            TriggerScript::new(INVALID_TRIGGER_SCRIPT_ID),
        );
    (loaded, database)
}

/// Start the AI scripts with a loader that finds the scripts of the library.
fn start(loaded: &mut LoadedScenario, database: &Database) {
    let mut load = |scenario: &mut LoadedScenario, file_name: &str| {
        Ok(scenario
            .world
            .trigger_engine()
            .script_library()
            .contains(file_name))
    };
    start_skirmish_ai_scripts(loaded, database, &mut load).unwrap();
}

/// The file name and the Player and Cost externals of each script, in script ID order.
fn scripts(loaded: &LoadedScenario) -> Vec<(String, Option<i32>, Option<TriggerCost>)> {
    loaded
        .world
        .trigger_engine()
        .scripts()
        .map(|(_, script)| {
            (
                script.name.clone(),
                script.externals.player,
                script.externals.cost,
            )
        })
        .collect()
}

#[test]
fn each_computer_ai_player_gets_the_ai_script_with_player_and_zero_cost() {
    // `BWorld_initPlayerAIs`: a `BAI` for each non-human player in ID order;
    // `BAI_startSkirmishAIScript` launches the script only for player type 1, then
    // `setScriptPlayer` and `setExternalCost` with a zero cost.
    let (mut loaded, database) = skirmish();
    let player_types = loaded
        .world
        .players()
        .map(|player| player.player_type)
        .collect::<Vec<_>>();
    assert_eq!(
        player_types,
        [
            PlayerType::Npc,
            PlayerType::Human,
            PlayerType::Npc,
            PlayerType::ComputerAi,
            PlayerType::ComputerAi,
        ]
    );

    start(&mut loaded, &database);

    let zero = Some(TriggerCost::default());
    assert_eq!(
        scripts(&loaded),
        [
            (SKIRMISH_AI_SCRIPT.to_owned(), Some(3), zero),
            (SKIRMISH_AI_SCRIPT.to_owned(), Some(4), zero),
        ]
    );
}

#[test]
fn ai_no_attack_config_sets_one_for_each_gamedata_resource() {
    // With `AINoAttack` the cost loop sets 1.0 for each of the `dword_1414F98C0` resources.
    let (mut loaded, database) = skirmish();
    let _defined = loaded.world.define_config("AINoAttack");

    start(&mut loaded, &database);

    let mut expected = TriggerCost::default();
    expected.set(0, 1.0);
    expected.set(1, 1.0);
    assert!(
        scripts(&loaded)
            .iter()
            .all(|(_, _, cost)| *cost == Some(expected))
    );
}

#[test]
fn ai_disable_and_game_type_gates_stop_the_ai_scripts() {
    // `gConfigAIDisable` stops every AI script.
    let (mut loaded, database) = skirmish();
    let _defined = loaded.world.define_config("AIDisable");
    start(&mut loaded, &database);
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);

    // A campaign never starts them.
    let (mut loaded, database) = skirmish();
    loaded.world.set_game_type(GameType::Campaign);
    start(&mut loaded, &database);
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);

    // A scenario game starts them only with `SkirmishAIOnScenario`.
    let (mut loaded, database) = skirmish();
    loaded.world.set_game_type(GameType::Scenario);
    start(&mut loaded, &database);
    assert_eq!(loaded.world.trigger_engine().script_count(), 0);
    let _defined = loaded.world.define_config("SkirmishAIOnScenario");
    start(&mut loaded, &database);
    assert_eq!(loaded.world.trigger_engine().script_count(), 2);
}
