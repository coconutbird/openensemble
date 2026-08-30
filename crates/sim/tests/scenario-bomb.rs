use glam::Vec3;
use sim::{
    BombPhase, GameplayCatalog, MotionType, UnitDetonatePhase, World, load_scenario_from_game_dir,
    object_prototype_id, spawn_object_at,
};

const FLOOD_BOMB: &str = "fx_proj_fldbomb_01";
const FLOOD_SPAWN: &str = "fld_inf_InfectionForm_03";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-bomb -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_flood_bomb_loads_scenario_physics_and_executes_persistent_actions() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profile(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let existing_spawns = matching_squad_count(world, FLOOD_SPAWN);
    let mut position = scenario_center(world);
    position.y += 4.0;
    let source = spawn_object_at(
        world,
        database,
        1,
        object_prototype_id(database, FLOOD_BOMB).expect("shipped Flood bomb object"),
        position,
        Vec3::X,
    )
    .expect("shipped Flood bomb should spawn");

    tick(world, database, gameplay);
    let bomb = world
        .get_unit(source)
        .expect("Flood bomb after first update");
    assert_eq!(bomb.bomb_phase(), BombPhase::Working);
    assert_eq!(bomb.detonate_phase(), UnitDetonatePhase::Working);
    assert_eq!(
        bomb.physics.as_ref().unwrap().motion_type(),
        MotionType::Dynamic
    );
    assert!(bomb.base.position.y < position.y);

    for _ in 0..80 {
        if world.get_unit(source).is_none() {
            break;
        }
        tick(world, database, gameplay);
    }

    assert!(world.get_unit(source).is_none());
    assert_eq!(
        matching_squad_count(world, FLOOD_SPAWN),
        existing_spawns + 1
    );
}

fn assert_shipped_profile(gameplay: &GameplayCatalog) {
    let profile = gameplay.bomb(FLOOD_BOMB).expect("compiled persistent Bomb");
    assert_eq!(profile.action_name(), "Bomb");
    assert_eq!(profile.roll_chance().to_bits(), 0.5_f32.to_bits());
    assert!(!profile.starts_disabled());
    assert_eq!(profile.physics_info(), Some("egg"));
    assert!(profile.physics_load_issue().is_none());
    let body = profile
        .physics_body()
        .expect("scenario-layered egg physics chain");
    assert_eq!(body.physics_info(), "egg");
    assert_eq!(body.material().mass.to_bits(), 20.0_f32.to_bits());
    assert_eq!(body.material().friction.to_bits(), 2.0_f32.to_bits());
    assert_eq!(body.material().restitution.to_bits(), 1.0_f32.to_bits());
    assert!(
        body.collider()
            .half_extents
            .abs_diff_eq(Vec3::splat(0.5), 0.000_1)
    );
    assert!(
        body.collider()
            .center_offset
            .abs_diff_eq(Vec3::ZERO, 0.000_1)
    );
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
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

fn matching_squad_count(world: &World, prototype_name: &str) -> usize {
    world
        .squads
        .iter()
        .filter(|(_, squad)| squad.proto_squad_name.eq_ignore_ascii_case(prototype_name))
        .count()
}
