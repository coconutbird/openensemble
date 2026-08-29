use super::*;
use crate::entity_id::EntityClass;
use crate::trigger::{EffectType, TriggerVar, VarType};

#[test]
fn retail_callout_ids_are_typed() {
    assert_eq!(
        EffectType::from_u16(809),
        Some(EffectType::HintCalloutCreate)
    );
    assert_eq!(
        EffectType::from_u16(810),
        Some(EffectType::HintCalloutDestroy)
    );
}

#[test]
fn create_uses_squad_unit_location_priority_and_writes_ids() {
    let mut world = World::new();
    let squad = world.create_squad(1);
    let unit = world.create_unit(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::LocStringID, TriggerValue::Int(42));
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(crate::TriggerVec3::new(1.0, 2.0, 3.0)),
    );
    add_value(&mut script, 3, VarType::Squad, TriggerValue::Squad(squad));
    add_value(&mut script, 4, VarType::Unit, TriggerValue::Unit(unit));
    add_value(&mut script, 5, VarType::Integer, TriggerValue::Int(-1));
    let effect = Effect::new(1, EffectType::HintCalloutCreate)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_input_at(5, 3)
        .with_input_at(6, 4)
        .with_output_at(4, 5);

    assert_eq!(
        create(&effect, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(0));
    assert_eq!(
        world.hint_callout(0).unwrap().anchor(),
        crate::HintCalloutAnchor::Entity(squad)
    );

    script.get_variable_mut(3).unwrap().is_null = true;
    assert_eq!(
        create(&effect, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(1));
    assert_eq!(
        world.hint_callout(1).unwrap().anchor(),
        crate::HintCalloutAnchor::Entity(unit)
    );

    script.get_variable_mut(4).unwrap().is_null = true;
    assert_eq!(
        create(&effect, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(2));
    assert_eq!(
        world.hint_callout(2).unwrap().anchor(),
        crate::HintCalloutAnchor::Location(glam::Vec3::new(1.0, 2.0, 3.0))
    );
}

#[test]
fn unit_slot_accepts_the_actual_retail_entity_class_and_destroy_is_idempotent() {
    let mut world = World::new();
    let squad = world.create_squad(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::LocStringID, TriggerValue::Int(9));
    add_value(&mut script, 2, VarType::Unit, TriggerValue::Unit(squad));
    add_value(&mut script, 3, VarType::Integer, TriggerValue::Int(-1));
    let create_effect = Effect::new(1, EffectType::HintCalloutCreate)
        .with_input_at(2, 1)
        .with_input_at(6, 2)
        .with_output_at(4, 3);
    assert_eq!(
        create(&create_effect, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    assert!(world.hint_callout(0).is_some());

    let destroy_effect = Effect::new(2, EffectType::HintCalloutDestroy).with_input_at(1, 3);
    assert_eq!(
        destroy(&destroy_effect, &script, &mut world),
        EffectOutcome::Presentation
    );
    assert!(world.hint_callouts().next().is_none());
    assert_eq!(
        destroy(&destroy_effect, &script, &mut world),
        EffectOutcome::Presentation
    );

    script.get_variable_mut(2).unwrap().value =
        TriggerValue::Unit(EntityId::new(EntityClass::Object, 1));
    assert_eq!(
        create(&create_effect, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    assert_eq!(script.get_variable(3).unwrap().value, TriggerValue::Int(-1));
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
