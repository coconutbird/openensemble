use super::*;
use crate::trigger::{TriggerVar, VarType};

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::PlayAnimationObject)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(6, 6);
    effect.version = version;
    effect
}

#[test]
fn version_three_uses_explicit_duration_and_writes_output() {
    let mut world = World::new();
    let entity_id = world.create_unit(1);
    world.get_unit_mut(entity_id).unwrap().proto_object_name = "bridge".to_owned();
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::Object,
        TriggerValue::Object(entity_id),
    );
    add(
        &mut script,
        2,
        VarType::AnimType,
        TriggerValue::String("Death".to_owned()),
    );
    add(&mut script, 7, VarType::Time, TriggerValue::Time(1_250));
    add(&mut script, 6, VarType::Time, TriggerValue::Time(0));
    let effect = effect(3).with_input_at(7, 7);

    assert_eq!(
        execute(&effect, &mut script, &mut world, None),
        Some(EffectOutcome::Applied)
    );
    let animation = world.entity_scripted_animation(entity_id).unwrap();
    assert_eq!(animation.animation_type(), "Death");
    assert_eq!(animation.duration_ms(), 1_250);
    assert_eq!(
        script.get_variable(6).unwrap().value,
        TriggerValue::Time(1_250)
    );
}

#[test]
fn version_two_uses_scenario_layered_clip_duration_and_asset() {
    let mut world = World::new();
    let entity_id = world.create_unit(1);
    world.get_unit_mut(entity_id).unwrap().proto_object_name = "bridge".to_owned();
    let mut gameplay = GameplayCatalog::default();
    gameplay.insert_test_scripted_animation_clip(
        "bridge",
        "Research",
        "art\\bridge_research.uax",
        2_500,
    );
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::Object,
        TriggerValue::Object(entity_id),
    );
    add(
        &mut script,
        2,
        VarType::AnimType,
        TriggerValue::String("Research".to_owned()),
    );
    add(&mut script, 6, VarType::Time, TriggerValue::Time(0));

    assert_eq!(
        execute(&effect(2), &mut script, &mut world, Some(&gameplay)),
        Some(EffectOutcome::Applied)
    );
    let animation = world.entity_scripted_animation(entity_id).unwrap();
    assert_eq!(animation.asset_path(), Some("art\\bridge_research.uax"));
    assert_eq!(animation.duration_ms(), 2_500);
}

#[test]
fn missing_object_still_writes_retails_zero_duration() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::Object,
        TriggerValue::Object(crate::EntityId::INVALID),
    );
    add(
        &mut script,
        2,
        VarType::AnimType,
        TriggerValue::String("Death".to_owned()),
    );
    add(&mut script, 6, VarType::Time, TriggerValue::Time(99));

    assert_eq!(
        execute(&effect(3), &mut script, &mut world, None),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(script.get_variable(6).unwrap().value, TriggerValue::Time(0));
}
