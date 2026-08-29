use glam::Vec3;
use sim::{
    load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    squad_prototype_id,
};

const DETONATOR: &str = "cpgn_scn02_detonator_01";
const DESTROYED_DETONATOR: &str = "cpgn_scn02_detonatord_01";
const SHATTER_SOURCE: &str = "fld_inf_tentacle_01";
const SHATTER_TARGET: &str = "fld_inf_tentacle_shatterdeath_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-death-replacements -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_campaign_detonator_retains_its_id_as_the_database_replacement() {
    let mut loaded = load_installed_scenario();
    let position = Vec3::new(128.0, 0.0, 128.0);
    let source_id = spawn_installed_object(&mut loaded, DETONATOR, position);

    assert!(loaded.simulation.world.kill_unit(source_id, false));
    tick_entities(&mut loaded);

    let replacement = loaded
        .simulation
        .world
        .get_unit(source_id)
        .expect("shipped replacement should retain the source ID");
    assert!(
        replacement
            .proto_object_name
            .eq_ignore_ascii_case(DESTROYED_DETONATOR)
    );
    assert_eq!(replacement.base.position, position);
    assert_eq!(replacement.base.forward, Vec3::X);
    assert_eq!(replacement.hitpoints.to_bits(), 1.0_f32.to_bits());
    assert_eq!(replacement.max_hitpoints.to_bits(), 1.0_f32.to_bits());
    assert!(replacement.is_static_death_replacement());
    assert!(!replacement.is_death_replacement_healing());

    assert!(loaded.simulation.world.kill_unit(source_id, true));
    assert!(loaded.simulation.world.get_unit(source_id).is_none());
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_frozen_tentacle_selects_its_transient_shatter_replacement() {
    let mut loaded = load_installed_scenario();
    assert!(object_prototype_id(&loaded.content.database, SHATTER_TARGET).is_some());
    let (squad_id, source_id) = spawn_installed_squad(&mut loaded, SHATTER_SOURCE, Vec3::ZERO);

    assert!(
        loaded
            .simulation
            .world
            .add_squad_cryo(squad_id, f32::MAX, &loaded.content.database)
    );
    let squad = loaded.simulation.world.get_squad(squad_id).unwrap();
    assert!(squad.is_cryo_frozen());
    assert_eq!(squad.maximum_cryo_points().to_bits(), 100.0_f32.to_bits());
    assert!(
        loaded
            .simulation
            .world
            .get_unit(source_id)
            .unwrap()
            .is_shatter_on_death()
    );

    assert!(loaded.simulation.world.kill_unit(source_id, false));
    tick_entities(&mut loaded);
    assert!(loaded.simulation.world.get_unit(source_id).is_none());
    assert!(loaded.simulation.world.get_squad(squad_id).is_none());
}

fn spawn_installed_squad(
    loaded: &mut sim::LoadedGameScenario,
    prototype_name: &str,
    position: Vec3,
) -> (sim::EntityId, sim::EntityId) {
    let prototype = squad_prototype_id(&loaded.content.database, prototype_name)
        .unwrap_or_else(|| panic!("shipped squad {prototype_name}"));
    let squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        prototype,
        position,
        Vec3::X,
    )
    .unwrap_or_else(|error| panic!("spawn shipped squad {prototype_name}: {error:?}"));
    let unit_id = *loaded
        .simulation
        .world
        .get_squad(squad_id)
        .and_then(|squad| squad.unit_ids.first())
        .expect("shipped squad has its authored member");
    (squad_id, unit_id)
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
