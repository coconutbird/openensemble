use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::ChildObjectType;
use sim::{
    EntityId, ResearchQueueResult, TechStatus, World, load_scenario_from_game_dir,
    object_prototype_id, spawn_object_at, technology_prototype_id,
};

const COMMAND_CENTER_ONE: &str = "unsc_bldg_command_01";
const COMMAND_CENTER_TWO: &str = "unsc_bldg_command_02";
const FOUNDATION_ONE: &str = "unsc_bldg_command_foundation_01";
const FOUNDATION_TWO: &str = "unsc_bldg_command_foundation_02";
const BASE_UPGRADE_ONE: &str = "unsc_base_upgrade1";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-unique-research -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_base_upgrade_transforms_only_its_blood_gulch_building_instance() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    loaded
        .simulation
        .world
        .get_player_mut(player_id)
        .unwrap()
        .resources
        .amounts = [100_000.0; 4];
    let command_id = object_prototype_id(&loaded.content.database, COMMAND_CENTER_ONE)
        .expect("layered database should expose the shipped command center");
    let technology_id = technology_prototype_id(&loaded.content.database, BASE_UPGRADE_ONE)
        .expect("layered database should expose the shipped base upgrade");
    let upgraded = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        command_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("shipped command center should be spawnable");
    let untouched = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        command_id,
        Vec3::new(50.0, 0.0, 0.0),
        Vec3::Z,
    )
    .expect("second shipped command center should be spawnable");
    let (old_foundation, old_socket_count) =
        initial_command_center_children(&loaded.simulation.world, upgraded);

    assert_eq!(
        loaded
            .simulation
            .world
            .technology_status(player_id, &loaded.content.database, technology_id)
            .unwrap(),
        TechStatus::Obtainable
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .queue_research(player_id, upgraded, &loaded.content.database, technology_id,)
            .unwrap(),
        ResearchQueueResult::Queued
    );
    let promoted = loaded
        .simulation
        .world
        .update_production(0.01, &loaded.content.database);
    assert_eq!(promoted.completed_research, 0);
    let completed = loaded
        .simulation
        .world
        .update_production(15.0, &loaded.content.database);
    assert_eq!(completed.completed_research, 1);

    let building = loaded.simulation.world.get_building(upgraded).unwrap();
    assert_eq!(building.base.id, upgraded);
    assert_eq!(building.proto_object_name, COMMAND_CENTER_TWO);
    assert!(building.unique_technology_is_active(technology_id));
    assert_upgraded_command_center_children(
        &loaded.simulation.world,
        &loaded.content.database,
        upgraded,
        old_foundation,
        old_socket_count,
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_building(untouched)
            .unwrap()
            .proto_object_name,
        COMMAND_CENTER_ONE
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .building_technology_status(
                player_id,
                upgraded,
                &loaded.content.database,
                technology_id,
            )
            .unwrap(),
        TechStatus::Active
    );
}

fn initial_command_center_children(world: &World, building_id: EntityId) -> (EntityId, usize) {
    let building = world.get_building(building_id).unwrap();
    let foundation_id = building.associated_foundations()[0];
    assert_eq!(building.associated_sockets().len(), 3);
    assert!(
        world
            .get_building(foundation_id)
            .unwrap()
            .proto_object_name
            .eq_ignore_ascii_case(FOUNDATION_ONE)
    );
    (foundation_id, building.associated_sockets().len())
}

fn assert_upgraded_command_center_children(
    world: &World,
    database: &Database,
    building_id: EntityId,
    old_foundation: EntityId,
    old_socket_count: usize,
) {
    let building = world.get_building(building_id).unwrap();
    let expected_socket_count = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(COMMAND_CENTER_TWO))
        .and_then(|prototype| prototype.child_objects.as_ref())
        .map_or(0, |children| {
            children
                .objects
                .iter()
                .filter(|child| child.child_type == Some(ChildObjectType::Socket))
                .count()
        });
    assert_eq!(expected_socket_count, 9);
    assert_eq!(
        building.associated_sockets().len(),
        old_socket_count + expected_socket_count
    );
    assert_eq!(building.associated_foundations().len(), 1);
    let new_foundation = building.associated_foundations()[0];
    assert_ne!(new_foundation, old_foundation);
    assert!(world.get_unit(old_foundation).is_none());
    assert!(
        world
            .get_building(new_foundation)
            .unwrap()
            .proto_object_name
            .eq_ignore_ascii_case(FOUNDATION_TWO)
    );
}
