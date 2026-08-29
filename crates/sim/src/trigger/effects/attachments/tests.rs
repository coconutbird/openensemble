use super::*;
use crate::trigger::{TriggerVar, VarType};
use pipeline::database::hw1::ProtoObject;

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "sys_icon_27_01".to_owned(),
            dbid: Some(1187),
            object_class: Some("Object".to_owned()),
            object_types: vec!["Icon".to_owned()],
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

#[test]
fn version_five_expands_a_squad_into_unit_owned_icon_entities() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first, squad_id));
    assert!(world.attach_unit_to_squad(second, squad_id));

    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        2,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(1187),
    );
    add(
        &mut script,
        6,
        VarType::Squad,
        TriggerValue::Squad(squad_id),
    );
    let mut effect = Effect::new(1, EffectType::AttachmentAddType)
        .with_input_at(2, 2)
        .with_input_at(6, 6);
    effect.version = 5;

    assert_eq!(
        execute(&effect, &script, &mut world, Some(&database)),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(world.objects.len(), 2);
    for unit_id in [first, second] {
        let [attachment_id] = world.entity_object_state(unit_id).unwrap().attachments() else {
            panic!("one attachment per squad child")
        };
        assert!(world.get_object(*attachment_id).unwrap().icon().is_some());
    }
}

#[test]
fn version_three_deduplicates_receiving_units_before_creation() {
    let database = database();
    let mut world = World::new();
    world.init_players(1);
    let unit_id = world.create_unit(1);
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        2,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(1187),
    );
    add(
        &mut script,
        3,
        VarType::UnitList,
        TriggerValue::UnitList(vec![unit_id, unit_id]),
    );
    add(&mut script, 5, VarType::Unit, TriggerValue::Unit(unit_id));
    let mut effect = Effect::new(1, EffectType::AttachmentAddType)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(5, 5);
    effect.version = 3;

    assert_eq!(
        execute(&effect, &script, &mut world, Some(&database)),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(
        world
            .entity_object_state(unit_id)
            .unwrap()
            .attachments()
            .len(),
        1
    );
}
