use super::*;
use crate::trigger::{ConditionType, TriggerVar, VarType};
use crate::world::GameTimerAudience;

fn script(timer_id: i32) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script
        .add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(timer_id)));
    script.add_variable(TriggerVar::new(2, VarType::Time).with_value(TriggerValue::Time(u32::MAX)));
    script
}

fn condition() -> Condition {
    Condition::new(1, ConditionType::IsTimerDone)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
}

#[test]
fn timer_condition_reports_current_time_and_completion() {
    assert_eq!(
        ConditionType::from_u16(661),
        Some(ConditionType::IsTimerDone)
    );
    let mut world = World::new();
    let timer_id = world.create_game_timer(false, 100, 0, None, GameTimerAudience::PrimaryUser);
    let mut script = script(timer_id);

    assert!(!is_done(&condition(), &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Time(100)
    );
    world.advance_time(100);
    assert!(is_done(&condition(), &mut script, &world));
    assert_eq!(script.get_variable(2).unwrap().value, TriggerValue::Time(0));
}

#[test]
fn missing_timer_writes_zero_and_returns_false() {
    let world = World::new();
    let mut script = script(77);
    assert!(!is_done(&condition(), &mut script, &world));
    assert_eq!(script.get_variable(2).unwrap().value, TriggerValue::Time(0));
}
