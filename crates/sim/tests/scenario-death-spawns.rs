use glam::Vec3;
use sim::{load_scenario_from_game_dir, object_prototype_id, spawn_object_at};

const FLOOD_BOMB: &str = "fx_proj_fldbomb_01";
const FLOOD_SPAWN: &str = "fld_inf_InfectionForm_03";
const FLOOD_MEMBER: &str = "fld_inf_infectionForm_02";
const GRUNT: &str = "cov_inf_grunt_01";
const GRUNT_CONFETTI: &str = "fx_obj_gruntconfetti";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-death-spawns -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_flood_bomb_death_spawns_its_database_squad() {
    let mut loaded = load_installed_scenario();
    let source_position = Vec3::new(128.0, 0.0, 128.0);
    let existing = matching_squad_count(&loaded.simulation.world, FLOOD_SPAWN);
    let source_id = spawn_installed_object(&mut loaded, FLOOD_BOMB, source_position);

    assert!(loaded.simulation.world.kill_unit(source_id, false));
    tick_entities(&mut loaded);

    assert!(loaded.simulation.world.get_unit(source_id).is_none());
    assert_eq!(
        matching_squad_count(&loaded.simulation.world, FLOOD_SPAWN),
        existing + 1
    );
    let spawned = loaded
        .simulation
        .world
        .squads
        .iter()
        .map(|(_, squad)| squad)
        .filter(|squad| squad.proto_squad_name.eq_ignore_ascii_case(FLOOD_SPAWN))
        .last()
        .expect("Flood bomb should create its infection-form squad");
    assert_eq!(spawned.base.player_id, 1);
    assert_eq!(spawned.base.position, source_position);
    assert_eq!(spawned.base.forward, Vec3::X);
    assert_eq!(spawned.unit_ids.len(), 3);
    assert!(spawned.unit_ids.iter().all(|unit_id| {
        loaded
            .simulation
            .world
            .get_unit(*unit_id)
            .is_some_and(|unit| unit.proto_object_name.eq_ignore_ascii_case(FLOOD_MEMBER))
    }));
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_grunt_birthday_party_technology_assigns_confetti_death_spawn() {
    let mut loaded = load_installed_scenario();
    assert!(
        loaded
            .simulation
            .world
            .activate_technology(1, &loaded.content.database, "skull_02")
            .expect("shipped skull_02 technology")
    );
    let existing = matching_squad_count(&loaded.simulation.world, GRUNT_CONFETTI);
    let source_id = spawn_installed_object(&mut loaded, GRUNT, Vec3::new(140.0, 0.0, 128.0));

    assert!(loaded.simulation.world.kill_unit(source_id, false));
    tick_entities(&mut loaded);

    assert!(loaded.simulation.world.get_unit(source_id).is_none());
    assert_eq!(
        matching_squad_count(&loaded.simulation.world, GRUNT_CONFETTI),
        existing + 1
    );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}

fn spawn_installed_object(
    loaded: &mut sim::LoadedGameScenario,
    prototype_name: &str,
    position: Vec3,
) -> sim::EntityId {
    let prototype = object_prototype_id(&loaded.content.database, prototype_name)
        .unwrap_or_else(|| panic!("shipped object {prototype_name}"));
    spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        prototype,
        position,
        Vec3::X,
    )
    .unwrap_or_else(|error| panic!("spawn shipped object {prototype_name}: {error:?}"))
}

fn tick_entities(loaded: &mut sim::LoadedGameScenario) {
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
}

fn matching_squad_count(world: &sim::World, prototype_name: &str) -> usize {
    world
        .squads
        .iter()
        .filter(|(_, squad)| squad.proto_squad_name.eq_ignore_ascii_case(prototype_name))
        .count()
}
