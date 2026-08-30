use glam::Vec3;
use sim::{EntityId, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const HUNTER: &str = "cov_inf_hunter_01";
const SPIRIT_BOND: &str = "SpiritBond";
const BOND_BEAM: &str = "fx_proj_hunterSpiritBondBeam_01";

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn hunter_spirit_bond_uses_scenario_layered_data_and_runtime_state() {
    let mut loaded = load_installed_scenario();
    assert_shipped_profile(&loaded.simulation.gameplay);

    let prototype = squad_prototype_id(&loaded.content.database, HUNTER)
        .expect("Hunter squad prototype from the layered scenario database");
    let squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        prototype,
        Vec3::new(128.0, 0.0, 128.0),
        Vec3::Z,
    )
    .expect("spawn Hunter pair");
    let members = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .expect("Hunter squad")
        .unit_ids
        .clone();
    assert_eq!(members.len(), 2);
    loaded
        .simulation
        .world
        .get_unit_mut(members[0])
        .expect("Hunter leader")
        .actions
        .set_enabled(SPIRIT_BOND, true);

    advance(&mut loaded);
    let first_beam = assert_active_bond(&loaded, squad_id, &members);
    assert!(loaded.simulation.world.remove_object(first_beam).is_some());
    advance(&mut loaded);
    let replacement_beam = assert_active_bond(&loaded, squad_id, &members);
    assert_ne!(replacement_beam, first_beam);

    assert!(loaded.simulation.world.kill_unit(members[1], false));
    advance(&mut loaded);
    let squad = loaded.simulation.world.get_squad(squad_id).unwrap();
    assert!(!squad.spirit_bond_active());
    assert_eq!(squad.spirit_bond_beam(), None);
    assert!(
        loaded
            .simulation
            .world
            .get_object(replacement_beam)
            .is_none()
    );
    assert!(nearly_equal(
        loaded
            .simulation
            .world
            .get_unit(members[0])
            .unwrap()
            .spirit_bond_damage_multiplier(),
        1.0,
    ));
}

fn assert_shipped_profile(gameplay: &sim::GameplayCatalog) {
    let profile = gameplay
        .spirit_bond(HUNTER)
        .expect("scenario-layered Hunter SpiritBond profile");
    assert_eq!(profile.action_name(), SPIRIT_BOND);
    assert!(nearly_equal(profile.damage_modifier(), 1.35));
    assert_eq!(profile.beam_proto_object(), Some(BOND_BEAM));
    assert!(profile.starts_disabled());
}

fn assert_active_bond(
    loaded: &sim::LoadedGameScenario,
    squad_id: EntityId,
    members: &[EntityId],
) -> EntityId {
    let squad = loaded.simulation.world.get_squad(squad_id).unwrap();
    assert!(squad.spirit_bond_active());
    for member in members {
        assert!(nearly_equal(
            loaded
                .simulation
                .world
                .get_unit(*member)
                .unwrap()
                .spirit_bond_damage_multiplier(),
            1.35,
        ));
    }
    let beam_id = squad.spirit_bond_beam().expect("SpiritBond beam object");
    let beam = loaded.simulation.world.get_object(beam_id).unwrap();
    assert_eq!(beam.proto_object_name, BOND_BEAM);
    let second_endpoint = beam
        .visual_secondary_position()
        .expect("authoritative second beam endpoint");
    assert!(beam.base.position.is_finite());
    assert!(second_endpoint.is_finite());
    beam_id
}

fn advance(loaded: &mut sim::LoadedGameScenario) {
    loaded.simulation.world.game_time_ms = loaded.simulation.world.game_time_ms.wrapping_add(50);
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.000_1
}
