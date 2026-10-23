//! World state tests.

use super::*;
use crate::scenario::{ScenarioData, load_test_scenario as load_scenario_into_world};
use crate::simulation::Simulation;
use pipeline::database::hw1::Database;

/// Sample scenario for testing
const TEST_SCENARIO: &str = r#"<?xml version="1.0"?>
<Scenario>
  <Players>
<Player Name="TestPlayer1" Civ="UNSC" Leader1="Cutter" Team="1" />
<Player Name="TestPlayer2" Civ="Covenant" Leader1="Arbiter" Team="2" />
  </Players>
  <Objects>
<Object IsSquad="true" Player="1" ID="0" Position="10.0,0.0,20.0">unsc_inf_marine_01</Object>
<Object IsSquad="true" Player="2" ID="1" Position="50.0,0.0,60.0">cov_inf_grunt_01</Object>
  </Objects>
</Scenario>"#;

/// Run a simulation for a given number of ticks and return the final checksum.
fn run_simulation(seed: u64, ticks: u32) -> (u32, u32) {
    let scenario = ScenarioData::from_xml_str(TEST_SCENARIO).unwrap();
    let db = Database::new();
    let mut loaded = load_scenario_into_world(&scenario, &db);
    loaded.world.rng.set_seed64(seed);

    let mut sim = Simulation::with_seed(seed);
    sim.start();

    for _ in 0..ticks {
        sim.tick_with_world(&mut loaded.world);
    }

    (loaded.world.checksum(), loaded.world.checksum_with_rng())
}

#[test]
fn test_world_checksum_deterministic() {
    // Same world should always produce same checksum
    let mut world1 = World::with_seed(12_345);
    world1.init_players(2);
    world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

    let mut world2 = World::with_seed(12_345);
    world2.init_players(2);
    world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

    assert_eq!(world1.checksum(), world2.checksum());
}

#[test]
fn test_world_checksum_different_state() {
    // Different state should produce different checksum
    let mut world1 = World::with_seed(12_345);
    world1.init_players(2);
    world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

    let mut world2 = World::with_seed(12_345);
    world2.init_players(2);
    world2.create_squad_at(1, glam::Vec3::new(15.0, 0.0, 25.0)); // Different position

    assert_ne!(world1.checksum(), world2.checksum());
}

#[test]
fn test_simulation_determinism_same_seed() {
    // Run simulation twice with same seed - must produce identical checksums
    let seed = 42;
    let ticks = 100;

    let (cs1, cs1_rng) = run_simulation(seed, ticks);
    let (cs2, cs2_rng) = run_simulation(seed, ticks);

    assert_eq!(cs1, cs2, "World checksum mismatch after {ticks} ticks");
    assert_eq!(
        cs1_rng, cs2_rng,
        "World+RNG checksum mismatch after {ticks} ticks"
    );
}

#[test]
fn test_simulation_different_seeds_different_rng() {
    // Different seeds should produce different RNG states
    // Note: World state (without RNG) may be identical if no random operations occur
    let (_, cs1_rng) = run_simulation(42, 100);
    let (_, cs2_rng) = run_simulation(43, 100);

    assert_ne!(
        cs1_rng, cs2_rng,
        "Different seeds should produce different RNG states"
    );
}

#[test]
fn test_simulation_determinism_long_run() {
    // Run for longer to catch subtle non-determinism
    let seed = 99_999;
    let ticks = 1_000;

    let (cs1, cs1_rng) = run_simulation(seed, ticks);
    let (cs2, cs2_rng) = run_simulation(seed, ticks);

    assert_eq!(cs1, cs2, "World checksum mismatch after {ticks} ticks");
    assert_eq!(
        cs1_rng, cs2_rng,
        "World+RNG checksum mismatch after {ticks} ticks"
    );
}

#[test]
fn test_simulation_checksum_changes_over_time() {
    // Checksum should change as simulation progresses (state evolves)
    let seed = 12_345;

    let (cs_10, _) = run_simulation(seed, 10);
    let (cs_100, _) = run_simulation(seed, 100);

    // At minimum, game_time_ms changes, so checksums should differ
    assert_ne!(cs_10, cs_100, "Checksum should change over time");
}

/// Run simulation with movement commands for determinism testing.
fn run_simulation_with_movement(seed: u64, ticks: u32) -> (u32, u32) {
    let scenario = ScenarioData::from_xml_str(TEST_SCENARIO).unwrap();
    let db = Database::new();
    let mut loaded = load_scenario_into_world(&scenario, &db);
    loaded.world.rng.set_seed64(seed);

    let mut sim = Simulation::with_seed(seed);
    sim.start();

    // Issue move commands to squads after a few ticks
    for tick in 0..ticks {
        if tick == 10 {
            // Issue move orders to all squads
            for (_id, squad) in loaded.world.squads.iter_mut() {
                let target = glam::Vec3::new(100.0, 0.0, 100.0);
                squad.move_to(target);
            }
        }
        sim.tick_with_world(&mut loaded.world);
    }

    (loaded.world.checksum(), loaded.world.checksum_with_rng())
}

#[test]
fn test_simulation_determinism_with_movement() {
    // Run simulation with movement twice - must be identical
    let seed = 7_777;
    let ticks = 200;

    let (cs1, cs1_rng) = run_simulation_with_movement(seed, ticks);
    let (cs2, cs2_rng) = run_simulation_with_movement(seed, ticks);

    assert_eq!(
        cs1, cs2,
        "World checksum mismatch with movement after {ticks} ticks"
    );
    assert_eq!(
        cs1_rng, cs2_rng,
        "World+RNG checksum mismatch with movement after {ticks} ticks"
    );
}

#[test]
fn test_movement_changes_checksum() {
    // Directly verify that issuing a move command changes the checksum
    let mut world1 = World::with_seed(12_345);
    world1.init_players(2);
    let squad_id = world1.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

    let mut world2 = World::with_seed(12_345);
    world2.init_players(2);
    let squad_id2 = world2.create_squad_at(1, glam::Vec3::new(10.0, 0.0, 20.0));

    // Initially should be equal
    assert_eq!(world1.checksum(), world2.checksum());

    // Issue move command to world2's squad
    if let Some(squad) = world2.get_squad_mut(squad_id2) {
        squad.move_to(glam::Vec3::new(100.0, 0.0, 100.0));
    }

    // Now checksums should differ (squad has move_target set)
    assert_ne!(
        world1.checksum(),
        world2.checksum(),
        "Move command should change checksum"
    );

    // Run some ticks on world2 to actually move
    for _ in 0..20 {
        world2.update_entities(0.05); // 50ms
        world2.game_time_ms += 50;
    }

    // Position should have changed
    let squad1_pos = world1.get_squad(squad_id).unwrap().position();
    let squad2_pos = world2.get_squad(squad_id2).unwrap().position();
    assert_ne!(
        squad1_pos, squad2_pos,
        "Squad should have moved after update"
    );
}

#[test]
fn buildings_share_the_unit_pool() {
    let mut world = World::new();
    let building_id = world.create_building_at(1, Vec3::new(4.0, 0.0, 8.0));

    assert_eq!(building_id.class(), Some(EntityClass::Unit));
    assert!(world.get_building(building_id).is_some());
    assert!(!world.get_unit_mut(building_id).unwrap().move_to(Vec3::X));
}

#[test]
fn detached_units_stop_inheriting_squad_motion() {
    let mut world = World::new();
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let unit_id = world.create_unit_at(1, Vec3::X);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::X * 10.0);
    world.update_entities(0.1);

    assert!(world.detach_unit_from_squad(unit_id));
    let detached_position = world.get_unit(unit_id).unwrap().base.position;
    assert_eq!(world.get_unit(unit_id).unwrap().base.velocity, Vec3::ZERO);
    assert!(world.get_unit_mut(unit_id).unwrap().move_to(Vec3::X * 20.0));
    world.update_entities(0.1);

    assert_ne!(
        world.get_unit(unit_id).unwrap().base.position,
        detached_position
    );
}
