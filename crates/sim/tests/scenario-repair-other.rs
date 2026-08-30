use glam::Vec3;
use sim::{RepairOtherPhase, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const ENGINEER: &str = "cov_inf_engineer_01";
const ANDERS: &str = "cpgn_npc_anders_01";
const CYCLOPS: &str = "unsc_inf_cyclops_01";
const WRAITH: &str = "cov_veh_wraith_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-repair-other -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_and_tactics_drive_engineer_repair_other() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let engineer = loaded
        .simulation
        .gameplay
        .repair_other_action(ENGINEER, "RepairOther")
        .expect("scenario-layered Engineer RepairOther action");
    assert_close(engineer.work_rate(), 0.15);
    assert_close(engineer.work_range(), 3.0);
    assert!(engineer.allow_reinforce());
    assert_eq!(engineer.effect_proto_object(), Some("fx_covhealbeam"));
    let auto = engineer
        .auto_repair()
        .expect("Engineer auto-repair profile");
    assert_eq!(auto.idle_time_ms(), 1000);
    assert_close(auto.threshold(), 0.99);
    assert_close(auto.search_distance(), 45.0);
    assert_close(
        loaded
            .simulation
            .gameplay
            .repair_other_action(ANDERS, "RepairOther")
            .expect("scenario-layered Anders RepairOther action")
            .work_rate(),
        0.8,
    );
    assert!(
        loaded
            .simulation
            .gameplay
            .repair_other_action(CYCLOPS, "RepairOther")
            .expect("scenario-layered Cyclops RepairOther action")
            .starts_disabled()
    );

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut origin = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    origin.y = world.terrain_height(origin, true).unwrap_or_default();
    let engineer_id = spawn(world, database, ENGINEER, origin);
    let wraith_id = spawn(world, database, WRAITH, origin + Vec3::X);
    let wraith_unit_id = world.get_squad(wraith_id).unwrap().unit_ids[0];
    let maximum = world.get_unit(wraith_unit_id).unwrap().max_hitpoints;
    world.get_unit_mut(wraith_unit_id).unwrap().hitpoints = maximum * 0.5;

    assert!(world.issue_repair_other_order(
        1,
        engineer_id,
        wraith_unit_id,
        None,
        database,
        gameplay,
    ));
    let before = world.get_unit(wraith_unit_id).unwrap().hitpoints;
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
    assert!(world.get_unit(wraith_unit_id).unwrap().hitpoints > before);
    let repairer = world.get_squad(engineer_id).unwrap();
    assert_eq!(repairer.repair_other_phase(), RepairOtherPhase::Working);
    let effect_id = repairer
        .repair_other_effect_id()
        .expect("sim-owned Engineer repair beam");
    assert_eq!(
        world.get_object(effect_id).unwrap().proto_object_name,
        "fx_covhealbeam"
    );

    for _ in 0..1000 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        if world.get_squad(engineer_id).unwrap().repair_other_phase() == RepairOtherPhase::Done {
            break;
        }
    }
    assert_eq!(
        world.get_squad(engineer_id).unwrap().repair_other_phase(),
        RepairOtherPhase::Done
    );
    assert_close(world.get_unit(wraith_unit_id).unwrap().hitpoints, maximum);
    assert!(world.get_object(effect_id).is_none());
}

fn spawn(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
    prototype: &str,
    position: Vec3,
) -> sim::EntityId {
    spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, prototype)
            .unwrap_or_else(|| panic!("missing installed squad {prototype}")),
        position,
        Vec3::Z,
    )
    .unwrap_or_else(|error| panic!("{prototype} should spawn: {error}"))
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
