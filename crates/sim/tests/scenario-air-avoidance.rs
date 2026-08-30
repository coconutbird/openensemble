use glam::Vec3;
use sim::{
    AircraftCrashPhase, FlightControllerKind, GameplayCatalog, TeamRelation, World,
    load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const BANSHEE: &str = "cov_air_banshee_01";
const FLOOD_SWARM: &str = "fld_air_swarm_01";
const MARINES: &str = "unsc_inf_marine_01";
const BANSHEE_UPGRADES: [&str; 3] = [
    "cov_banshee_upgrade1",
    "cov_banshee_upgrade2",
    "cov_banshee_upgrade3",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-air-avoidance -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_banshee_uses_layered_air_avoidance_and_kamikaze_data() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profiles(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.set_fog_of_war_enabled(false);

    let ground = scenario_center(world);
    let banshee_squad = spawn(
        world,
        database,
        1,
        BANSHEE,
        ground + Vec3::Y * 20.0,
        Vec3::X,
    );
    let marine_squad = spawn(
        world,
        database,
        2,
        MARINES,
        ground + Vec3::X * 30.0,
        Vec3::NEG_X,
    );
    let banshee_id = world.get_squad(banshee_squad).unwrap().unit_ids[0];
    let marine_ids = world.get_squad(marine_squad).unwrap().unit_ids.clone();
    assert_close(world.get_unit(banshee_id).unwrap().reverse_speed(), 0.0);
    assert_eq!(
        world.get_unit(banshee_id).unwrap().flight_controller_kind(),
        FlightControllerKind::PhysicsHover
    );
    assert_close(
        world.get_squad(banshee_squad).unwrap().leash_deadzone(),
        15.0,
    );
    assert_eq!(
        world
            .get_squad(banshee_squad)
            .unwrap()
            .leash_recall_delay_ms(),
        2_500
    );
    for technology in BANSHEE_UPGRADES {
        assert_eq!(world.activate_technology(1, database, technology), Ok(true));
    }

    let cursor = world.latest_impact_effect_sequence();
    let lethal_damage = world
        .get_unit(banshee_id)
        .map(|unit| unit.hitpoints + unit.shields.current + 1.0)
        .unwrap();
    assert!(world.damage_unit_with_gameplay(banshee_id, lethal_damage, gameplay));
    assert_eq!(
        world.get_unit(banshee_id).unwrap().aircraft_crash_phase(),
        AircraftCrashPhase::PendingTarget
    );

    tick(world, database, gameplay);
    let banshee = world.get_unit(banshee_id).expect("Banshee starts its dive");
    assert_eq!(banshee.aircraft_crash_phase(), AircraftCrashPhase::Crashing);
    assert!(
        banshee
            .kamikaze_target()
            .is_some_and(|id| marine_ids.contains(&id))
    );
    assert!(!banshee.is_attackable());

    for _ in 0..80 {
        if world.get_unit(banshee_id).is_none() {
            break;
        }
        tick(world, database, gameplay);
    }

    assert!(world.get_unit(banshee_id).is_none());
    let impact = world
        .impact_effect_requests_after(cursor)
        .find(|request| request.projectile_id() == banshee_id)
        .expect("scenario-loaded Banshee crash impact");
    assert_eq!(impact.effect().name, "Tankshell");
    let target_id = impact
        .primary_target_id()
        .expect("Banshee collided with a concrete Marine");
    assert!(marine_ids.contains(&target_id));
    assert!(world.get_unit(target_id).is_none());

    assert_installed_hover(world, database, gameplay, ground);
    assert_installed_move_air(world, database, gameplay, ground);
}

fn assert_installed_hover(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    ground: Vec3,
) {
    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Ally));
    let hover_squad = spawn(
        world,
        database,
        1,
        BANSHEE,
        ground + Vec3::Y * 20.0,
        Vec3::X,
    );
    for _ in 0..100 {
        tick(world, database, gameplay);
    }
    let hover_position = world.get_squad(hover_squad).unwrap().base.position;
    let hover_goal = terrain_hover_goal(world, hover_position, Vec3::X, 0.0);
    assert!(
        (hover_position.y - hover_goal).abs() < 0.5,
        "installed Banshee settled at {} instead of terrain-relative {hover_goal}",
        hover_position.y
    );
}

fn assert_installed_move_air(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    ground: Vec3,
) {
    let start = ground + Vec3::new(-80.0, 12.0, 0.0);
    let swarm_squad = spawn(world, database, 1, FLOOD_SWARM, start, Vec3::X);
    let unit_ids = world.get_squad(swarm_squad).unwrap().unit_ids.clone();
    assert_eq!(unit_ids.len(), 4);
    for unit_id in &unit_ids {
        assert_eq!(
            world.get_unit(*unit_id).unwrap().flight_controller_kind(),
            FlightControllerKind::MoveAir
        );
    }
    let initial_positions = unit_ids
        .iter()
        .map(|unit_id| world.get_unit(*unit_id).unwrap().base.position)
        .collect::<Vec<_>>();
    world
        .get_squad_mut(swarm_squad)
        .unwrap()
        .move_to(start + Vec3::X * 60.0);
    for _ in 0..60 {
        tick(world, database, gameplay);
    }

    let squad = world.get_squad(swarm_squad).unwrap();
    assert!(squad.base.position.x > start.x + 10.0);
    let mut minimum_y = f32::INFINITY;
    let mut maximum_y = f32::NEG_INFINITY;
    let mut independently_positioned = false;
    for (unit_id, initial) in unit_ids.iter().zip(initial_positions) {
        let unit = world.get_unit(*unit_id).unwrap();
        assert!(unit.base.position.distance(initial) > 1.0);
        assert!(unit.base.position.is_finite());
        assert!(unit.base.velocity.length() > 0.0);
        minimum_y = minimum_y.min(unit.base.position.y);
        maximum_y = maximum_y.max(unit.base.position.y);
        independently_positioned |=
            (unit.base.position.y - (squad.base.position.y + unit.formation_offset.y)).abs() > 0.1;
    }
    assert!(maximum_y - minimum_y > 0.01);
    assert!(independently_positioned);
    let squad_position = squad.base.position;

    assert!(world.set_mutual_team_relation(1, 2, TeamRelation::Enemy));
    let mut target_position = squad_position + Vec3::X * 20.0;
    target_position.y = world
        .terrain_height(target_position, true)
        .unwrap_or(target_position.y);
    let target_squad = spawn(world, database, 2, MARINES, target_position, Vec3::NEG_X);
    assert!(world.issue_attack_order(1, swarm_squad, target_squad, 0.0));
    tick(world, database, gameplay);
    assert!(unit_ids.iter().all(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(sim::Unit::is_move_air_attack_blocked)
    }));
    tick(world, database, gameplay);
    assert!(unit_ids.iter().all(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(sim::Unit::is_move_air_attack_blocked)
    }));
    tick(world, database, gameplay);
    assert!(unit_ids.iter().all(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(|unit| !unit.is_move_air_attack_blocked())
    }));
}

fn assert_shipped_profiles(gameplay: &GameplayCatalog) {
    assert_eq!(
        gameplay.flight_controller_kind(BANSHEE, false),
        FlightControllerKind::PhysicsHover
    );
    assert_eq!(
        gameplay.flight_controller_kind(FLOOD_SWARM, false),
        FlightControllerKind::MoveAir
    );
    assert!(
        gameplay
            .object(FLOOD_SWARM)
            .is_some_and(|object| object.ranged_actions().next().is_some())
    );
    let profiles = gameplay.air_avoidance_actions(BANSHEE);
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].action_name(), "AvoidCollisionAir");
    assert!(!profiles[0].starts_disabled());
    assert_close(profiles[0].hover_altitude_offset(), 0.0);
    assert_close(profiles[0].max_target_depression_angle(), 60.0);
    assert!(profiles[0].kamikaze_weapon().is_none());

    assert_eq!(profiles[1].action_name(), "KamikazeOnDeath");
    assert!(profiles[1].starts_disabled());
    assert_close(profiles[1].max_target_depression_angle(), 60.0);
    let weapon = profiles[1]
        .kamikaze_weapon()
        .expect("scenario-layered KamikazeDive weapon");
    assert_close(weapon.damage(), 1_500.0);
    assert_close(weapon.max_range(), 65.0);
    assert_eq!(weapon.weapon_type(), Some("Basic"));
    let area = weapon.area_damage().expect("shipped Banshee crash AOE");
    assert_close(area.radius, 6.0);
    assert_close(area.primary_target_factor, 0.5);
    assert_close(area.distance_factor, 0.2);
    assert_close(area.damage_factor, 0.2);
    assert_eq!(
        weapon.impact_effect().map(|effect| effect.name.as_str()),
        Some("Tankshell")
    );
}

fn spawn(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    player_id: u8,
    prototype: &str,
    position: Vec3,
    forward: Vec3,
) -> sim::EntityId {
    spawn_squad_at(
        world,
        database,
        player_id,
        squad_prototype_id(database, prototype).expect("shipped squad prototype"),
        position,
        forward,
    )
    .expect("shipped squad should spawn")
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn scenario_center(world: &World) -> Vec3 {
    let bounds = world.terrain_bounds().expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world.terrain_height(center, true).unwrap_or_default();
    center
}

fn terrain_hover_goal(world: &World, position: Vec3, forward: Vec3, hover_offset: f32) -> f32 {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right = Vec3::Y.cross(forward);
    [
        position + forward * 4.0,
        position - right * 4.0 - forward * 4.0,
        position + right * 4.0 - forward * 4.0,
    ]
    .into_iter()
    .filter_map(|sample| world.terrain_height(sample, true))
    .map(|terrain| (terrain + 16.0 + hover_offset).max(terrain + 13.0))
    .reduce(f32::max)
    .expect("installed scenario terrain under Banshee")
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "{actual} != {expected}"
    );
}
