use glam::Vec3;
use render::ugx::{UnitScene, simulation_proto_names};
use sim::{
    AircraftCrashPhase, World, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const BANSHEE: &str = "cov_air_banshee_01";
const MARINES: &str = "unsc_inf_marine_01";
const BANSHEE_UPGRADES: [&str; 3] = [
    "cov_banshee_upgrade1",
    "cov_banshee_upgrade2",
    "cov_banshee_upgrade3",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-air-avoidance -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_kamikaze_clip_and_dive_pitch_from_sim_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed scenario and layered database should load");
    configure_enemies(&mut loaded.simulation.world);
    let ground = scenario_center(&loaded.simulation.world);
    let banshee_squad = spawn(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        BANSHEE,
        ground + Vec3::Y * 20.0,
        Vec3::X,
    );
    let marine_squad = spawn(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        ground + Vec3::X * 30.0,
        Vec3::NEG_X,
    );
    let banshee_id = loaded
        .simulation
        .world
        .get_squad(banshee_squad)
        .unwrap()
        .unit_ids[0];
    let marine_ids = loaded
        .simulation
        .world
        .get_squad(marine_squad)
        .unwrap()
        .unit_ids
        .clone();
    activate_banshee_upgrades(&mut loaded);

    load_active_visuals(&mut loaded);
    let mut scene = UnitScene::load_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    );
    let lethal_damage = loaded
        .simulation
        .world
        .get_unit(banshee_id)
        .map(|unit| unit.hitpoints + unit.shields.current + 1.0)
        .unwrap();
    assert!(loaded.simulation.world.damage_unit_with_gameplay(
        banshee_id,
        lethal_damage,
        &loaded.simulation.gameplay,
    ));
    tick(&mut loaded);

    let banshee = loaded
        .simulation
        .world
        .get_unit(banshee_id)
        .expect("Banshee remains present during its dive");
    assert_eq!(banshee.aircraft_crash_phase(), AircraftCrashPhase::Crashing);
    assert!(
        banshee
            .kamikaze_target()
            .is_some_and(|id| marine_ids.contains(&id))
    );
    assert!(
        !scene.roster_matches_with_gameplay(&loaded.simulation.world, &loaded.simulation.gameplay,)
    );
    assert!(scene.sync_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    ));

    let placement = scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == banshee_id)
        .expect("renderer Banshee placement");
    assert_eq!(placement.animation_type(), Some("Kamikaze"));
    assert!(
        placement
            .unit()
            .attachments_for_animation(Some("Kamikaze"))
            .any(|attachment| attachment
                .name
                .eq_ignore_ascii_case("effects\\vehicle_fx\\cov\\banshee\\kamakazifire_01"))
    );
    assert!(placement.animation_uses_simulation_clock());
    assert!(placement.unit().has_scripted_animation());
    let rendered_forward = placement.transform().transform_vector3(Vec3::Z);
    assert!(rendered_forward.abs_diff_eq(banshee.base.forward.normalize(), 0.000_1));
    assert!(rendered_forward.y < 0.0);
}

fn activate_banshee_upgrades(loaded: &mut sim::LoadedGameScenario) {
    for technology in BANSHEE_UPGRADES {
        assert_eq!(
            loaded
                .simulation
                .world
                .activate_technology(1, &loaded.content.database, technology),
            Ok(true)
        );
    }
}

fn configure_enemies(world: &mut World) {
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.set_fog_of_war_enabled(false);
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

fn load_active_visuals(loaded: &mut sim::LoadedGameScenario) {
    let names = simulation_proto_names(&loaded.simulation.world).collect::<Vec<_>>();
    loaded
        .content
        .load_visuals_for(&mut loaded.source, names.iter().copied());
}

fn tick(loaded: &mut sim::LoadedGameScenario) {
    loaded.simulation.world.advance_time(50);
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
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
