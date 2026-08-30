use glam::Vec3;
use sim::{
    CapturePhase, GAIA_PLAYER, load_scenario_from_game_dir, object_prototype_id, spawn_object_at,
    spawn_squad_at, squad_prototype_id,
};

const MARINES: &str = "unsc_inf_marine_01";
const CAPTURE_NODE: &str = "for_bldg_factory_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-capture -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_and_tactics_drive_marine_capture() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let capture = loaded
        .simulation
        .gameplay
        .capture_action(MARINES, "Capture")
        .expect("shipped Marine Capture action");
    assert_eq!(capture.action_name(), "Capture");
    // Blood Gulch overrides the root tactic's omitted rate in its mounted layer.
    assert_close(capture.work_rate(), 0.167);
    assert_close(capture.work_range(), 4.0);

    let database = &loaded.content.database;
    assert_close(
        database
            .game_data
            .as_ref()
            .and_then(|data| data.capture_decay_rate)
            .expect("scenario-layered GameData CaptureDecayRate"),
        0.5,
    );
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
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, MARINES).expect("installed Marine squad"),
        origin,
        Vec3::Z,
    )
    .expect("Marine squad should spawn");
    let leader_position = world
        .get_squad(squad_id)
        .and_then(|squad| squad.unit_ids.first())
        .and_then(|unit_id| world.get_unit(*unit_id))
        .map(|unit| unit.base.position)
        .expect("spawned Marine leader");
    let target_id = spawn_object_at(
        world,
        database,
        GAIA_PLAYER,
        object_prototype_id(database, CAPTURE_NODE).expect("installed capturable factory"),
        leader_position,
        Vec3::Z,
    )
    .expect("capturable factory should spawn");
    let target = world.get_unit(target_id).expect("spawned capture target");
    assert!(target.is_capturable());
    assert_close(target.maximum_capture_points(), 20.0);

    assert!(world.issue_capture_order(1, squad_id, target_id, database, gameplay));
    for _ in 0..800 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        if world.get_squad(squad_id).unwrap().capture_phase() == CapturePhase::Done {
            break;
        }
    }

    let target = world.get_unit(target_id).expect("captured factory");
    assert_eq!(target.base.player_id, 1);
    assert_close(target.capture_points(), 0.0);
    assert_close(target.hitpoints, target.max_hitpoints);
    assert_eq!(
        world.get_squad(squad_id).unwrap().capture_phase(),
        CapturePhase::Done
    );
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= f32::EPSILON,
        "expected {left} to equal {right}"
    );
}
