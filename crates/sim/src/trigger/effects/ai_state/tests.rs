use super::*;
use crate::trigger::{TriggerVar, TriggerVec3, VarType};

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn focus_effect() -> Effect {
    let mut effect = Effect::new(1, EffectType::AISetFocus)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    effect.version = 2;
    effect
}

/// `BTriggerEffect_teAISetFocusV2`: a non-null location wins, then a live squad, then a live
/// unit. Without a source the look-at position does not change.
#[test]
fn ai_set_focus_v2_prefers_location_then_squad_then_unit() {
    let mut world = World::new();
    world.init_players(2);
    let squad_id = world.create_squad_at(2, Vec3::new(5.0, 0.0, 6.0));
    let unit_id = world.create_unit(2);
    world.get_unit_mut(unit_id).unwrap().base.position = Vec3::new(7.0, 1.0, 8.0);
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Player, TriggerValue::Player(2));
    add(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(1.0, 2.0, 3.0)),
    );
    add(&mut script, 3, VarType::Unit, TriggerValue::Unit(unit_id));
    add(
        &mut script,
        4,
        VarType::Squad,
        TriggerValue::Squad(squad_id),
    );
    let focus = |world: &World| world.get_player(2).unwrap().look_at_position;

    assert_eq!(
        set_focus(&focus_effect(), &script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(focus(&world), Vec3::new(1.0, 2.0, 3.0));

    script.add_variable(TriggerVar::new(2, VarType::Vector).null());
    assert_eq!(
        set_focus(&focus_effect(), &script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(focus(&world), Vec3::new(5.0, 0.0, 6.0));

    assert!(world.remove_squad(squad_id).is_some());
    assert_eq!(
        set_focus(&focus_effect(), &script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(focus(&world), Vec3::new(7.0, 1.0, 8.0));

    assert!(world.remove_unit(unit_id).is_some());
    assert_eq!(
        set_focus(&focus_effect(), &script, &mut world),
        EffectOutcome::Skipped
    );
    assert_eq!(focus(&world), Vec3::new(7.0, 1.0, 8.0));

    let mut version_one = focus_effect();
    version_one.version = 1;
    assert_eq!(
        set_focus(&version_one, &script, &mut world),
        EffectOutcome::Unsupported(711)
    );
}

/// The default case of `BTriggerEffect_fire` and `BTriggerEffect_teAIBindLog` change no
/// simulation state.
#[test]
fn retail_default_case_and_ai_log_effects_are_supported_no_ops() {
    let script = TriggerScript::new(1);
    let mut world = World::new();
    let checksum = world.checksum();
    for raw_type in RETAIL_DEFAULT_CASE_EFFECTS {
        let mut effect = Effect::new(1, EffectType::Custom);
        effect.raw_type = raw_type;
        assert_eq!(
            execute(&effect, &script, &mut world),
            Some(EffectOutcome::Presentation)
        );
    }
    let bind_log = Effect::new(1, EffectType::AIBindLog);
    assert_eq!(
        execute(&bind_log, &script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    let mut unknown = Effect::new(1, EffectType::Custom);
    unknown.raw_type = 209;
    assert_eq!(execute(&unknown, &script, &mut world), None);
    assert_eq!(world.checksum(), checksum);
}
