use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    GameplayCatalog, World, load_scenario_from_game_dir, object_prototype_id, spawn_object_at,
    spawn_squad_at, squad_prototype_id,
};

const AIR_PAD: &str = "unsc_bldg_airPad_01";
const HEAVY_FACTORY: &str = "cov_bldg_heavyfactory_01";
const FLOOD_SWARM: &str = "fld_air_swarm_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-air-traffic-control -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_air_bases_initialize_source_exact_landing_spots() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profiles(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().civ_id = civilization_id(database, "UNSC");
    world.get_player_mut(2).unwrap().civ_id = civilization_id(database, "Covenant");
    let center = scenario_center(world);
    let air_pad = spawn_controller(world, database, 1, AIR_PAD, center, Vec3::Z);
    let factory_position = center + Vec3::X * 40.0;
    let heavy_factory =
        spawn_controller(world, database, 2, HEAVY_FACTORY, factory_position, Vec3::X);

    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);

    let unsc = world
        .get_unit(air_pad)
        .unwrap()
        .production
        .air_traffic_control()
        .expect("shipped Air Pad controller state");
    assert_eq!(unsc.action_name(), "AirTrafficControl");
    assert_eq!(unsc.landing_spots().len(), 8);
    assert_close(
        unsc.landing_spots()[0].position(),
        center + Vec3::new(3.0, 3.0, 0.0),
    );
    assert_close(
        unsc.landing_spots()[7].position(),
        center + Vec3::new(-6.0, 3.0, -31.0),
    );
    assert_close(
        unsc.landing_spots()[0].forward(),
        Vec3::new(1.0, 0.0, -1.0).normalize(),
    );

    let covenant = world
        .get_unit(heavy_factory)
        .unwrap()
        .production
        .air_traffic_control()
        .expect("shipped Heavy Factory controller state");
    assert_eq!(covenant.action_name(), "AirTrafficControl");
    assert_eq!(covenant.landing_spots().len(), 8);
    assert_close(
        covenant.landing_spots()[0].position(),
        factory_position + Vec3::new(20.0, 3.0, 0.0),
    );
    assert_close(covenant.landing_spots()[0].forward(), Vec3::X);
    assert_close(
        covenant.landing_spots()[2].position(),
        factory_position + Vec3::new(0.0, 3.0, -20.0),
    );
    assert_close(covenant.landing_spots()[2].forward(), Vec3::NEG_Z);

    assert_unlinked_move_air_does_not_reserve(
        world,
        database,
        gameplay,
        center,
        [air_pad, heavy_factory],
    );
}

fn assert_unlinked_move_air_does_not_reserve(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    center: Vec3,
    controllers: [sim::EntityId; 2],
) {
    let swarm_squad = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, FLOOD_SWARM).expect("shipped Flood swarm squad"),
        center + Vec3::new(0.0, 12.0, 60.0),
        Vec3::X,
    )
    .expect("shipped Flood swarm should spawn");
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
    for controller_id in controllers {
        assert!(
            world
                .get_unit(controller_id)
                .unwrap()
                .production
                .air_traffic_control()
                .unwrap()
                .landing_spots()
                .iter()
                .all(|spot| spot.aircraft_id().is_none()),
            "conjured MoveAir units have no train-limit link and must not reserve a pad"
        );
    }
    for unit_id in world
        .get_squad(swarm_squad)
        .unwrap()
        .unit_ids
        .iter()
        .copied()
    {
        let unit = world.get_unit(unit_id).unwrap();
        assert!(!unit.is_move_air_parked());
        assert_eq!(unit.move_air_base_id(), None);
    }

    for prototype in [AIR_PAD, HEAVY_FACTORY] {
        let prototype = database
            .objects
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(prototype))
            .expect("shipped controller prototype");
        assert!(
            prototype.train_limits.is_empty(),
            "shipped multiplayer controller has no MoveAir train-limit link"
        );
    }
}

fn assert_shipped_profiles(gameplay: &GameplayCatalog) {
    for prototype in [AIR_PAD, HEAVY_FACTORY] {
        let profiles = gameplay.air_traffic_control_actions(prototype);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].action_name(), "AirTrafficControl");
        assert!(!profiles[0].starts_disabled());
    }
}

fn spawn_controller(
    world: &mut World,
    database: &Database,
    player_id: sim::PlayerId,
    prototype: &str,
    position: Vec3,
    forward: Vec3,
) -> sim::EntityId {
    spawn_object_at(
        world,
        database,
        player_id,
        object_prototype_id(database, prototype).expect("shipped air-base object"),
        position,
        forward,
    )
    .expect("shipped air base should spawn")
}

fn civilization_id(database: &Database, name: &str) -> i32 {
    database
        .civs
        .iter()
        .position(|civilization| civilization.name.eq_ignore_ascii_case(name))
        .and_then(|index| i32::try_from(index).ok())
        .expect("shipped civilization")
}

fn scenario_center(world: &World) -> Vec3 {
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut position = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    position.y = world.terrain_height(position, true).unwrap_or_default();
    position
}

fn assert_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.000_1),
        "expected {actual:?} to equal {expected:?}"
    );
}
