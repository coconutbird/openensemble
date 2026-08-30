use sim::LoadedGameScenario;

pub(super) fn assert_tower_wall_state(scenario: &str, loaded: &LoadedGameScenario) {
    if scenario != "PHXscn15" {
        return;
    }
    let world = &loaded.simulation.world;
    let walls = world
        .squads
        .iter()
        .filter(|(_, squad)| !squad.associated_wall_towers().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(walls.len(), 2, "PHXscn15 should link both wall pairs");
    for (_, source_squad) in walls {
        assert_eq!(source_squad.proto_squad_name, "hook_bldg_wall_01");
        let source_unit = world
            .get_unit(source_squad.unit_ids[0])
            .expect("PHXscn15 wall source leader");
        let action = source_unit.tower_wall.expect("PHXscn15 tower-wall action");
        let [target_squad_id] = source_squad.associated_wall_towers() else {
            panic!("PHXscn15 wall source should have one endpoint");
        };
        assert_eq!(action.target_squad_id(), *target_squad_id);
        let target_squad = world
            .get_squad(*target_squad_id)
            .expect("PHXscn15 wall target squad");
        assert_eq!(target_squad.proto_squad_name, "hook_bldg_wall_02");
        let target_unit = world
            .get_unit(target_squad.unit_ids[0])
            .expect("PHXscn15 wall target leader");
        let direction = glam::Vec3::new(
            target_unit.base.position.x - source_unit.base.position.x,
            0.0,
            target_unit.base.position.z - source_unit.base.position.z,
        )
        .normalize();
        assert_vec3_close(source_unit.base.forward, -direction, 0.000_001);
        assert_vec3_close(target_unit.base.forward, direction, 0.000_001);
        assert_eq!(action.beam_start_position(), source_unit.base.position);
        assert_eq!(action.beam_end_position(), target_unit.base.position);
    }
}

pub(super) fn assert_vec3_close(actual: glam::Vec3, expected: glam::Vec3, tolerance: f32) {
    assert!(
        (actual - expected).abs().max_element() <= tolerance,
        "expected {expected:?}, got {actual:?}"
    );
}
