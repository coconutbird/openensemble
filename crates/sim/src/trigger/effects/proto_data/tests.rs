use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use pipeline::database::hw1::{ProtoObject, Tech};

fn variable(id: u32, var_type: VarType, value: TriggerValue) -> TriggerVar {
    TriggerVar::new(id, var_type)
        .with_name(format!("v{id}"))
        .with_value(value)
}

fn effect(bindings: &[(u16, u32)]) -> Effect {
    let mut effect = Effect::new(1, EffectType::ModifyProtoData);
    effect.version = 5;
    for &(signature_id, variable_id) in bindings {
        effect = effect.with_input_at(signature_id, variable_id);
    }
    effect
}

fn scalar_script(data_type: &str, amount: f32) -> TriggerScript {
    let mut script = TriggerScript::new(1).with_name("proto data");
    script.add_variable(variable(1, VarType::Player, TriggerValue::Player(1)));
    script.add_variable(variable(
        2,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1, 2]),
    ));
    script.add_variable(variable(
        3,
        VarType::ObjectType,
        TriggerValue::ObjectType("Unit".to_owned()),
    ));
    script.add_variable(variable(4, VarType::Float, TriggerValue::Float(amount)));
    script.add_variable(variable(
        5,
        VarType::ObjectDataType,
        TriggerValue::String(data_type.to_owned()),
    ));
    script.add_variable(variable(
        6,
        VarType::ObjectDataRelative,
        TriggerValue::String("Percent".to_owned()),
    ));
    script.add_variable(variable(7, VarType::Bool, TriggerValue::Bool(true)));
    script
}

#[test]
fn player_union_and_object_type_reconcile_live_hitpoints_once() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "marine".to_owned(),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(100.0),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(2);
    for player_id in [1, 2] {
        let unit_id = world.create_unit(player_id);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "marine".to_owned();
        unit.hitpoints = 50.0;
        unit.max_hitpoints = 100.0;
    }
    let script = scalar_script("Hitpoints", 2.0);
    let effect = effect(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]);

    assert_eq!(
        modify_proto_data(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    for player_id in [1, 2] {
        let unit = world
            .units
            .iter()
            .find(|(_, unit)| unit.base.player_id == player_id)
            .unwrap()
            .1;
        assert_close(unit.hitpoints, 100.0);
        assert_close(unit.max_hitpoints, 200.0);
        assert_eq!(
            world
                .get_player(player_id)
                .unwrap()
                .technologies
                .runtime_proto_modification_count(),
            1
        );
    }
}

#[test]
fn named_weapon_damage_is_player_owned_and_amount_precedes_percent() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "marine".to_owned(),
            object_types: vec!["Unit".to_owned()],
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(1);
    let mut script = scalar_script("Damage", 3.0);
    script.add_variable(variable(8, VarType::Float, TriggerValue::Float(99.0)));
    script.add_variable(variable(
        9,
        VarType::String,
        TriggerValue::String("Rifle".to_owned()),
    ));
    script.get_variable_mut(7).unwrap().value = TriggerValue::Bool(false);
    let effect = effect(&[
        (1, 1),
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 6),
        (7, 7),
        (8, 8),
        (9, 9),
    ]);

    assert_eq!(
        modify_proto_data(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let technologies = &world.get_player(1).unwrap().technologies;
    assert_close(technologies.weapon_damage("marine", "Rifle", 10.0), 30.0);
    assert_close(technologies.weapon_damage("marine", "Pistol", 10.0), 10.0);
}

#[test]
fn version_five_tech_command_data_takes_precedence() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "base".to_owned(),
            object_class: Some("Building".to_owned()),
            ..ProtoObject::default()
        }],
        techs: vec![Tech {
            name: "Upgrade".to_owned(),
            ..Tech::default()
        }],
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(1);
    let mut script = scalar_script("CommandEnable", 1.0);
    script.get_variable_mut(3).unwrap().value = TriggerValue::ObjectType("Building".to_owned());
    script.add_variable(variable(
        10,
        VarType::TechDataCommandType,
        TriggerValue::Int(0),
    ));
    script.add_variable(variable(11, VarType::Integer, TriggerValue::Int(99)));
    script.add_variable(variable(12, VarType::Tech, TriggerValue::Tech(0)));
    let effect = effect(&[
        (1, 1),
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 6),
        (7, 7),
        (11, 10),
        (12, 11),
        (13, 12),
    ]);

    assert_eq!(
        modify_proto_data(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .command_enabled("base", "Research", "Upgrade", false)
    );
}

#[test]
fn obsolete_versions_and_unknown_subtypes_remain_visible() {
    let database = Database::default();
    let mut world = World::new();
    world.init_players(1);
    let script = scalar_script("UnknownData", 1.0);
    let mut effect = effect(&[(1, 1), (3, 3), (4, 4), (5, 5), (6, 6)]);
    assert_eq!(
        modify_proto_data(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Unsupported(237)
    );
    effect.version = 3;
    assert_eq!(
        modify_proto_data(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Unsupported(237)
    );
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
