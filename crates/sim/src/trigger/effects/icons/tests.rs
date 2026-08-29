use super::*;
use crate::trigger::{TriggerColor, TriggerVar, TriggerVec3, VarType};
use pipeline::database::hw1::ProtoObject;

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "sys_icon_30_01".to_owned(),
            dbid: Some(30),
            object_class: Some("Object".to_owned()),
            object_types: vec!["Icon".to_owned()],
            flags: vec!["VisibleForOwnerOnly".to_owned()],
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add(&mut script, 2, VarType::Player, TriggerValue::Player(1));
    add(
        &mut script,
        3,
        VarType::Vector,
        TriggerValue::Location(TriggerVec3::new(10.0, 0.0, 20.0)),
    );
    add(
        &mut script,
        4,
        VarType::Color,
        TriggerValue::Color(TriggerColor::new(255, 255, 0, 128)),
    );
    add(&mut script, 5, VarType::Bool, TriggerValue::Bool(true));
    add(
        &mut script,
        6,
        VarType::Object,
        TriggerValue::Object(EntityId::INVALID),
    );
    add(
        &mut script,
        7,
        VarType::ObjectList,
        TriggerValue::ObjectList(vec![EntityId::new(crate::EntityClass::Object, 99)]),
    );
    add(
        &mut script,
        9,
        VarType::IconType,
        TriggerValue::String("sys_icon_30_01".to_owned()),
    );
    add(&mut script, 10, VarType::Bool, TriggerValue::Bool(true));
    script
}

fn effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::CreateIconObject)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_input_at(5, 5)
        .with_output_at(6, 6)
        .with_output_at(7, 7)
        .with_input_at(9, 9)
        .with_input_at(10, 10);
    effect.version = version;
    effect
}

#[test]
fn version_three_creates_sim_icon_and_updates_retail_outputs() {
    assert_eq!(
        EffectType::from_u16(520),
        Some(EffectType::CreateIconObject)
    );
    let database = database();
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let mut script = script();

    assert_eq!(
        execute(&effect(3), &mut script, &mut world, Some(&database)),
        Some(EffectOutcome::Applied)
    );
    let TriggerValue::Object(icon_id) = script.get_variable(6).unwrap().value else {
        panic!("icon object output")
    };
    let icon = world.get_object(icon_id).unwrap().icon().copied().unwrap();
    assert_eq!(icon.color_override(), Some([255, 255, 0]));
    assert!(icon.visible_to_all());
    assert!(!icon.visible_for_owner_only());
    assert_eq!(
        script.get_variable(7).unwrap().value,
        TriggerValue::ObjectList(vec![icon_id])
    );
}

#[test]
fn failed_creation_writes_invalid_object_and_clears_only_when_requested() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let mut script = script();
    script.get_variable_mut(9).unwrap().value = TriggerValue::String("not-an-icon".to_owned());

    assert_eq!(
        execute(&effect(2), &mut script, &mut world, Some(&database)),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(
        script.get_variable(6).unwrap().value,
        TriggerValue::Object(EntityId::INVALID)
    );
    assert_eq!(
        script.get_variable(7).unwrap().value,
        TriggerValue::ObjectList(Vec::new())
    );
}
