use super::*;
use crate::trigger::{TriggerVar, VarType};

fn script_with(values: &[(u32, VarType, TriggerValue)]) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    for (id, var_type, value) in values {
        script.add_variable(TriggerVar::new(*id, *var_type).with_value(value.clone()));
    }
    script
}

fn vector(x: f32, y: f32, z: f32) -> TriggerValue {
    TriggerValue::Vector(TriggerVec3::new(x, y, z))
}

fn four_slot(effect_type: EffectType) -> Effect {
    Effect::new(1, effect_type)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4)
}

fn output(script: &TriggerScript) -> TriggerValue {
    script.get_variable(4).unwrap().value.clone()
}

/// Inline case `0x126`: `(b - a) * t + a` with `t` clamped to 0 to 1.
#[test]
fn lerp_location_clamps_the_fraction() {
    let mut script = script_with(&[
        (1, VarType::Vector, vector(0.0, 0.0, 10.0)),
        (2, VarType::Vector, vector(10.0, 4.0, 20.0)),
        (3, VarType::Float, TriggerValue::Float(0.35)),
        (4, VarType::Vector, vector(0.0, 0.0, 0.0)),
    ]);
    let effect = four_slot(EffectType::LerpLocation);
    assert_eq!(lerp_location(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(
        output(&script),
        vector(10.0 * 0.35, 4.0 * 0.35, 10.0 * 0.35 + 10.0)
    );
    script.get_variable_mut(3).unwrap().value = TriggerValue::Float(2.0);
    assert_eq!(lerp_location(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(output(&script), vector(10.0, 4.0, 20.0));
    script.get_variable_mut(3).unwrap().value = TriggerValue::Float(-1.0);
    assert_eq!(lerp_location(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(output(&script), vector(0.0, 0.0, 10.0));
}

/// Inline case `0x127`: operator 0 adds, operator 1 subtracts, another operator writes nothing.
#[test]
fn math_location_adds_or_subtracts() {
    let mut script = script_with(&[
        (1, VarType::Vector, vector(1.0, 2.0, 3.0)),
        (2, VarType::MathOperator, TriggerValue::Int(0)),
        (3, VarType::Vector, vector(10.0, 0.0, -1.0)),
        (4, VarType::Vector, vector(0.0, 0.0, 0.0)),
    ]);
    let effect = four_slot(EffectType::MathLocation);
    assert_eq!(math_location(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(output(&script), vector(11.0, 2.0, 2.0));
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    assert_eq!(math_location(&effect, &mut script), EffectOutcome::Applied);
    assert_eq!(output(&script), vector(-9.0, 2.0, 4.0));
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(2);
    assert_eq!(math_location(&effect, &mut script), EffectOutcome::Skipped);
    assert_eq!(output(&script), vector(-9.0, 2.0, 4.0));
}

/// `BTriggerEffect_teLocationAdjustDir`: the effect normalizes the direction and pulls a result
/// outside the simulation bounds back to 0.01 inside the crossed edge.
#[test]
fn location_adjust_dir_moves_along_the_direction_inside_the_bounds() {
    let bounds = WorldBounds::from_corners(glam::Vec3::ZERO, glam::Vec3::new(100.0, 0.0, 100.0));
    let origin = TriggerVec3::new(50.0, 0.0, 50.0);
    let inside = adjust_dir(origin, TriggerVec3::new(0.0, 0.0, 2.0), 30.0, bounds);
    assert_eq!(inside, TriggerVec3::new(50.0, 0.0, 80.0));
    let clamped = adjust_dir(origin, TriggerVec3::new(3.0, 0.0, 0.0), 80.0, bounds);
    assert!((clamped.x - 99.99).abs() < 1.0e-4, "{clamped:?}");
    assert!((clamped.z - 50.0).abs() < 1.0e-6);
    let zero = adjust_dir(origin, TriggerVec3::new(0.0, 0.0, 0.0), 80.0, bounds);
    assert_eq!(zero, origin);

    let mut script = script_with(&[
        (1, VarType::Vector, TriggerValue::Vector(origin)),
        (2, VarType::Vector, vector(0.0, 0.0, -1.0)),
        (3, VarType::Float, TriggerValue::Float(10.0)),
        (4, VarType::Vector, vector(0.0, 0.0, 0.0)),
    ]);
    let world = World::new();
    let effect = four_slot(EffectType::LocationAdjustDir);
    assert_eq!(
        location_adjust_dir(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(output(&script), vector(50.0, 0.0, 40.0));
}
