use super::*;
use crate::physics::MotionType;
use crate::trigger::{TriggerVar, TriggerVec3, VarType};
use pipeline::database::hw1::gamedata::{CodeProtoObject, CodeProtoObjectsWrapper};
use pipeline::database::hw1::{GameData, ProtoObject};

#[test]
fn obstruction_unit_uses_code_mapping_static_collider_and_retail_outputs() {
    assert_eq!(
        EffectType::from_u16(833),
        Some(EffectType::CreateObstructionUnit)
    );
    let database = obstruction_database();
    let mut world = World::new();
    world.init_players(0);
    let previous = world.create_unit(0);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        11,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(10.0, 2.0, 30.0)),
    );
    add_value(
        &mut script,
        12,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(3.0, 0.0, 4.0)),
    );
    add_value(&mut script, 13, VarType::Float, TriggerValue::Float(29.0));
    add_value(&mut script, 14, VarType::Float, TriggerValue::Float(3.0));
    add_value(&mut script, 15, VarType::Float, TriggerValue::Float(30.0));
    add_value(&mut script, 16, VarType::Bool, TriggerValue::Bool(true));
    add_value(
        &mut script,
        17,
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    add_value(
        &mut script,
        18,
        VarType::UnitList,
        TriggerValue::UnitList(vec![previous]),
    );
    let mut effect = Effect::new(1, EffectType::CreateObstructionUnit);
    for (slot, variable_id) in (1..=8).zip(11..=18) {
        effect = effect.with_input_at(slot, variable_id);
    }
    effect.version = u8::MAX;

    assert_eq!(
        create_obstruction_unit(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let created = unit(&script, 17);
    assert_ne!(created, previous);
    assert_eq!(unit_list(&script, 18), &[created]);
    let obstruction = world.get_unit(created).expect("obstruction unit");
    assert_eq!(obstruction.base.player_id, 0);
    assert_eq!(obstruction.proto_object_id, 41);
    assert_eq!(obstruction.proto_object_name, "sys_obstruction");
    assert_eq!(obstruction.base.position, Vec3::new(10.0, 2.0, 30.0));
    assert_eq!(obstruction.base.forward, Vec3::new(0.6, 0.0, 0.8));
    assert_eq!(
        obstruction.obstruction_half_extents,
        Vec3::new(29.0, 3.0, 30.0)
    );
    let body = obstruction.physics.as_ref().expect("static collider");
    assert_eq!(body.motion_type(), MotionType::Static);
    assert_eq!(
        body.collider().half_extents,
        obstruction.obstruction_half_extents
    );
    assert!(!obstruction.base.is_mobile());

    let unit_count = world.units.len();
    script.get_variable_mut(15).unwrap().value = TriggerValue::Bool(false);
    assert_eq!(
        create_obstruction_unit(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Skipped
    );
    assert_eq!(world.units.len(), unit_count);
    assert_eq!(unit(&script, 17), created);
}

fn obstruction_database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "sys_obstruction".to_owned(),
        dbid: Some(41),
        object_class: Some("Unit".to_owned()),
        flags: vec!["Immoveable".to_owned(), "NoRender".to_owned()],
        ..ProtoObject::default()
    });
    database.game_data = Some(GameData {
        code_proto_objects: Some(CodeProtoObjectsWrapper {
            entries: vec![CodeProtoObject {
                object_type: "Obstruction".to_owned(),
                proto_name: "sys_obstruction".to_owned(),
            }],
        }),
        ..GameData::default()
    });
    database
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn unit(script: &TriggerScript, id: u32) -> EntityId {
    match script.get_variable(id).expect("unit variable").value {
        TriggerValue::Unit(value) => value,
        ref value => panic!("expected unit, got {value:?}"),
    }
}

fn unit_list(script: &TriggerScript, id: u32) -> &[EntityId] {
    match &script.get_variable(id).expect("unit list variable").value {
        TriggerValue::UnitList(values) => values,
        value => panic!("expected unit list, got {value:?}"),
    }
}
