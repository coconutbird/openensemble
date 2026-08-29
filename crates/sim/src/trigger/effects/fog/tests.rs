use super::*;
use crate::trigger::{EffectType, TriggerValue, TriggerVar, VarType};

fn effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::EnableFogOfWar).with_input_at(1, 1);
    effect.version = version;
    effect
}

fn script(enabled: bool) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Bool).with_value(TriggerValue::Bool(enabled)));
    script
}

#[test]
fn version_one_sets_authoritative_world_fog_state() {
    let mut world = World::new();

    assert_eq!(
        set_enabled(&effect(1), &script(false), &mut world),
        EffectOutcome::Applied
    );
    assert!(!world.fog_of_war_enabled());
    assert_eq!(
        set_enabled(&effect(1), &script(true), &mut world),
        EffectOutcome::Applied
    );
    assert!(world.fog_of_war_enabled());
}

#[test]
fn invalid_binding_skips_and_unknown_version_stays_visible() {
    let mut world = World::new();
    let empty_script = TriggerScript::new(1);

    assert_eq!(
        set_enabled(&effect(1), &empty_script, &mut world),
        EffectOutcome::Skipped
    );
    assert!(world.fog_of_war_enabled());
    assert_eq!(
        set_enabled(&effect(2), &script(false), &mut world),
        EffectOutcome::Unsupported(EffectType::EnableFogOfWar as u16)
    );
}

#[test]
fn whole_map_effects_mutate_only_supported_retail_versions() {
    let mut clear = Effect::new(2, EffectType::ClearBlackMap);
    clear.version = 1;
    let mut reset = Effect::new(3, EffectType::ResetBlackMap);
    reset.version = 1;
    let mut world = World::new();

    assert_eq!(black_map(&clear, &mut world), EffectOutcome::Applied);
    assert!(world.black_map_is_cleared());
    assert_eq!(black_map(&reset, &mut world), EffectOutcome::Applied);
    assert!(!world.black_map_is_cleared());

    clear.version = 2;
    assert_eq!(
        black_map(&clear, &mut world),
        EffectOutcome::Unsupported(EffectType::ClearBlackMap as u16)
    );
    assert!(!world.black_map_is_cleared());
}
