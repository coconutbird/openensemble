use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use pipeline::database::hw1::{ProtoObject, Squad, Tech};

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "object".to_owned(),
            dbid: Some(101),
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

fn script(players: Vec<i32>) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Bool, TriggerValue::Bool(true));
    add_value(
        &mut script,
        2,
        VarType::PlayerList,
        TriggerValue::PlayerList(players),
    );
    add_value(
        &mut script,
        3,
        VarType::TechList,
        TriggerValue::TechList(vec![0, 999]),
    );
    add_value(
        &mut script,
        4,
        VarType::ProtoObjectList,
        TriggerValue::ProtoObjectList(vec![101, 999]),
    );
    add_value(
        &mut script,
        5,
        VarType::ProtoSquadList,
        TriggerValue::ProtoSquadList(vec![201, 999]),
    );
    script
}

fn effect() -> Effect {
    Effect::new(1, EffectType::Forbid)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_input_at(5, 5)
}

#[test]
fn forbid_sets_every_valid_prototype_for_each_valid_player() {
    let database = database();
    let mut world = World::new();
    world.init_players(2);
    let script = script(vec![1, 2, 99]);

    assert_eq!(
        set_forbidden(&effect(), &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    for player_id in [1, 2] {
        let player = world.get_player(player_id).unwrap();
        assert!(player.is_object_forbidden(&database, 101));
        assert!(player.is_squad_forbidden(&database, 201));
        assert!(player.is_technology_forbidden(&database, 0));
    }
}

#[test]
fn empty_player_list_is_an_applied_noop_and_database_is_required() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let script = script(Vec::new());

    assert_eq!(
        set_forbidden(&effect(), &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert!(
        !world
            .get_player(1)
            .unwrap()
            .is_object_forbidden(&database, 101)
    );
    assert_eq!(
        set_forbidden(&effect(), &script, &mut world, None),
        EffectOutcome::Skipped
    );
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
