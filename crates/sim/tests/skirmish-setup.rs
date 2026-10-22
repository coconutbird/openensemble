//! Checks the tick-0 state of a 1v1 Blood Gulch skirmish against `BScenario_load`.

use sim::PlayerType;

fn count_units(world: &sim::World, proto: &str, player: Option<u8>) -> usize {
    world
        .units
        .iter()
        .filter(|(_, unit)| unit.proto_object_name.eq_ignore_ascii_case(proto))
        .filter(|(_, unit)| player.is_none_or(|player| unit.base.player_id == player))
        .count()
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test skirmish-setup -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_1v1_matches_the_retail_tick_zero_setup() {
    let loaded = test_support::installed_scenario("blood_gulch");
    let world = &loaded.simulation.world;
    let database = &loaded.content.database;
    let leader = |name: &str| {
        database
            .leaders
            .iter()
            .position(|leader| leader.name == name)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap()
    };

    // `BScenario_setupPlayers`: Gaia, two lobby players and the Creeps NPC.
    let players = world.players().collect::<Vec<_>>();
    assert_eq!(players.len(), 4);
    assert_eq!(players[1].player_type, PlayerType::Human);
    assert_eq!(players[1].leader_id, leader("Cutter"));
    assert_eq!(players[2].player_type, PlayerType::ComputerAi);
    assert_eq!(players[2].leader_id, leader("Arbiter"));
    assert_eq!(players[3].name, "Creeps");
    assert_eq!(players[3].player_type, PlayerType::Npc);
    assert!(
        players[3].leader_id >= 0,
        "Creeps get a random Covenant leader"
    );
    for player in &players[1..=2] {
        assert!((player.get_resource(0) - 800.0).abs() < f32::EPSILON);
    }

    // `BScenario_createStartingUnits`: Gaia sockets with unbuilt HQs and the leader squads.
    for (player, hq) in [(1, "unsc_bldg_command_03"), (2, "cov_bldg_builder_02")] {
        let hq_id = world
            .units
            .iter()
            .find(|(_, unit)| unit.proto_object_name == hq && unit.base.player_id == player)
            .map(|(id, _)| id)
            .expect("starting HQ");
        assert!(!world.get_unit(hq_id).unwrap().built);
        assert!(loaded.simulation.get_initial_base_id(player).is_some());
    }
    assert_eq!(count_units(world, "unsc_veh_warthog_01", Some(1)), 1);
    assert_eq!(count_units(world, "cov_veh_ghost_01", Some(2)), 1);

    // `.sc2` trees and `.sc3` sound objects.
    let objects = |prefix: &str| {
        world
            .objects
            .iter()
            .filter(|(_, object)| object.proto_object_name.starts_with(prefix))
            .count()
    };
    assert_eq!(objects("env_arcadia_pinetree"), 35);
    assert_eq!(objects("snd_play"), 18);
}
