use super::*;
use crate::trigger::{ConditionType, TriggerVar, VarType};
use crate::world::ObjectiveState;

fn script(objective: i32) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Objective).with_value(TriggerValue::Objective(objective)),
    );
    script
}

fn condition() -> Condition {
    Condition::new(1, ConditionType::IsObjectiveComplete).with_input_at(1, 1)
}

/// Two objectives with sparse IDs 7 and 3, in this order. Objective 3 is complete.
fn world() -> World {
    let mut world = World::new();
    world.configure_objectives(vec![ObjectiveState::new(7), ObjectiveState::new(3)]);
    assert!(world.set_objective_completed(3, true));
    world
}

#[test]
fn objective_input_is_a_list_position_like_retail() {
    assert_eq!(
        ConditionType::from_u16(746),
        Some(ConditionType::IsObjectiveComplete)
    );
    let world = world();
    // Position 1 is objective 3, which is complete.
    assert!(is_objective_complete(&condition(), &script(1), &world));
    // Position 0 is objective 7, which is not complete.
    assert!(!is_objective_complete(&condition(), &script(0), &world));
    // ID 3 used as a position is outside the list.
    assert!(!is_objective_complete(&condition(), &script(3), &world));
    assert!(!is_objective_complete(&condition(), &script(-1), &world));
}

#[test]
fn missing_input_is_false() {
    let world = world();
    let empty = TriggerScript::new(1);
    assert!(!is_objective_complete(&condition(), &empty, &world));
}
