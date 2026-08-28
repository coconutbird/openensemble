use super::*;
use crate::entities::{Squad, Unit};
use crate::trigger::{TriggerVar, VarType};

fn variable(id: u32, var_type: VarType, value: TriggerValue) -> TriggerVar {
    TriggerVar::new(id, var_type)
        .with_name(format!("v{id}"))
        .with_value(value)
}

fn effect(version: u8, bindings: &[(u16, u32)]) -> Effect {
    let mut effect = Effect::new(1, crate::trigger::EffectType::ModifyDataScalar);
    effect.version = version;
    effect.raw_type = 413;
    for &(signature_id, variable_id) in bindings {
        effect = effect.with_input_at(signature_id, variable_id);
    }
    effect
}

#[test]
fn adjusting_overlapping_unit_and_squad_inputs_only_multiplies_once() {
    let mut world = World::new();
    let unit_id = world.units.allocate_id();
    world.units.insert(unit_id, Unit::new(unit_id, 1));
    let squad_id = world.squads.allocate_id();
    let mut squad = Squad::new(squad_id, 1);
    squad.add_unit(unit_id);
    world.squads.insert(squad_id, squad);

    let mut script = TriggerScript::new(1).with_name("scalar");
    script.add_variable(variable(
        1,
        VarType::UnitList,
        TriggerValue::UnitList(vec![unit_id]),
    ));
    script.add_variable(variable(2, VarType::Unit, TriggerValue::Unit(unit_id)));
    script.add_variable(variable(3, VarType::Squad, TriggerValue::Squad(squad_id)));
    script.add_variable(variable(4, VarType::Float, TriggerValue::Float(0.5)));
    script.add_variable(variable(5, VarType::Bool, TriggerValue::Bool(true)));
    script.add_variable(variable(
        6,
        VarType::DataScalar,
        TriggerValue::String("DamageTaken".to_owned()),
    ));
    let effect = effect(2, &[(1, 2), (2, 1), (3, 3), (6, 4), (7, 5), (8, 6)]);

    assert_eq!(
        modify_data_scalar(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!((world.get_unit(unit_id).unwrap().damage_taken_multiplier - 0.5).abs() < f32::EPSILON);
}

#[test]
fn version_two_assigns_each_retail_scalar() {
    let mut world = World::new();
    let unit_id = world.units.allocate_id();
    world.units.insert(unit_id, Unit::new(unit_id, 1));
    for (name, value) in [
        ("Accuracy", 0.0),
        ("WorkRate", 1.0),
        ("Damage", 2.0),
        ("LOS", 3.0),
        ("Velocity", 4.0),
        ("WeaponRange", 5.0),
        ("DamageTaken", 6.0),
    ] {
        let mut script = TriggerScript::new(1).with_name("scalar");
        script.add_variable(variable(1, VarType::Unit, TriggerValue::Unit(unit_id)));
        script.add_variable(variable(2, VarType::Float, TriggerValue::Float(value)));
        script.add_variable(variable(
            3,
            VarType::DataScalar,
            TriggerValue::String(name.to_owned()),
        ));
        let effect = effect(2, &[(1, 1), (6, 2), (8, 3)]);
        assert_eq!(
            modify_data_scalar(&effect, &script, &mut world),
            EffectOutcome::Applied
        );
        let scalar = UnitDataScalar::from_trigger_value(name).unwrap();
        assert!(
            (world.get_unit(unit_id).unwrap().data_scalar(scalar) - value).abs() < f32::EPSILON
        );
    }
}

#[test]
fn version_one_leaves_damage_taken_untouched() {
    let mut world = World::new();
    let unit_id = world.units.allocate_id();
    world.units.insert(unit_id, Unit::new(unit_id, 1));
    let mut script = TriggerScript::new(1).with_name("scalar");
    script.add_variable(variable(1, VarType::Unit, TriggerValue::Unit(unit_id)));
    script.add_variable(variable(2, VarType::Float, TriggerValue::Float(0.0)));
    script.add_variable(variable(
        3,
        VarType::DataScalar,
        TriggerValue::String("DamageTaken".to_owned()),
    ));
    let effect = effect(1, &[(1, 1), (5, 3), (6, 2)]);

    assert_eq!(
        modify_data_scalar(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!((world.get_unit(unit_id).unwrap().damage_taken_multiplier - 1.0).abs() < f32::EPSILON);
}
