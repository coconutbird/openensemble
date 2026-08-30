use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    GameplayCatalog, HealPhase, World, load_scenario_from_game_dir, spawn_squad_at,
    squad_prototype_id,
};

const MEDIC: &str = "unsc_inf_medic_01";
const MEDIC_SQUAD: &str = "unsc_inf_marine_03";
const MONITOR: &str = "for_air_monitor_02";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-heal -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_and_tactics_drive_medic_healing() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profiles(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let medic_squad = spawn_medic(world, database);
    let medic_id = world
        .get_squad(medic_squad)
        .unwrap()
        .unit_ids
        .iter()
        .copied()
        .find(|unit_id| {
            world
                .get_unit(*unit_id)
                .is_some_and(|unit| gameplay.heal(&unit.proto_object_name).is_some())
        })
        .expect("spawned Marine squad Medic");
    let maximum = world.get_unit(medic_id).unwrap().max_hitpoints;
    world.get_unit_mut(medic_id).unwrap().hitpoints = maximum * 0.5;

    for _ in 0..61 {
        tick(world, database, gameplay);
    }
    assert_eq!(
        world.get_unit(medic_id).unwrap().heal_phase(),
        HealPhase::Waiting
    );
    assert_close(world.get_unit(medic_id).unwrap().hitpoints, maximum * 0.5);

    tick(world, database, gameplay);
    assert_eq!(
        world.get_unit(medic_id).unwrap().heal_phase(),
        HealPhase::Working
    );
    let before = world.get_unit(medic_id).unwrap().hitpoints;
    tick(world, database, gameplay);
    assert_close(world.get_unit(medic_id).unwrap().hitpoints - before, 2.5);
}

fn assert_shipped_profiles(gameplay: &GameplayCatalog) {
    let medic = gameplay
        .heal(MEDIC)
        .expect("scenario-layered Medic Heal action");
    assert_eq!(medic.action_name(), "MedicHeal");
    assert_close(medic.work_rate(), 50.0);
    assert_eq!(medic.min_idle_duration_ms(), 3_000);
    assert!(!medic.allow_reinforce());
    assert!(!medic.heal_target());
    assert!(!medic.starts_disabled());

    let monitor = gameplay
        .heal(MONITOR)
        .expect("scenario-layered Monitor Heal action");
    assert_eq!(monitor.action_name(), "MedicHeal");
    assert_close(monitor.work_rate(), 200.0);
    assert_eq!(monitor.min_idle_duration_ms(), 3_000);
    assert!(!monitor.allow_reinforce());
    assert!(monitor.heal_target());
    assert!(!monitor.starts_disabled());
}

fn spawn_medic(world: &mut World, database: &Database) -> sim::EntityId {
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut position = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    position.y = world.terrain_height(position, true).unwrap_or_default();
    spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, MEDIC_SQUAD).expect("installed Marine squad with Medic"),
        position,
        Vec3::Z,
    )
    .expect("Medic should spawn from the layered scenario database")
}

fn tick(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
