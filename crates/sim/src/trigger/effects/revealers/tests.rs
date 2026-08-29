use super::*;
use crate::trigger::{EffectType, TriggerVar, TriggerVec3, VarType};
use pipeline::database::hw1::{GameData, ProtoObject};

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "sys_revealer".to_owned(),
            dbid: Some(13),
            los: Some(1.0),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            minimum_revealer_size: Some(4.0),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world
}

fn effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::Revealer)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 5)
        .with_output_at(6, 6)
        .with_input_at(7, 7)
        .with_input_at(8, 8);
    effect.version = version;
    effect
}

fn script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(30.0, 0.0, 40.0)),
    );
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(250));
    add_value(&mut script, 4, VarType::Float, TriggerValue::Float(2.0));
    add_value(
        &mut script,
        5,
        VarType::Object,
        TriggerValue::Object(EntityId::INVALID),
    );
    add_value(
        &mut script,
        6,
        VarType::ObjectList,
        TriggerValue::ObjectList(vec![EntityId::new(crate::EntityClass::Unit, 99)]),
    );
    add_value(&mut script, 7, VarType::Bool, TriggerValue::Bool(true));
    add_value(
        &mut script,
        8,
        VarType::VectorList,
        TriggerValue::VectorList(vec![TriggerVec3::new(10.0, 0.0, 20.0)]),
    );
    script
}

#[test]
fn version_two_creates_one_timed_class_zero_object_and_updates_outputs() {
    let database = database();
    let mut world = world();
    world.advance_time(1_000);
    let mut script = script();

    assert_eq!(
        create(&effect(2), &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let TriggerValue::Object(created) = script.get_variable(5).unwrap().value else {
        panic!("created revealer output")
    };
    let revealer = world.get_revealer(created).unwrap();
    assert_eq!(created.class(), Some(crate::EntityClass::Object));
    assert!((revealer.line_of_sight_scalar() - 4.0).abs() < f32::EPSILON);
    assert_eq!(revealer.lifespan_expiration_ms(), Some(1_250));
    assert_eq!(
        script.get_variable(6).unwrap().value,
        TriggerValue::ObjectList(vec![created])
    );
}

#[test]
fn version_two_zero_lifespan_is_timed_and_failure_leaves_list_untouched() {
    let database = database();
    let mut world = world();
    let mut script = script();
    script.get_variable_mut(3).unwrap().value = TriggerValue::Time(0);
    script.get_variable_mut(7).unwrap().value = TriggerValue::Bool(false);

    assert_eq!(
        create(&effect(2), &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let TriggerValue::Object(created) = script.get_variable(5).unwrap().value else {
        panic!("created revealer output")
    };
    assert_eq!(
        world
            .get_revealer(created)
            .unwrap()
            .lifespan_expiration_ms(),
        Some(0)
    );
    world.update_entities(0.05);
    assert!(world.get_revealer(created).is_none());

    script.get_variable_mut(1).unwrap().value = TriggerValue::Player(99);
    let prior_list = script.get_variable(6).unwrap().value.clone();
    assert_eq!(
        create(&effect(2), &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(5).unwrap().value,
        TriggerValue::Object(EntityId::INVALID)
    );
    assert_eq!(script.get_variable(6).unwrap().value, prior_list);
}

#[test]
fn version_three_copies_location_list_then_appends_scalar_and_zero_is_permanent() {
    let database = database();
    let mut world = world();
    let mut script = script();
    script.get_variable_mut(3).unwrap().value = TriggerValue::Time(0);

    assert_eq!(
        create(&effect(3), &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let TriggerValue::ObjectList(created) = &script.get_variable(6).unwrap().value else {
        panic!("revealer list output")
    };
    assert_eq!(created.len(), 2);
    assert_eq!(
        world.get_object(created[0]).unwrap().base.position,
        Vec3::new(10.0, 0.0, 20.0)
    );
    assert_eq!(
        world.get_object(created[1]).unwrap().base.position,
        Vec3::new(30.0, 0.0, 40.0)
    );
    assert!(created.iter().all(|id| {
        world
            .get_revealer(*id)
            .unwrap()
            .lifespan_expiration_ms()
            .is_none()
    }));
    assert_eq!(
        script.get_variable(5).unwrap().value,
        TriggerValue::Object(created[0])
    );
}

#[test]
fn version_three_empty_or_failed_creation_still_writes_outputs_and_unknown_version_is_visible() {
    let database = database();
    let mut world = world();
    let mut script = script();
    script.get_variable_mut(1).unwrap().value = TriggerValue::Player(99);
    script.get_variable_mut(2).unwrap().is_null = true;
    script.get_variable_mut(8).unwrap().value = TriggerValue::VectorList(Vec::new());

    assert_eq!(
        create(&effect(3), &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(5).unwrap().value,
        TriggerValue::Object(EntityId::INVALID)
    );
    assert_eq!(
        script.get_variable(6).unwrap().value,
        TriggerValue::ObjectList(Vec::new())
    );
    assert_eq!(
        create(&effect(4), &mut script, &mut world, Some(&database)),
        EffectOutcome::Unsupported(EffectType::Revealer as u16)
    );
    assert_eq!(
        create(&effect(3), &mut script, &mut world, None),
        EffectOutcome::Skipped
    );
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
