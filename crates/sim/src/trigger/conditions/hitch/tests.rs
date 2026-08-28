use super::*;
use crate::entity_id::EntityClass;
use crate::trigger::{ConditionType, TriggerVar, VarType};

fn add_squad_variable(script: &mut TriggerScript, id: u32, value: EntityId) {
    script.add_variable(TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(value)));
}

#[test]
fn is_hitched_only_writes_output_when_relationship_exists() {
    let mut world = World::new();
    let towing_id = world.create_squad(1);
    let trailer_id = world.create_squad(1);
    let sentinel = EntityId::new(EntityClass::Squad, 99);
    let mut script = TriggerScript::new(1);
    add_squad_variable(&mut script, 1, trailer_id);
    add_squad_variable(&mut script, 2, sentinel);
    let condition = Condition::new(1, ConditionType::IsHitched)
        .with_input_at(2, 1)
        .with_output_at(3, 2);

    assert!(!is_hitched(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(sentinel)
    );

    world.issue_hitch_order(1, towing_id, trailer_id).unwrap();
    assert!(is_hitched(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(towing_id)
    );
}

#[test]
fn has_hitched_always_writes_retail_output() {
    let mut world = World::new();
    let towing_id = world.create_squad(1);
    let trailer_id = world.create_squad(1);
    let sentinel = EntityId::new(EntityClass::Squad, 99);
    let mut script = TriggerScript::new(1);
    add_squad_variable(&mut script, 1, towing_id);
    add_squad_variable(&mut script, 2, sentinel);
    let condition = Condition::new(1, ConditionType::HasHitched)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    assert!(!has_hitched(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(EntityId::INVALID)
    );

    world.issue_hitch_order(1, towing_id, trailer_id).unwrap();
    assert!(has_hitched(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(trailer_id)
    );
}
