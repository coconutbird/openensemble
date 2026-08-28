use render::ugx::UnitScene;
use sim::entities::squads::marine::MARINE_SQUAD_NAME;
use sim::{
    LoadedGameScenario, Simulation, load_scenario_from_game_dir, spawn_squad_at,
    spawn_squad_from_base_by_name, squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-roster -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn spawned_marines_and_projectiles_join_the_authoritative_render_roster() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());
    let LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = load_scenario_from_game_dir(&game_dir, &scenario).expect("real scenario should load");

    load_active_visuals(&simulation.world, &mut content, &mut source);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    let (&_player_id, &base_id) = simulation
        .initial_base_ids
        .first_key_value()
        .expect("skirmish scenario should assign a player base");
    let squad_id = spawn_squad_from_base_by_name(
        &mut simulation.world,
        &content.database,
        base_id,
        MARINE_SQUAD_NAME,
    )
    .expect("real database should spawn Marines");
    let member_ids = simulation
        .world
        .get_squad(squad_id)
        .expect("spawned Marine squad")
        .unit_ids
        .clone();

    load_active_visuals(&simulation.world, &mut content, &mut source);
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    for member_id in member_ids {
        assert!(
            scene
                .placements()
                .iter()
                .any(|placement| placement.entity_id() == member_id),
            "spawned Marine {member_id:?} should have a presentation placement"
        );
    }

    let projectile_ids =
        launch_real_marine_projectiles(&mut simulation, &content.database, squad_id);
    load_active_visuals(&simulation.world, &mut content, &mut source);
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(scene.roster_matches(&simulation.world));
    assert_eq!(
        scene.simulation_entity_count(),
        simulation.world.units.len() + simulation.world.projectiles.len()
    );
    for projectile_id in projectile_ids {
        let projectile = simulation
            .world
            .get_projectile(projectile_id)
            .expect("live projectile");
        let prototype = content
            .database
            .objects
            .iter()
            .find(|prototype| {
                prototype
                    .name
                    .eq_ignore_ascii_case(&projectile.proto_object_name)
            })
            .expect("projectile prototype");
        let has_placement = scene
            .placements()
            .iter()
            .any(|placement| placement.entity_id() == projectile_id);
        let has_asset_diagnostic = scene.issues().iter().any(|issue| {
            issue
                .proto_name()
                .eq_ignore_ascii_case(&projectile.proto_object_name)
        });
        assert!(
            has_placement || has_asset_diagnostic,
            "Marine projectile {projectile_id:?} ({}) must produce a visual placement or an explicit asset diagnostic; visual={:?}, flags={:?}",
            projectile.proto_object_name,
            prototype.visual,
            prototype.flags,
        );
    }
}

fn launch_real_marine_projectiles(
    simulation: &mut sim::LoadedScenario,
    database: &pipeline::database::hw1::Database,
    attacker_squad_id: sim::EntityId,
) -> Vec<sim::EntityId> {
    let (attacker_player_id, position, forward) = simulation
        .world
        .get_squad(attacker_squad_id)
        .map(|squad| {
            (
                squad.base.player_id,
                squad.base.position,
                squad.base.forward,
            )
        })
        .expect("attacking Marine squad");
    let enemy_player_id = simulation
        .world
        .active_players()
        .map(|player| player.id)
        .find(|&player_id| {
            simulation
                .world
                .players_are_enemies(attacker_player_id, player_id)
        })
        .expect("real scenario should contain an enemy player");
    let marine_proto_id =
        squad_prototype_id(database, MARINE_SQUAD_NAME).expect("Marine squad prototype");
    let target_squad_id = spawn_squad_at(
        &mut simulation.world,
        database,
        enemy_player_id,
        marine_proto_id,
        position + forward * 10.0,
        -forward,
    )
    .expect("enemy Marine squad");
    assert!(simulation.world.issue_attack_order(
        attacker_player_id,
        attacker_squad_id,
        target_squad_id,
        0.0,
    ));

    let mut clock = Simulation::new();
    clock.start();
    for _ in 0..600 {
        clock.tick_with_scenario(simulation, database);
        let projectile_ids = simulation.world.projectiles.ids().collect::<Vec<_>>();
        if !projectile_ids.is_empty() {
            return projectile_ids;
        }
    }
    panic!("real Marine Attack tags did not create a projectile");
}

fn load_active_visuals(
    world: &sim::World,
    content: &mut pipeline::hw1::World,
    source: &mut pipeline::source::AssetSource<pipeline::source::StdFileProvider>,
) {
    let active_proto_names = world
        .units
        .iter()
        .map(|(_, unit)| unit.proto_object_name.as_str())
        .chain(
            world
                .projectiles
                .iter()
                .map(|(_, projectile)| projectile.proto_object_name.as_str()),
        )
        .collect::<Vec<_>>();
    content.load_visuals_for(source, active_proto_names.iter().copied());
}
