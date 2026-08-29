use sim::{LoadedGameScenario, UnitRevivalProfile, object_prototype_id};

pub(super) fn assert_loaded_gameplay_catalog(loaded: &LoadedGameScenario) {
    assert_real_object_type_catalog(loaded);
    assert_real_revival_catalog(loaded);
    assert_real_timed_projectile_profiles(loaded);
    assert!(loaded.simulation.gameplay.referenced_tactic_count() > 0);
    assert!(!loaded.simulation.gameplay.is_empty());
    let authored_intercept_distance = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|data| data.track_intercept_distance)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default();
    assert!(
        (loaded.simulation.gameplay.track_intercept_distance() - authored_intercept_distance).abs()
            < f32::EPSILON
    );
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
    assert_real_rifle_accuracy(marine, rifle);
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
    assert_real_projectile_profile(loaded, rifle);
    let rocket = marine
        .attack_profile("RocketAttackAction")
        .expect("scenario-layered Marine rocket timing should resolve");
    let area = rocket
        .area_damage
        .expect("Marine rockets should retain authored area damage");
    assert!((area.radius - 4.0).abs() < f32::EPSILON);
    assert!(area.primary_target_factor.abs() < f32::EPSILON);
    assert!((area.distance_factor - 0.25).abs() < f32::EPSILON);
    assert!((area.damage_factor - 0.5).abs() < f32::EPSILON);
    assert!(!area.linear_damage);
    assert!(!area.ignores_y_axis);
    assert!(!area.friendly_fire);
    assert_real_marine_rocket_perturbance(loaded, rocket);
    let grenade = marine
        .attack_profile("GrenadeAttackAction")
        .expect("scenario-layered Marine grenade timing should resolve");
    assert_real_ballistic_projectile_profile(loaded, grenade);
}

fn assert_real_timed_projectile_profiles(loaded: &LoadedGameScenario) {
    for (name, explodes, expires) in [
        ("fx_proj_plasmaGrenade_01", true, false),
        ("fx_proj_plasmaburn_01", false, true),
    ] {
        let authored = loaded
            .content
            .database
            .objects
            .iter()
            .find(|object| object.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("layered database should contain {name}"));
        let profile = loaded
            .simulation
            .gameplay
            .projectile(name)
            .unwrap_or_else(|| panic!("simulation should expose {name}"));
        assert_eq!(profile.explodes_on_timer(), explodes);
        assert_eq!(profile.expires_on_timer(), expires);
        assert!(profile.is_sticky());
        assert_eq!(
            profile.explodes_on_timer(),
            has_flag(authored, "ExplodeOnTimer")
        );
        assert_eq!(
            profile.expires_on_timer(),
            has_flag(authored, "ExpireOnTimer")
        );
        assert_eq!(profile.is_sticky(), has_flag(authored, "IsSticky"));
    }
}

fn assert_real_marine_rocket_perturbance(loaded: &LoadedGameScenario, rocket: &sim::AttackProfile) {
    let projectile_name = rocket
        .projectile
        .as_deref()
        .expect("Marine rocket action should reference a projectile");
    let authored = loaded
        .content
        .database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(projectile_name))
        .expect("layered database should contain the Marine rocket projectile");
    let profile = loaded
        .simulation
        .gameplay
        .projectile(projectile_name)
        .expect("simulation should expose the layered Marine rocket projectile");

    assert!(profile.tracks_target());
    assert!(profile.perturbance.chance > 0.0);
    assert!(
        (profile.perturbance.chance - nonnegative_or_zero(authored.perturbance_chance)).abs()
            < f32::EPSILON
    );
    assert!(
        (profile.perturbance.velocity - nonnegative_or_zero(authored.perturbance_velocity)).abs()
            < f32::EPSILON
    );
    assert!(
        (profile.perturbance.min_time - nonnegative_or_zero(authored.perturbance_min_time)).abs()
            < f32::EPSILON
    );
    assert!(
        (profile.perturbance.max_time - nonnegative_or_zero(authored.perturbance_max_time)).abs()
            < f32::EPSILON
    );
    assert_eq!(
        profile.perturbance.initial.is_some(),
        authored.perturb_initial_velocity.is_some()
    );
}

fn nonnegative_or_zero(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn assert_real_rifle_accuracy(marine: &sim::ObjectGameplay, rifle: &sim::AttackProfile) {
    let authored_rifle = marine
        .ranged_actions()
        .find(|action| {
            action
                .action
                .name
                .eq_ignore_ascii_case("AssaultRifleAttackAction")
        })
        .expect("the Marine rifle action should retain its layered tactic weapon")
        .weapon;
    assert_real_accuracy_profile(rifle, authored_rifle);
}

fn assert_real_accuracy_profile(
    attack: &sim::AttackProfile,
    weapon: &pipeline::database::hw1::tactics::Weapon,
) {
    let defaults = sim::AttackAccuracyProfile::default();
    let expected = sim::AttackAccuracyProfile {
        accuracy: finite_or(weapon.accuracy, defaults.accuracy),
        moving_accuracy: finite_or(weapon.moving_accuracy, defaults.moving_accuracy),
        max_deviation: finite_or(weapon.max_deviation, defaults.max_deviation),
        moving_max_deviation: finite_or(weapon.moving_max_deviation, defaults.moving_max_deviation),
        distance_factor: finite_or(weapon.accuracy_distance_factor, defaults.distance_factor),
        deviation_factor: finite_or(weapon.accuracy_deviation_factor, defaults.deviation_factor),
    };
    assert_eq!(attack.accuracy, expected);
    assert!(
        (attack.max_velocity_lead - finite_or(weapon.max_velocity_lead, 0.0)).abs() < f32::EPSILON
    );
}

fn finite_or(value: Option<f32>, fallback: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

fn assert_real_projectile_profile(loaded: &LoadedGameScenario, attack: &sim::AttackProfile) {
    let projectile_name = attack
        .projectile
        .as_deref()
        .expect("Marine rifle should reference a projectile prototype");
    let authored = loaded
        .content
        .database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(projectile_name))
        .expect("layered database should contain the Marine rifle projectile");
    let profile = loaded
        .simulation
        .gameplay
        .projectile(projectile_name)
        .expect("gameplay catalog should expose the Marine rifle projectile");
    let authored_fuel = authored
        .fuel
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default();
    let authored_acceleration = if authored_fuel > f32::EPSILON {
        authored
            .acceleration
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or_default()
    } else {
        0.0
    };

    assert_eq!(profile.proto_object_name, authored.name);
    assert!((profile.fuel - authored_fuel).abs() < f32::EPSILON);
    assert!((profile.acceleration - authored_acceleration).abs() < f32::EPSILON);
    assert!(
        (profile.max_projectile_height - authored.max_projectile_height.unwrap_or_default()).abs()
            < f32::EPSILON
    );
    assert_eq!(profile.tracks_target(), has_flag(authored, "Tracking"));
    assert_eq!(
        profile.is_affected_by_gravity(),
        has_flag(authored, "IsAffectedByGravity")
    );
    assert_eq!(profile.tumbles(), has_flag(authored, "ProjectileTumbles"));
    assert_eq!(
        profile.allows_self_damage(),
        has_flag(authored, "SelfDamage")
    );
    assert!(!attack.friendly_fire);
    assert!(!attack.targets_foot_of_unit);
}

fn assert_real_ballistic_projectile_profile(
    loaded: &LoadedGameScenario,
    attack: &sim::AttackProfile,
) {
    let projectile_name = attack
        .projectile
        .as_deref()
        .expect("Marine grenade should reference a projectile prototype");
    let authored = loaded
        .content
        .database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(projectile_name))
        .expect("layered database should contain the Marine grenade projectile");
    let profile = loaded
        .simulation
        .gameplay
        .projectile(projectile_name)
        .expect("gameplay catalog should expose the Marine grenade projectile");

    assert!(has_flag(authored, "IsAffectedByGravity"));
    assert!(profile.is_affected_by_gravity());
    assert!(profile.max_projectile_height > 0.0);
    assert!(
        (profile.max_projectile_height - authored.max_projectile_height.unwrap_or_default()).abs()
            < f32::EPSILON
    );
}

fn has_flag(object: &pipeline::database::hw1::ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn assert_real_revival_catalog(loaded: &LoadedGameScenario) {
    let Some(UnitRevivalProfile::Hero(hero)) = loaded
        .simulation
        .gameplay
        .unit_revival_profile("cpgn_inf_spartanRocket_01")
    else {
        panic!("scenario-layered campaign Spartan should resolve HeroDeath");
    };
    assert!((hero.hp_regen_time - 90.0).abs() < f32::EPSILON);
    assert!((hero.revival_distance - 15.0).abs() < f32::EPSILON);
    assert!((hero.hitpoint_threshold - 0.5).abs() < f32::EPSILON);
    assert!(loaded.simulation.gameplay.objects().any(|object| matches!(
        loaded
            .simulation
            .gameplay
            .unit_revival_profile(object.proto_object_name()),
        Some(UnitRevivalProfile::Revive(_))
    )));
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
