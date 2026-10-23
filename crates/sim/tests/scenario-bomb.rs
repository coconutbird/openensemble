//! Checks that the shipped Flood bomb loads scenario physics and runs its persistent actions.

use glam::Vec3;
use num_traits::ToPrimitive;
use sim::{
    BombPhase, GameplayCatalog, UnitDetonatePhase, World, object_prototype_id, spawn_object_at,
};

const FLOOD_BOMB: &str = "fx_proj_fldbomb_01";
const FLOOD_SPAWN: &str = "fld_inf_InfectionForm_03";
const TICK_SECONDS: f32 = 0.05;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-bomb -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_flood_bomb_loads_scenario_physics_and_executes_persistent_actions() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
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
    let body = world.unit_body(source).expect("the Flood bomb body");
    assert_eq!(
        world.physics().world().motion_type(body),
        Some(physics::MotionType::BoxInertia)
    );
    assert!(bomb.base.position.y < position.y);
    // Retail's `BUnitActionBomb::connect` draws `mRoll` from the synchronized
    // sim random stream, which skirmish setup and the skirmish scripts also
    // consume, so either branch is valid here; each must finish its own way.
    let rolls = bomb.bomb_rolls();
    let countdown = gameplay
        .first_detonate_action(FLOOD_BOMB)
        .and_then(|action| action.duration())
        .expect("shipped Flood bomb Detonate countdown");
    // The longest sampled countdown, plus one second for the action start and
    // the detonation's death cleanup.
    let max_updates = ((countdown.seconds() + countdown.spread() + 1.0) / TICK_SECONDS)
        .ceil()
        .to_usize()
        .expect("finite Detonate countdown");

    let mut completed = false;
    for _ in 0..max_updates {
        let Some(bomb) = world.get_unit(source) else {
            break;
        };
        completed |= bomb.bomb_phase() == BombPhase::Complete;
        tick(world, database, gameplay);
    }

    assert!(world.get_unit(source).is_none());
    assert_eq!(
        completed, rolls,
        "a rolling Bomb completes on contact and waits for the countdown; \
         a non-rolling Bomb detonates on impact"
    );
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
    assert_eq!(body.name(), "egg");
    let main = body.main_body();
    assert_eq!(main.mass().to_bits(), 20.0_f32.to_bits());
    assert_eq!(main.friction().to_bits(), 2.0_f32.to_bits());
    assert_eq!(main.restitution().to_bits(), 1.0_f32.to_bits());
    assert!(body.half_extents().abs_diff_eq(Vec3::splat(0.5), 0.000_1));
    assert!(body.center_offset().abs_diff_eq(Vec3::ZERO, 0.000_1));
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.update_entities_with_database_and_gameplay(TICK_SECONDS, database, gameplay);
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
