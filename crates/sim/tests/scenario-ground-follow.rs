//! Checks that the Blood Gulch Warthog follows the installed terrain up a hill.

use glam::Vec3;
use sim::{EntityId, LoadedGameScenario, MS_PER_TICK, Simulation, WorkCommand};

const WARTHOG: &str = "unsc_veh_warthog_01";
/// The route from the player 1 Warthog runs south over the hill beside the player 1 base.
const ROUTE: Vec3 = Vec3::new(0.0, 0.0, -100.0);
/// Largest height difference between the Warthog origin and the terrain under it.
const TERRAIN_TOLERANCE: f32 = 0.5;
/// Smallest climb of the Warthog origin along the route.
const MINIMUM_CLIMB: f32 = 6.0;
const TICKS: usize = 200;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ground-follow -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_warthog_follows_the_terrain_over_a_hill() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let mut clock = Simulation::new();
    clock.start();
    let (unit_id, squad_id) = player_warthog(&loaded);
    let start = unit_position(&loaded, unit_id);
    let target = start + ROUTE;
    let route_peak = route_peak(&loaded, start, target);
    assert!(
        route_peak - start.y > MINIMUM_CLIMB + 2.0,
        "the route must cross a hill (peak {route_peak}, start {start:?})"
    );

    let exec_time = clock.game_time_ms.saturating_add(MS_PER_TICK);
    clock.command_queue.enqueue_work(
        WorkCommand::user_work(1, vec![squad_id], target, None, false),
        exec_time,
        1,
    );
    let mut highest = start.y;
    let mut largest_gap = 0.0_f32;
    for tick in 0..TICKS {
        clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        let position = unit_position(&loaded, unit_id);
        let terrain = loaded
            .simulation
            .world
            .terrain_height(position, true)
            .expect("Blood Gulch has simulation terrain");
        let gap = position.y - terrain;
        assert!(
            gap.abs() < TERRAIN_TOLERANCE,
            "tick {tick}: the Warthog is at {position:?}, the terrain is at {terrain}"
        );
        largest_gap = largest_gap.max(gap.abs());
        highest = highest.max(position.y);
    }
    let end = unit_position(&loaded, unit_id);
    println!(
        "start {start:?}, end {end:?}, highest {highest}, route peak {route_peak}, \
         largest gap {largest_gap}"
    );
    assert!(
        highest - start.y > MINIMUM_CLIMB,
        "the Warthog climbed only {} units",
        highest - start.y
    );
}

fn player_warthog(loaded: &LoadedGameScenario) -> (EntityId, EntityId) {
    let (unit_id, unit) = loaded
        .simulation
        .world
        .units
        .iter()
        .find(|(_, unit)| {
            unit.base.player_id == 1 && unit.proto_object_name.eq_ignore_ascii_case(WARTHOG)
        })
        .expect("player 1 starts with a Warthog");
    (unit_id, unit.squad_id.expect("the Warthog is in a squad"))
}

fn unit_position(loaded: &LoadedGameScenario, unit_id: EntityId) -> Vec3 {
    loaded
        .simulation
        .world
        .get_unit(unit_id)
        .expect("the Warthog stays alive")
        .base
        .position
}

fn route_peak(loaded: &LoadedGameScenario, start: Vec3, target: Vec3) -> f32 {
    (0..=100)
        .filter_map(|step| {
            let point = start.lerp(target, f32::from(u8::try_from(step).ok()?) / 100.0);
            loaded.simulation.world.terrain_height(point, true)
        })
        .fold(f32::MIN, f32::max)
}
