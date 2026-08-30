use glam::Vec3;
use sim::{
    GAIA_PLAYER, load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    squad_prototype_id,
};

const MARINES: &str = "unsc_inf_marine_01";
const SUPPLY_CRATE: &str = "rsrc_supplies_crate_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-gather -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_catalog_and_resource_node_drive_marine_gathering() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let supplies = loaded
        .simulation
        .gameplay
        .gather_action(MARINES, "Supplies")
        .expect("shipped Marine GatherSupplies action");
    let collectable = loaded
        .simulation
        .gameplay
        .gather_action(MARINES, "Collectable")
        .expect("shipped Marine GatherCollectables action");
    assert_eq!(supplies.action_name(), "GatherSupplies");
    assert_eq!(supplies.resource_id(), 0);
    assert_eq!(collectable.action_name(), "GatherCollectables");
    assert_eq!(collectable.resource_id(), 3);
    let supplies_id = supplies.resource_id();

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
    let crate_id = spawn_object_at(
        world,
        database,
        GAIA_PLAYER,
        object_prototype_id(database, SUPPLY_CRATE).expect("installed supply crate"),
        leader_position,
        Vec3::Z,
    )
    .expect("supply crate should spawn as a unit resource node");
    let resources_before = world.get_player(1).unwrap().get_resource(supplies_id);
    let amount_before = world
        .get_unit(crate_id)
        .expect("spawned resource node")
        .resource_amount();
    assert_eq!(
        world.get_unit(crate_id).unwrap().resource_name(),
        Some("Supplies")
    );
    assert!((amount_before - 30.0).abs() <= f32::EPSILON);

    assert!(world.issue_gather_order(1, squad_id, crate_id, gameplay));
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);

    assert!(world.get_player(1).unwrap().get_resource(supplies_id) > resources_before);
    assert!(world.get_unit(crate_id).unwrap().resource_amount() < amount_before);
    assert!(world.is_unit_being_gathered_from(crate_id));
}
