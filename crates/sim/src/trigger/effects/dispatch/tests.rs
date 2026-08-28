use super::*;

#[test]
fn retail_debug_effects_are_supported_presentation_work() {
    let mut script = TriggerScript::new(1);
    let mut world = World::new();
    let debug_effects = [
        (207, EffectType::DebugVarTime),
        (209, EffectType::DebugVarCount),
        (218, EffectType::DebugVarFloat),
        (221, EffectType::DebugVarPlayerList),
    ];

    for (raw_type, effect_type) in debug_effects {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
        assert_eq!(
            execute_effect(
                &Effect::new(i32::from(raw_type), effect_type),
                &mut script,
                &mut world,
                None,
                None,
            ),
            (EffectOutcome::Presentation, None)
        );
    }
}
