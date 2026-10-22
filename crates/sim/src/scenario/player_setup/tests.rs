use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::leaders::ResourceEntry;
use pipeline::database::hw1::playercolors::{PlayerColorCiv, PlayerColorSlot};
use pipeline::database::hw1::{Civ, GameData, Leader, PlayerColors};

use super::*;
use crate::scenario::PlayerSettings;

fn civ(name: &str) -> Civ {
    Civ {
        name: name.to_owned(),
        ..Civ::default()
    }
}

fn leader(name: &str, civ: &str) -> Leader {
    Leader {
        name: name.to_owned(),
        civ: Some(civ.to_owned()),
        ..Leader::default()
    }
}

/// Civs `None`, `UNSC` and `Covenant`. The UNSC leaders are `Random` (random flag), `Cutter`,
/// `Forge`, `TestLeader` (test flag) and `Anders`; `Cutter` starts with 800 supplies. The
/// colour rows are the retail rows: UNSC `0, 1, 2, 3, 4, 5, 6, 13, 14` and Covenant
/// `0, 7, 8, 9, 10, 11, 12, 13, 14`.
fn database() -> Database {
    let resource = |name: &str| ResourceDef {
        name: name.to_owned(),
        ..ResourceDef::default()
    };
    let civ_colors = |name: &str, colors: [u32; 9]| PlayerColorCiv {
        name: name.to_owned(),
        players: (0..)
            .zip(colors)
            .map(|(num, color)| PlayerColorSlot {
                num: Some(num),
                color: Some(color),
            })
            .collect(),
    };
    Database {
        civs: vec![civ("None"), civ("UNSC"), civ("Covenant")],
        leaders: vec![
            Leader {
                random: Some(true),
                ..leader("Random", "UNSC")
            },
            Leader {
                resources: vec![ResourceEntry {
                    resource_type: "Supplies".to_owned(),
                    amount: 800.0,
                }],
                ..leader("Cutter", "UNSC")
            },
            leader("Forge", "UNSC"),
            Leader {
                test: Some(true),
                ..leader("TestLeader", "UNSC")
            },
            leader("Anders", "UNSC"),
            leader("Arbiter", "Covenant"),
        ],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![resource("Supplies"), resource("Power")],
            }),
            ..GameData::default()
        }),
        player_colors: Some(PlayerColors {
            civs: vec![
                civ_colors("UNSC", [0, 1, 2, 3, 4, 5, 6, 13, 14]),
                civ_colors("Covenant", [0, 7, 8, 9, 10, 11, 12, 13, 14]),
            ],
            ..PlayerColors::default()
        }),
        ..Database::default()
    }
}

fn setup(scenario: &str, settings: &GameSettings, configs: &[&str]) -> (World, PlayerSetup) {
    let scenario = ScenarioData::from_xml_str(scenario).unwrap();
    let mut world = World::new();
    world.seed_sim_random(settings.random_seed);
    for config in configs {
        let _defined = world.define_config(*config);
    }
    let setup = setup_players(&mut world, &scenario, &database(), settings).unwrap();
    (world, setup)
}

#[test]
fn controllable_players_fill_the_lobby_slots_and_the_others_become_npcs() {
    // `BScenario_setupPlayers`: controllable players take slots 1..=PlayerCount in document
    // order (slot type 1 = computer AI, else human, name `PlayerNName` or "Player N"); a
    // controllable player beyond PlayerCount gets no world player; a non-controllable player
    // is an NPC (type 0) with its `.scn` name. World IDs follow the document order.
    let settings = GameSettings::skirmish(
        vec![
            PlayerSettings {
                name: Some("Host".to_owned()),
                ..PlayerSettings::default()
            },
            PlayerSettings {
                slot_type: SlotType::ComputerAi,
                ..PlayerSettings::default()
            },
        ],
        0,
    );
    let (world, setup) = setup(
        r#"<Scenario><Players>
            <Player Name="Alpha" Civ="UNSC" Leader1="Cutter" Team="1" />
            <Player Name="Creeps" Controllable="false" Civ="Covenant" Leader1="Arbiter" Team="3" />
            <Player Name="Beta" Civ="UNSC" Leader1="Forge" Team="2" />
            <Player Name="Gamma" Civ="UNSC" Leader1="Anders" Team="2" />
        </Players></Scenario>"#,
        &settings,
        &[],
    );

    let players = world
        .players()
        .map(|player| (player.name.as_str(), player.player_type, player.team_id))
        .collect::<Vec<_>>();
    assert_eq!(
        players,
        [
            ("Gaia", PlayerType::Npc, 0),
            ("Host", PlayerType::Human, 1),
            ("Creeps", PlayerType::Npc, 3),
            ("Player 2", PlayerType::ComputerAi, 2),
        ]
    );
    // The scenario-player to world-player map of `BScenario_loadObject`
    // (`gScenarioPlayerToWorldPlayer`).
    let owners = (-1..=9)
        .map(|player| setup.object_owner(player))
        .collect::<Vec<_>>();
    assert_eq!(
        owners,
        [
            None,
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            None,
            None,
            None,
            None,
            None,
            None
        ]
    );
}

#[test]
fn a_random_leader_is_a_civ_leader_without_random_or_test_flag_picked_by_the_seed() {
    // `BScenario_pickRandomLeaderForCiv` @ 0x1403861B0: the candidates of UNSC are Cutter,
    // Forge and Anders; `getRandomRange(0, 2)` on the sync stream seeded with `RandomSeed`
    // picks one. Seeds 0, 3334 and 6678 draw 0, 1 and 2.
    let scenario = r#"<Scenario><Players>
        <Player Name="P1" UsePlayerSettings="true" Team="1" />
    </Players></Scenario>"#;
    let picked = |random_seed, leader: Option<&str>, configs: &[&str]| {
        let settings = GameSettings::skirmish(
            vec![PlayerSettings {
                civ: Some("UNSC".to_owned()),
                leader: leader.map(str::to_owned),
                ..PlayerSettings::default()
            }],
            random_seed,
        );
        let (world, _) = setup(scenario, &settings, configs);
        let leader_id = world.get_player(1).unwrap().leader_id;
        database().leaders[usize::try_from(leader_id).unwrap()]
            .name
            .clone()
    };

    assert_eq!(picked(0, None, &[]), "Cutter");
    assert_eq!(picked(3334, None, &[]), "Forge");
    assert_eq!(picked(6678, None, &[]), "Anders");
    // The `Random` leader of the lobby is a random choice too.
    assert_eq!(picked(6678, Some("Random"), &[]), "Anders");
    // `UseTestLeaders` takes the first `Test` leader of the civ without a draw.
    assert_eq!(picked(6678, None, &["UseTestLeaders"]), "TestLeader");
}

#[test]
fn default_resources_keep_the_leader_resources_and_otherwise_the_scn_values_win() {
    // `BPlayer_setup` seeds the leader resources; with `DefaultResources="false"` the loop at
    // 0x140382DA1 sets each resource to its `.scn` attribute, 0 without one.
    let settings = GameSettings::skirmish(vec![PlayerSettings::default(); 2], 0);
    let (world, _) = setup(
        r#"<Scenario><Players>
            <Player Name="P1" Civ="UNSC" Leader1="Cutter" Team="1" Supplies="150" />
            <Player Name="P2" Civ="UNSC" Leader1="Cutter" Team="2" DefaultResources="false"
                Supplies="150" />
        </Players></Scenario>"#,
        &settings,
        &[],
    );

    let resources = |player_id| {
        let player = world.get_player(player_id).unwrap();
        [player.get_resource(0), player.get_resource(1)]
    };
    assert_eq!(resources(1), [800.0, 0.0]);
    assert_eq!(resources(2), [150.0, 0.0]);
}

fn colors(world: &World) -> Vec<(String, u8)> {
    world
        .players()
        .map(|player| (player.name.clone(), player.color))
        .collect()
}

#[test]
fn each_player_takes_the_next_colour_of_its_civ_row() {
    // Loop at 0x140382FC0: the colour is `civRow[civ][ordinal]` with
    // `ordinal = 1 + world players 1..id with the same civ`. Gaia keeps the `BPlayer_ctor`
    // colour -1 (255).
    let settings = GameSettings::skirmish(vec![PlayerSettings::default(); 4], 0);
    let (world, _) = setup(
        r#"<Scenario><Players>
            <Player Name="P1" UsePlayerSettings="true" Civ="UNSC" Leader1="Cutter" Team="1"
                Color="9" />
            <Player Name="P2" Civ="Covenant" Leader1="Arbiter" Team="2" />
            <Player Name="P3" Civ="UNSC" Leader1="Forge" Team="1" />
            <Player Name="P4" Civ="Covenant" Leader1="Arbiter" Team="2" />
        </Players></Scenario>"#,
        &settings,
        &[],
    );

    // P1 uses the player settings, so it ignores its `Color`.
    assert_eq!(
        colors(&world),
        [
            ("Gaia".to_owned(), u8::MAX),
            ("Player 1".to_owned(), 1),
            ("Player 2".to_owned(), 7),
            ("Player 3".to_owned(), 2),
            ("Player 4".to_owned(), 8),
        ]
    );
}

#[test]
fn a_scenario_colour_wins_only_without_player_settings() {
    // Check at 0x140383034: `UsePlayerSettings="false"` and `Color != -1` stores `Color`.
    // Without a `Color` the civ row applies, and the player still counts for later players.
    let settings = GameSettings::skirmish(vec![PlayerSettings::default(); 1], 0);
    let (world, _) = setup(
        r#"<Scenario><Players>
            <Player Name="Creeps" Controllable="false" UsePlayerSettings="false" Civ="UNSC"
                Leader1="Cutter" Team="3" Color="13" />
            <Player Name="Rebels" Controllable="false" UsePlayerSettings="false" Civ="UNSC"
                Leader1="Cutter" Team="3" />
            <Player Name="P1" Civ="UNSC" Leader1="Cutter" Team="1" />
        </Players></Scenario>"#,
        &settings,
        &[],
    );

    assert_eq!(
        colors(&world),
        [
            ("Gaia".to_owned(), u8::MAX),
            ("Creeps".to_owned(), 13),
            ("Rebels".to_owned(), 2),
            ("Player 1".to_owned(), 3),
        ]
    );
}

#[test]
fn a_civ_outside_the_colour_rows_gives_colour_minus_one() {
    // 0x140383139: a civ outside 0..=4 stores -1; the player keeps its low byte.
    let mut database = database();
    database.civs.extend(["A", "B", "C"].map(civ));
    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario><Players>
            <Player Name="P1" Civ="C" Team="1" />
        </Players></Scenario>"#,
    )
    .unwrap();
    let mut world = World::new();
    let settings = GameSettings::skirmish(vec![PlayerSettings::default()], 0);
    setup_players(&mut world, &scenario, &database, &settings).unwrap();

    assert_eq!(world.get_player(1).unwrap().civ_id, 5);
    assert_eq!(world.get_player(1).unwrap().color, u8::MAX);
}
