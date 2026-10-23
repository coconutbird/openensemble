use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use crate::world::flat_test_terrain;

#[test]
fn random_location_uses_retail_radius_distribution_rotation_and_rng_stream() {
    let mut script = random_location_script();
    let effect = random_location_effect();
    let mut world = World::new();
    let mut oracle = World::new();
    let radial_sample = oracle.trigger_random_float(0.0, 1.0);
    let radius = radial_sample.sqrt() * 8.0 + 2.0;
    let theta = oracle.trigger_random_float(0.0, std::f32::consts::TAU);
    let expected = TriggerVec3::new(
        100.0 + crate::math::cos(theta) * radius,
        7.0,
        200.0 - crate::math::sin(theta) * radius,
    );

    assert_eq!(
        random_location(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(output(&script), expected);
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn random_location_snaps_to_the_authoritative_xsd_height_grid() {
    let mut script = random_location_script();
    set_float(&mut script, 2, 0.0);
    set_float(&mut script, 3, 0.0);
    let effect = random_location_effect();
    let mut world = World::new();
    world.configure_terrain_simulation(flat_test_terrain(100.0, 23.5));

    assert_eq!(
        random_location(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(output(&script), TriggerVec3::new(100.0, 23.5, 200.0));
}

#[test]
fn probe_modes_remain_visible_and_do_not_consume_rng() {
    let mut script = random_location_script();
    script.get_variable_mut(4).unwrap().value = TriggerValue::Bool(true);
    let effect = random_location_effect();
    let mut world = World::new();
    let mut oracle = World::new();

    assert_eq!(
        random_location(&effect, &mut script, &mut world),
        EffectOutcome::Unsupported(55)
    );
    assert_eq!(output(&script), TriggerVec3::zero());
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

fn random_location_script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Vector)
            .with_value(TriggerValue::Vector(TriggerVec3::new(100.0, 7.0, 200.0))),
    );
    script.add_variable(TriggerVar::new(2, VarType::Float).with_value(TriggerValue::Float(2.0)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(10.0)));
    script.add_variable(TriggerVar::new(4, VarType::Bool).with_value(TriggerValue::Bool(false)));
    script.add_variable(TriggerVar::new(5, VarType::Bool).with_value(TriggerValue::Bool(false)));
    script.add_variable(
        TriggerVar::new(6, VarType::Vector).with_value(TriggerValue::Vector(TriggerVec3::zero())),
    );
    script
}

fn random_location_effect() -> Effect {
    let mut effect = Effect::new(1, EffectType::RandomLocation)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(4, 3)
        .with_input_at(5, 4)
        .with_input_at(6, 5)
        .with_output_at(3, 6);
    effect.version = 4;
    effect
}

fn set_float(script: &mut TriggerScript, id: u32, value: f32) {
    script.get_variable_mut(id).unwrap().value = TriggerValue::Float(value);
}

fn output(script: &TriggerScript) -> TriggerVec3 {
    match script.get_variable(6).unwrap().value {
        TriggerValue::Vector(value) => value,
        ref value => panic!("expected vector output, found {value:?}"),
    }
}
