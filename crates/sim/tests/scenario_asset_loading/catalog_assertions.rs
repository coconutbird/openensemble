use sim::{LoadedGameScenario, object_prototype_id};

pub(super) fn assert_loaded_gameplay_catalog(loaded: &LoadedGameScenario) {
    assert_real_object_type_catalog(loaded);
    assert!(loaded.simulation.gameplay.referenced_tactic_count() > 0);
    assert!(!loaded.simulation.gameplay.is_empty());
    let marine = loaded
        .simulation
        .gameplay
        .object("UNSC_INF_MARINE_01")
        .expect("the scenario-layered source should resolve Marine tactics");
    assert_eq!(marine.damage_type(), Some("Light"));
    assert!(marine.ranged_actions().any(|action| {
        action
            .action
            .name
            .eq_ignore_ascii_case("AssaultRifleAttackAction")
    }));
    let rifle = marine
        .attack_profile("AssaultRifleAttackAction")
        .unwrap_or_else(|| {
            let issue = loaded
                .simulation
                .gameplay
                .timing_issues()
                .iter()
                .find(|issue| {
                    issue
                        .proto_object_name()
                        .eq_ignore_ascii_case("unsc_inf_marine_01")
                        && issue
                            .action_name()
                            .eq_ignore_ascii_case("AssaultRifleAttackAction")
                })
                .map_or("no timing diagnostic", |issue| issue.reason());
            panic!("scenario-layered Marine rifle timing should resolve: {issue}")
        });
    assert_eq!(
        loaded
            .simulation
            .gameplay
            .initial_attack_profile("unsc_inf_marine_01")
            .map(|profile| profile.action_name.as_str()),
        Some("AssaultRifleAttackAction")
    );
    assert!(rifle.damage_per_attack > 0.0);
    assert!(!rifle.animations.is_empty());
    assert!(
        rifle
            .animations
            .iter()
            .any(|animation| !animation.attack_positions.is_empty())
    );
    assert!(
        rifle
            .animations
            .iter()
            .all(|animation| loaded.source.provenance(&animation.asset_path).is_some())
    );
    assert!(
        loaded
            .source
            .provenance_data(marine.tactics_path())
            .is_some()
    );
}

fn assert_real_object_type_catalog(loaded: &LoadedGameScenario) {
    let marine_proto_id = object_prototype_id(&loaded.content.database, "unsc_inf_marine_01")
        .expect("the layered database should resolve the Marine proto-object ID");
    for object_type in ["unsc_inf_marine_01", "Infantry"] {
        assert!(
            loaded
                .simulation
                .world
                .prototype_is_object_type(marine_proto_id, object_type),
            "Marine should retain retail object type {object_type}"
        );
    }
}
