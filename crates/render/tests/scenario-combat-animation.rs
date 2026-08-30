use glam::Vec3;
use render::ugx::{UnitScene, simulation_proto_names};
use sim::{
    CommandEntry, CommandExecutor, QueuedCommand, WorkCommand, load_scenario_from_game_dir,
    spawn_squad_at, squad_prototype_id,
};

const BRUTE_CHIEF: &str = "cov_inf_bruteChief_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-combat-animation -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_the_exact_sim_selected_combat_clip() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed scenario should load");
    let database = &loaded.content.database;
    loaded.simulation.world.get_player_mut(1).unwrap().team_id = 1;
    loaded.simulation.world.get_player_mut(2).unwrap().team_id = 2;
    loaded.simulation.world.configure_standard_team_relations();

    let chief_proto = squad_prototype_id(database, BRUTE_CHIEF).expect("Brute Chief squad");
    let attacker_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        chief_proto,
        Vec3::ZERO,
        Vec3::X,
    )
    .expect("attacker squad");
    let target_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        2,
        chief_proto,
        Vec3::X * 2.0,
        Vec3::NEG_X,
    )
    .expect("target squad");
    let attacker_id = loaded
        .simulation
        .world
        .get_squad(attacker_squad)
        .unwrap()
        .unit_ids[0];

    load_active_visuals(&mut loaded);
    let mut scene = UnitScene::load_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    );
    issue_attack(&mut loaded, attacker_squad, target_squad);

    let (animation_type, animation_asset) = wait_for_active_clip(&mut loaded, attacker_id);
    assert!(
        !scene.roster_matches_with_gameplay(&loaded.simulation.world, &loaded.simulation.gameplay)
    );
    assert!(scene.sync_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    ));
    let placement = find_placement(&scene, attacker_id);
    assert_eq!(placement.animation_type(), Some(animation_type.as_str()));
    assert_eq!(placement.animation_asset(), Some(animation_asset.as_str()));
    assert!(placement.animation_uses_simulation_clock());
    assert!(placement.unit().has_scripted_animation());
}

fn load_active_visuals(loaded: &mut sim::LoadedGameScenario) {
    let names = simulation_proto_names(&loaded.simulation.world).collect::<Vec<_>>();
    loaded
        .content
        .load_visuals_for(&mut loaded.source, names.iter().copied());
}

fn issue_attack(
    loaded: &mut sim::LoadedGameScenario,
    attacker_squad: sim::EntityId,
    target_squad: sim::EntityId,
) {
    CommandExecutor::with_database(&loaded.content.database).execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Work(WorkCommand::attack_squads(
                1,
                vec![attacker_squad],
                target_squad,
            )),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn wait_for_active_clip(
    loaded: &mut sim::LoadedGameScenario,
    attacker_id: sim::EntityId,
) -> (String, String) {
    for _ in 0..200 {
        loaded
            .simulation
            .world
            .update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);
        let unit = loaded.simulation.world.get_unit(attacker_id).unwrap();
        let action = unit.combat.action_name().unwrap_or_default();
        let Some(profile) = loaded
            .simulation
            .gameplay
            .object(&unit.proto_object_name)
            .and_then(|object| object.attack_profile(action))
        else {
            continue;
        };
        let Some(index) = unit.combat.animation_index() else {
            continue;
        };
        let animation = &profile.animations[index];
        if unit.combat.animation_position(animation.duration).is_some() {
            return (profile.animation_type.clone(), animation.asset_path.clone());
        }
    }
    panic!("Brute Chief never entered its combat animation interval");
}

fn find_placement(scene: &UnitScene, entity_id: sim::EntityId) -> &render::ugx::UnitPlacement {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
        .expect("attacker placement")
}
