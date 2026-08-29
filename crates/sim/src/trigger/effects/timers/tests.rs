use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn create_effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::CreateTimer);
    effect.version = version;
    effect
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4)
        .with_input_at(6, 6)
        .with_input_at(7, 7)
        .with_input_at(8, 8)
}

fn timer_script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Bool, TriggerValue::Bool(false));
    add(&mut script, 2, VarType::Time, TriggerValue::Time(60_000));
    add(&mut script, 3, VarType::Time, TriggerValue::Time(0));
    add(&mut script, 4, VarType::Integer, TriggerValue::Int(-1));
    add(
        &mut script,
        6,
        VarType::LocStringID,
        TriggerValue::Int(24_222),
    );
    add(&mut script, 7, VarType::Player, TriggerValue::Player(2));
    add(
        &mut script,
        8,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1, 2]),
    );
    script
}

#[test]
fn retail_timer_ids_are_typed() {
    assert_eq!(EffectType::from_u16(658), Some(EffectType::CreateTimer));
    assert_eq!(EffectType::from_u16(659), Some(EffectType::DestroyTimer));
}

#[test]
fn version_five_creates_countdown_and_unions_ui_players() {
    let mut world = World::new();
    let mut script = timer_script();
    assert_eq!(
        create(&create_effect(5), &mut script, &mut world),
        EffectOutcome::Applied
    );

    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Int(0));
    let timer = world.game_timer(0).unwrap();
    assert!(!timer.count_up());
    assert_eq!(timer.start_time_ms(), 60_000);
    assert_eq!(timer.stop_time_ms(), 0);
    assert_eq!(timer.label_string_id(), Some(24_222));
    assert_eq!(timer.audience(), &GameTimerAudience::Players(vec![1, 2]));
}

#[test]
fn version_four_uses_primary_user_and_destroy_is_an_applied_noop_when_missing() {
    let mut world = World::new();
    let mut script = timer_script();
    assert_eq!(
        create(&create_effect(4), &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(
        world.game_timer(0).unwrap().audience(),
        &GameTimerAudience::PrimaryUser
    );

    script.get_variable_mut(4).unwrap().value = TriggerValue::Int(0);
    let destroy_effect = Effect::new(2, EffectType::DestroyTimer).with_input_at(1, 4);
    assert_eq!(
        destroy(&destroy_effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!(world.game_timer(0).is_none());
    assert_eq!(
        destroy(&destroy_effect, &script, &mut world),
        EffectOutcome::Applied
    );
}

#[test]
fn unsupported_create_version_does_not_mutate_state_or_output() {
    let mut world = World::new();
    let mut script = timer_script();
    assert_eq!(
        create(&create_effect(3), &mut script, &mut world),
        EffectOutcome::Unsupported(658)
    );
    assert!(world.game_timers().next().is_none());
    assert_eq!(script.get_variable(4).unwrap().value, TriggerValue::Int(-1));
}
