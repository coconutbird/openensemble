use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};

#[test]
fn get_child_units_clears_and_unique_adds_valid_children_in_retail_order() {
    let mut world = World::new();
    let first_squad = world.create_squad(1);
    let second_squad = world.create_squad(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let third = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first, first_squad));
    assert!(world.attach_unit_to_squad(second, first_squad));
    assert!(world.attach_unit_to_squad(third, second_squad));
    world
        .get_squad_mut(first_squad)
        .unwrap()
        .unit_ids
        .push(EntityId::INVALID);

    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Squad,
        TriggerValue::Squad(first_squad),
    );
    add_value(
        &mut script,
        2,
        VarType::SquadList,
        TriggerValue::SquadList(vec![second_squad, first_squad]),
    );
    add_value(
        &mut script,
        3,
        VarType::UnitList,
        TriggerValue::UnitList(vec![EntityId::INVALID]),
    );
    let effect = Effect::new(1, EffectType::GetChildUnits)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3);

    assert_eq!(
        get_child_units(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(unit_list(&script, 3), &[first, second, third]);

    script.get_variable_mut(1).unwrap().is_null = true;
    script.get_variable_mut(2).unwrap().is_null = true;
    assert_eq!(
        get_child_units(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert!(unit_list(&script, 3).is_empty());
}

#[test]
fn get_parent_squad_writes_invalid_for_standalone_and_preserves_on_invalid_input() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let member = world.create_unit(1);
    let standalone = world.create_unit(1);
    assert!(world.attach_unit_to_squad(member, squad_id));

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(member));
    add_value(
        &mut script,
        2,
        VarType::Squad,
        TriggerValue::Squad(EntityId::INVALID),
    );
    let effect = Effect::new(1, EffectType::GetParentSquad)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    assert_eq!(
        get_parent_squad(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(squad(&script, 2), squad_id);

    script.get_variable_mut(1).unwrap().value = TriggerValue::Unit(standalone);
    assert_eq!(
        get_parent_squad(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(squad(&script, 2), EntityId::INVALID);

    script.get_variable_mut(1).unwrap().value = TriggerValue::Unit(EntityId::INVALID);
    script.get_variable_mut(2).unwrap().value = TriggerValue::Squad(squad_id);
    assert_eq!(
        get_parent_squad(&effect, &mut script, &world),
        EffectOutcome::Skipped
    );
    assert_eq!(squad(&script, 2), squad_id);
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn unit_list(script: &TriggerScript, id: u32) -> &[EntityId] {
    match &script.get_variable(id).expect("unit list").value {
        TriggerValue::UnitList(units) => units,
        value => panic!("expected unit list, got {value:?}"),
    }
}

fn squad(script: &TriggerScript, id: u32) -> EntityId {
    match script.get_variable(id).expect("squad").value {
        TriggerValue::Squad(squad_id) => squad_id,
        ref value => panic!("expected squad, got {value:?}"),
    }
}
