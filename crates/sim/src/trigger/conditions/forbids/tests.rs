use super::*;
use crate::trigger::{ConditionType, TriggerValue, TriggerVar, VarType};
use pipeline::database::hw1::{ProtoObject, Squad, Tech};

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "object".to_owned(),
            dbid: Some(101),
            flags: vec!["Forbid".to_owned()],
            ..ProtoObject::default()
        }],
        squads: vec![Squad {
            name: "squad".to_owned(),
            dbid: Some(201),
            ..Squad::default()
        }],
        techs: vec![Tech {
            name: "technology".to_owned(),
            ..Tech::default()
        }],
        ..Database::default()
    }
}

#[test]
fn is_forbidden_ors_each_used_prototype_kind() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(201),
    );
    add_value(
        &mut script,
        3,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(101),
    );
    add_value(&mut script, 4, VarType::Tech, TriggerValue::Tech(0));
    let all = Condition::new(1, ConditionType::IsForbidden)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    assert!(is_forbidden(&all, &script, &world, Some(&database)));

    world
        .get_player_mut(1)
        .unwrap()
        .set_object_forbidden(&database, 101, false);
    assert!(!is_forbidden(&all, &script, &world, Some(&database)));
    world
        .get_player_mut(1)
        .unwrap()
        .set_squad_forbidden(&database, 201, true);
    assert!(is_forbidden(&all, &script, &world, Some(&database)));
}

#[test]
fn is_forbidden_requires_a_database_and_valid_player() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(99));
    add_value(
        &mut script,
        2,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(101),
    );
    let condition = Condition::new(1, ConditionType::IsForbidden)
        .with_input_at(1, 1)
        .with_input_at(3, 2);

    assert!(!is_forbidden(&condition, &script, &world, Some(&database)));
    script.get_variable_mut(1).unwrap().value = TriggerValue::Player(1);
    assert!(!is_forbidden(&condition, &script, &world, None));
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
