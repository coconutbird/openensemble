use glam::Vec3;
use sim::{
    load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    squad_prototype_id,
};

const MARINE: &str = "unsc_inf_marine_01";
const WARTHOG: &str = "unsc_veh_warthog_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ammunition -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_drives_authoritative_ammunition_and_technology() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());
    let mut loaded =
        load_scenario_from_game_dir(&game_dir, &scenario).expect("real scenario should load");
    let database = &loaded.content.database;

    let marine_proto = find_object(database, MARINE);
    assert_eq!(marine_proto.ammo_max, Some(200.0));
    assert_eq!(marine_proto.ammo_regen_rate, Some(9.0));
    assert!(!has_flag(&marine_proto.flags, "StartAtMaxAmmo"));
    let warthog_proto = find_object(database, WARTHOG);
    assert_eq!(warthog_proto.ammo_max, Some(800.0));
    assert_eq!(warthog_proto.ammo_regen_rate, Some(40.0));
    assert!(has_flag(&warthog_proto.flags, "StartAtMaxAmmo"));

    let marine_id = spawn_object_at(
        &mut loaded.simulation.world,
        database,
        1,
        object_prototype_id(database, MARINE).unwrap(),
        Vec3::ZERO,
        Vec3::Z,
    )
    .unwrap();
    let warthog_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        squad_prototype_id(database, WARTHOG).unwrap(),
        Vec3::X * 10.0,
        Vec3::Z,
    )
    .unwrap();
    let warthog_id = loaded
        .simulation
        .world
        .get_squad(warthog_squad_id)
        .unwrap()
        .unit_ids[0];

    assert_ammunition(&loaded.simulation.world, marine_id, 0.0, 200.0, 9.0);
    assert_ammunition(&loaded.simulation.world, warthog_id, 800.0, 800.0, 40.0);
    assert_eq!(
        loaded.simulation.world.squad_ammunition(warthog_squad_id),
        Some((800.0, 800.0))
    );

    loaded.simulation.world.update_entities(0.05);
    assert_ammunition(&loaded.simulation.world, marine_id, 0.0, 200.0, 9.0);
    loaded.simulation.world.update_entities(0.05);
    assert_ammunition(&loaded.simulation.world, marine_id, 0.45, 200.0, 9.0);

    loaded
        .simulation
        .world
        .get_unit_mut(warthog_id)
        .unwrap()
        .ammunition
        .set_current(400.0);
    assert!(
        loaded
            .simulation
            .world
            .activate_technology(1, database, "unsc_warthog_upgrade1")
            .unwrap()
    );
    assert_ammunition(&loaded.simulation.world, warthog_id, 500.0, 1_000.0, 50.0);
    assert_eq!(
        loaded.simulation.world.squad_ammunition(warthog_squad_id),
        Some((500.0, 1_000.0))
    );
}

fn find_object<'database>(
    database: &'database pipeline::database::hw1::Database,
    name: &str,
) -> &'database pipeline::database::hw1::ProtoObject {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
        .expect("shipped prototype")
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags.iter().any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn assert_ammunition(
    world: &sim::World,
    unit_id: sim::EntityId,
    current: f32,
    maximum: f32,
    rate: f32,
) {
    let ammunition = world
        .unit_ammunition(unit_id)
        .expect("live ammunition unit");
    assert!(ammunition.is_enabled());
    assert_close(ammunition.current(), current);
    assert_close(ammunition.maximum(), maximum);
    assert_close(ammunition.regeneration_rate(), rate);
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= 0.000_1 * expected.abs().max(1.0));
}
