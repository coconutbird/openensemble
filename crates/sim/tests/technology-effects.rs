use glam::Vec3;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::TacticData;
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad, Tech};
use sim::{GameplayCatalog, World, spawn_squad_at, squad_prototype_id};

#[test]
fn marine_technology_chain_updates_live_and_future_sim_entities() {
    let database = marine_database();
    let mut world = World::new();
    world.init_players(1);
    let marine_id = squad_prototype_id(&database, "unsc_inf_marine_01").unwrap();
    let first = spawn_squad_at(&mut world, &database, 1, marine_id, Vec3::ZERO, Vec3::Z).unwrap();
    assert_eq!(world.get_squad(first).unwrap().unit_ids.len(), 2);

    let baseline_checksum = world.checksum();
    assert!(
        world
            .activate_technology(1, &database, "unsc_marine_upgrade1")
            .unwrap()
    );
    assert_eq!(world.get_squad(first).unwrap().unit_ids.len(), 3);
    assert_eq!(count_members(&world, first, "unsc_inf_marine_02"), 1);
    assert!(nearly_equal(
        world.get_player(1).unwrap().technologies.weapon_damage(
            "unsc_inf_marine_01",
            "Grenade",
            100.0
        ),
        125.0
    ));

    assert!(
        world
            .activate_technology(1, &database, "unsc_marine_upgrade2")
            .unwrap()
    );
    let technology = &world.get_player(1).unwrap().technologies;
    assert!(!technology.action_enabled("unsc_inf_marine_01", "GrenadeAttackAction", true));
    assert!(technology.action_enabled("unsc_inf_marine_01", "RocketAttackAction", false));
    assert!(nearly_equal(
        technology.weapon_damage("unsc_inf_marine_01", "AssaultRifle", 100.0),
        125.0
    ));
    assert!(
        world
            .get_squad(first)
            .unwrap()
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .all(|unit| {
                nearly_equal(unit.max_hitpoints, 125.0) && nearly_equal(unit.hitpoints, 125.0)
            })
    );

    let second = spawn_squad_at(&mut world, &database, 1, marine_id, Vec3::X, Vec3::Z).unwrap();
    assert_eq!(world.get_squad(second).unwrap().unit_ids.len(), 3);
    assert!(
        world
            .get_squad(second)
            .unwrap()
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .all(|unit| nearly_equal(unit.max_hitpoints, 125.0))
    );

    assert!(
        world
            .activate_technology(1, &database, "unsc_marine_upgrade3")
            .unwrap()
    );
    assert_eq!(count_members(&world, first, "unsc_inf_medic_01"), 1);
    assert_eq!(count_members(&world, second, "unsc_inf_medic_01"), 1);
    let third =
        spawn_squad_at(&mut world, &database, 1, marine_id, Vec3::X * 2.0, Vec3::Z).unwrap();
    assert_eq!(world.get_squad(third).unwrap().unit_ids.len(), 4);
    assert_ne!(world.checksum(), baseline_checksum);

    assert!(
        world
            .deactivate_technology(1, &database, "unsc_marine_upgrade2")
            .unwrap()
    );
    assert!(
        world
            .get_squad(first)
            .unwrap()
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .all(|unit| {
                nearly_equal(unit.max_hitpoints, 100.0) && nearly_equal(unit.hitpoints, 100.0)
            })
    );
    assert_eq!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .resolved_squad_prototype("unsc_inf_marine_01"),
        "unsc_inf_marine_03"
    );
}

#[test]
fn player_ability_recovery_modifier_uses_retail_percent_math() {
    let database = marine_database();
    let mut world = World::new();
    world.init_players(1);

    assert!(
        world
            .activate_technology(1, &database, "skull_half_recovery")
            .unwrap()
    );
    assert!(nearly_equal(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .ability_recovery_time("UnscMarineRockets", 20.0),
        10.0
    ));
}

#[test]
fn shield_technology_updates_live_and_future_units_and_recharge_timing() {
    let database = shield_database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, std::iter::empty::<(String, TacticData)>());
    let mut world = World::new();
    world.init_players(1);
    let ghost_id = squad_prototype_id(&database, "cov_veh_ghost_01").unwrap();
    let first = spawn_squad_at(&mut world, &database, 1, ghost_id, Vec3::ZERO, Vec3::Z).unwrap();
    let first_unit_id = world.get_squad(first).unwrap().unit_ids[0];
    assert!(nearly_equal(
        world.get_unit(first_unit_id).unwrap().shields.maximum,
        0.0
    ));

    assert!(
        world
            .activate_technology(1, &database, "shield_upgrade")
            .unwrap()
    );
    assert!(nearly_equal(
        world.get_unit(first_unit_id).unwrap().shields.maximum,
        1_200.0
    ));

    let second = spawn_squad_at(&mut world, &database, 1, ghost_id, Vec3::X, Vec3::Z).unwrap();
    let second_unit_id = world.get_squad(second).unwrap().unit_ids[0];
    assert!(nearly_equal(
        world.get_unit(second_unit_id).unwrap().shields.maximum,
        1_200.0
    ));

    world.update_entities_with_gameplay(1.25, &gameplay);
    assert!(nearly_equal(
        world.get_unit(first_unit_id).unwrap().shields.current,
        1_200.0
    ));
    assert!(nearly_equal(
        world.get_unit(second_unit_id).unwrap().shields.current,
        1_200.0
    ));

    // Retail recharge actions retain their fixed five-second lifetime even
    // after reaching full shields, so let the birth actions finish first.
    world.update_entities_with_gameplay(3.75, &gameplay);
    assert!(world.damage_unit(first_unit_id, 200.0));
    world.update_entities_with_gameplay(5.0, &gameplay);
    assert!(nearly_equal(
        world.get_unit(first_unit_id).unwrap().shields.current,
        1_000.0
    ));
    world.update_entities_with_gameplay(0.1, &gameplay);
    assert!(world.get_unit(first_unit_id).unwrap().shields.current > 1_000.0);
}

fn marine_database() -> Database {
    let mut database = Database::new();
    database.objects.extend([
        marine_object("unsc_inf_marine_01"),
        marine_object("unsc_inf_marine_02"),
        marine_object("unsc_inf_medic_01"),
    ]);
    database.squads.extend(marine_squads());
    database.techs.extend(marine_technologies());
    database
}

fn shield_database() -> Database {
    let mut database = Database::new();
    database.game_data = Some(GameData {
        shield_regen_delay: Some(20.0),
        shield_regen_time: Some(5.0),
        ..GameData::default()
    });
    database.objects.push(ProtoObject {
        name: "cov_veh_ghost_01".to_owned(),
        object_class: Some("Unit".to_owned()),
        damage_type: Some("Shielded".to_owned()),
        hitpoints: Some(100.0),
        ..ProtoObject::default()
    });
    database.squads.push(marine_squad(
        "cov_veh_ghost_01",
        20,
        &[("cov_veh_ghost_01", 1)],
    ));
    database.techs.push(Tech {
        name: "shield_upgrade".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                scalar_effect(
                    "Shieldpoints",
                    1_200.0,
                    "Absolute",
                    "ProtoUnit",
                    Some("cov_veh_ghost_01"),
                    None,
                    false,
                ),
                scalar_effect(
                    "ShieldRegenRate",
                    2.0,
                    "Percent",
                    "ProtoUnit",
                    Some("cov_veh_ghost_01"),
                    None,
                    false,
                ),
                scalar_effect(
                    "ShieldRegenDelay",
                    0.5,
                    "Percent",
                    "ProtoUnit",
                    Some("cov_veh_ghost_01"),
                    None,
                    false,
                ),
                scalar_effect(
                    "ShieldRegenRate",
                    2.0,
                    "Percent",
                    "Player",
                    None,
                    None,
                    false,
                ),
                scalar_effect(
                    "ShieldRegenDelay",
                    0.5,
                    "Percent",
                    "Player",
                    None,
                    None,
                    false,
                ),
            ],
        }),
        ..Tech::default()
    });
    database
}

fn marine_squads() -> [ProtoSquad; 3] {
    [
        marine_squad("unsc_inf_marine_01", 10, &[("unsc_inf_marine_01", 2)]),
        marine_squad(
            "unsc_inf_marine_02",
            11,
            &[("unsc_inf_marine_01", 2), ("unsc_inf_marine_02", 1)],
        ),
        marine_squad(
            "unsc_inf_marine_03",
            12,
            &[
                ("unsc_inf_marine_01", 2),
                ("unsc_inf_marine_02", 1),
                ("unsc_inf_medic_01", 1),
            ],
        ),
    ]
}

fn marine_technologies() -> [Tech; 4] {
    [
        Tech {
            name: "unsc_marine_upgrade1".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![
                    scalar_effect(
                        "Damage",
                        1.25,
                        "Percent",
                        "ProtoUnit",
                        Some("unsc_inf_marine_01"),
                        Some("Grenade"),
                        false,
                    ),
                    transform_effect("unsc_inf_marine_01", "unsc_inf_marine_02"),
                ],
            }),
            ..Tech::default()
        },
        Tech {
            name: "unsc_marine_upgrade2".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![
                    scalar_effect(
                        "Damage",
                        1.25,
                        "Percent",
                        "ProtoUnit",
                        Some("unsc_inf_marine_01"),
                        None,
                        true,
                    ),
                    scalar_effect(
                        "Hitpoints",
                        1.25,
                        "Percent",
                        "ProtoUnit",
                        Some("unsc_inf_marine_01"),
                        None,
                        false,
                    ),
                    scalar_effect(
                        "Hitpoints",
                        1.25,
                        "Percent",
                        "ProtoUnit",
                        Some("unsc_inf_marine_02"),
                        None,
                        false,
                    ),
                    scalar_effect(
                        "Hitpoints",
                        1.25,
                        "Percent",
                        "ProtoUnit",
                        Some("unsc_inf_medic_01"),
                        None,
                        false,
                    ),
                    action_effect("GrenadeAttackAction", false),
                    action_effect("RocketAttackAction", true),
                ],
            }),
            ..Tech::default()
        },
        Tech {
            name: "unsc_marine_upgrade3".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![transform_effect("unsc_inf_marine_01", "unsc_inf_marine_03")],
            }),
            ..Tech::default()
        },
        Tech {
            name: "skull_half_recovery".to_owned(),
            effects: Some(EffectsWrapper {
                entries: vec![ability_recovery_effect("UnscMarineRockets", 0.5)],
            }),
            ..Tech::default()
        },
    ]
}

fn marine_object(name: &str) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        object_class: Some("Unit".to_owned()),
        hitpoints: Some(100.0),
        ..ProtoObject::default()
    }
}

fn marine_squad(name: &str, dbid: i32, units: &[(&str, i32)]) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        formation_type: Some("Flock".to_owned()),
        units: Some(UnitsWrapper {
            entries: units
                .iter()
                .map(|(name, count)| UnitEntry {
                    proto_object: (*name).to_owned(),
                    count: *count,
                    ..UnitEntry::default()
                })
                .collect(),
        }),
        ..ProtoSquad::default()
    }
}

fn action_effect(action: &str, enabled: bool) -> TechEffect {
    scalar_effect(
        "ActionEnable",
        if enabled { 1.0 } else { 0.0 },
        "Absolute",
        "ProtoUnit",
        Some("unsc_inf_marine_01"),
        Some(action),
        false,
    )
}

fn scalar_effect(
    subtype: &str,
    amount: f32,
    relativity: &str,
    target_type: &str,
    target_value: Option<&str>,
    action: Option<&str>,
    allactions: bool,
) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        amount: Some(amount),
        subtype: Some(subtype.to_owned()),
        relativity: Some(relativity.to_owned()),
        target: Some(EffectTarget {
            target_type: Some(target_type.to_owned()),
            value: target_value.map(str::to_owned),
        }),
        action: action.map(str::to_owned),
        allactions: Some(allactions),
        ..TechEffect::default()
    }
}

fn ability_recovery_effect(ability: &str, amount: f32) -> TechEffect {
    TechEffect {
        ability: Some(ability.to_owned()),
        ..scalar_effect(
            "AbilityRecoverTime",
            amount,
            "Percent",
            "Player",
            None,
            None,
            false,
        )
    }
}

fn transform_effect(from: &str, to: &str) -> TechEffect {
    TechEffect {
        effect_type: "TransformProtoSquad".to_owned(),
        from_type: Some(from.to_owned()),
        to_type: Some(to.to_owned()),
        ..TechEffect::default()
    }
}

fn count_members(world: &World, squad_id: sim::EntityId, proto_object: &str) -> usize {
    world
        .get_squad(squad_id)
        .unwrap()
        .unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
        .filter(|unit| unit.proto_object_name.eq_ignore_ascii_case(proto_object))
        .count()
}

fn nearly_equal(left: f32, right: f32) -> bool {
    let tolerance = f32::EPSILON * left.abs().max(right.abs()).max(1.0) * 4.0;
    (left - right).abs() <= tolerance
}
