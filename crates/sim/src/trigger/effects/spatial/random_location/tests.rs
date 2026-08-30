use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use byteorder::{BigEndian, ByteOrder};
use half::f16;

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
        100.0 + theta.cos() * radius,
        7.0,
        200.0 - theta.sin() * radius,
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
    world.configure_terrain_simulation(&flat_xsd(23.5)).unwrap();

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

fn flat_xsd(height: f32) -> Vec<u8> {
    let mut header = vec![0_u8; 32];
    BigEndian::write_i32(&mut header[0..4], 4);
    BigEndian::write_i32(&mut header[4..8], 8);
    BigEndian::write_f32(&mut header[8..12], 1.0);
    BigEndian::write_f32(&mut header[12..16], 1.0);
    BigEndian::write_i32(&mut header[16..20], 8);
    BigEndian::write_i32(&mut header[20..24], 8);
    BigEndian::write_f32(&mut header[24..28], 100.0);
    BigEndian::write_i32(&mut header[28..32], 1);

    let mut heights = vec![0_u8; 8 * 8 * 2];
    for sample in heights.as_chunks_mut::<2>().0 {
        BigEndian::write_u16(sample, f16::from_f32(height).to_bits());
    }
    let mut writer = ecf::Writer::new(0);
    writer.add_chunk(0x1111, header);
    writer.add_chunk(0x2222, heights);
    writer.finalize().unwrap()
}
