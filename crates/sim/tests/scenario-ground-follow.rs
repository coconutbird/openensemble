//! Checks that the Blood Gulch Warthog drives over the hill south of the player 1 base on its
//! wheels: the squad moves, the `BPhysicsWarthogAction` drives the body after it, and the
//! readback puts the unit on the terrain.

use glam::Vec3;
use sim::{EntityId, LoadedGameScenario, MS_PER_TICK, Simulation, SquadState, WorkCommand};

const WARTHOG: &str = "unsc_veh_warthog_01";
/// The route from the player 1 Warthog runs south over the crest of the hill beside the player 1
/// base (crest about 90 units south) and down its far side.
const ROUTE: Vec3 = Vec3::new(0.0, 0.0, -120.0);
/// Largest height difference between the unit origin and the terrain under it. At rest on flat
/// ground the unit origin is 0.15 under the terrain: the springs hold the body origin
/// 2.13 above it, and the `CenterOffset` of the `warthog` info is 2.28.
const TERRAIN_TOLERANCE: f32 = 0.5;
/// Smallest climb of the unit along the route, and smallest descent after its highest point.
const MINIMUM_CLIMB: f32 = 6.0;
const MINIMUM_DESCENT: f32 = 2.0;
/// `BPhysicsWarthogAction` brakes at the goal and stops within this distance of it.
const GOAL_TOLERANCE: f32 = 1.5;
const TICKS: usize = 400;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ground-follow -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_warthog_drives_over_the_hill_on_its_wheels() {
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
        let world = &loaded.simulation.world;
        let unit = world.get_unit(unit_id).expect("the Warthog stays alive");
        let in_air = unit
            .physics_object
            .as_ref()
            .and_then(sim::physics::UnitPhysics::vehicle_in_air)
            .expect("the Warthog has its vehicle action");
        assert!(!in_air, "tick {tick}: no wheel touches the terrain");
        let position = unit.base.position;
        let terrain = world
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
    assert!(
        highest - end.y > MINIMUM_DESCENT,
        "the Warthog came down only {} units",
        highest - end.y
    );
    let squad = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .expect("the Warthog squad");
    assert_eq!(squad.state, SquadState::Idle);
    let miss = Vec3::new(end.x - target.x, 0.0, end.z - target.z).length();
    assert!(
        miss < GOAL_TOLERANCE,
        "the Warthog stopped {miss} from the goal"
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
