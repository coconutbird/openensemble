use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};

#[test]
fn set_playable_bounds_uses_retail_corner_slots_and_world_clamping() {
    let mut world = World::new();
    assert!(world.configure_terrain_bounds(Vec3::ZERO, Vec3::new(100.0, 5.0, 80.0)));
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(120.0, 99.0, 70.0)),
    );
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(-20.0, -99.0, 10.0)),
    );
    let mut effect = Effect::new(1, EffectType::SetPlayableBounds)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    effect.version = 1;

    assert_eq!(
        set_playable_bounds(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    let bounds = world.playable_bounds().expect("scenario subset");
    assert_close(bounds.min_x(), 0.0);
    assert_close(bounds.min_z(), 10.0);
    assert_close(bounds.max_x(), 100.0);
    assert_close(bounds.max_z(), 70.0);
}

#[test]
fn mean_location_versions_use_retail_authored_and_valid_divisors() {
    let mut world = World::new();
    let unit_id = world.create_unit_at(1, Vec3::new(8.0, 4.0, 2.0));
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::UnitList,
        TriggerValue::UnitList(vec![unit_id, EntityId::INVALID]),
    );
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    let mut effect = Effect::new(1, EffectType::GetMeanLocation)
        .with_input_at(1, 1)
        .with_output_at(4, 2);

    effect.version = 1;
    assert_eq!(
        get_mean_location(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(vector(&script, 2), TriggerVec3::new(4.0, 2.0, 1.0));

    effect.version = 2;
    assert_eq!(
        get_mean_location(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(vector(&script, 2), TriggerVec3::new(8.0, 4.0, 2.0));
}

#[test]
fn mean_location_uses_squad_child_average_and_input_priority() {
    let mut world = World::new();
    let squad_id = world.create_squad_at(1, Vec3::new(100.0, 0.0, 0.0));
    let first = world.create_unit_at(1, Vec3::new(2.0, 0.0, 0.0));
    let second = world.create_unit_at(1, Vec3::new(6.0, 0.0, 0.0));
    assert!(world.attach_unit_to_squad(first, squad_id));
    assert!(world.attach_unit_to_squad(second, squad_id));

    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::SquadList,
        TriggerValue::SquadList(vec![squad_id]),
    );
    add_value(
        &mut script,
        2,
        VarType::VectorList,
        TriggerValue::VectorList(vec![
            TriggerVec3::new(20.0, 0.0, 0.0),
            TriggerVec3::new(40.0, 0.0, 0.0),
        ]),
    );
    add_value(
        &mut script,
        3,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    let mut effect = Effect::new(1, EffectType::GetMeanLocation)
        .with_input_at(2, 1)
        .with_input_at(5, 2)
        .with_output_at(4, 3);
    effect.version = 2;

    assert_eq!(
        get_mean_location(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(vector(&script, 3), TriggerVec3::new(4.0, 0.0, 0.0));

    script.get_variable_mut(1).unwrap().is_null = true;
    assert_eq!(
        get_mean_location(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(vector(&script, 3), TriggerVec3::new(30.0, 0.0, 0.0));
}

#[test]
fn mean_location_v2_reproduces_nan_for_nonempty_all_invalid_entity_list() {
    let world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::ObjectList,
        TriggerValue::ObjectList(vec![EntityId::INVALID]),
    );
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    let mut effect = Effect::new(1, EffectType::GetMeanLocation)
        .with_input_at(3, 1)
        .with_output_at(4, 2);
    effect.version = 2;

    assert_eq!(
        get_mean_location(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    let result = vector(&script, 2);
    assert!(result.x.is_nan() && result.y.is_nan() && result.z.is_nan());
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn vector(script: &TriggerScript, id: u32) -> TriggerVec3 {
    match script.get_variable(id).expect("vector").value {
        TriggerValue::Vector(value) => value,
        ref value => panic!("expected vector, got {value:?}"),
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < f32::EPSILON);
}
