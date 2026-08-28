use super::*;
use crate::entity_id::EntityClass;
use crate::trigger::{BuildingCommandState, ConditionType, TriggerVar, VarType};
use crate::world::CustomCommand;

#[test]
fn building_command_done_filters_stale_squads_and_writes_retail_outputs() {
    let mut world = World::new();
    let live = world.create_squad(1);
    let expired_squad_id = EntityId::new(EntityClass::Squad, 200);
    let mut state = BuildingCommandState::default();
    state.record_trained_squad(live);
    state.record_trained_squad(expired_squad_id);
    state.finish();

    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::BuildingCommandState)
            .with_value(TriggerValue::BuildingCommandState(state)),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::Squad).with_value(TriggerValue::Squad(expired_squad_id)),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::SquadList)
            .with_value(TriggerValue::SquadList(vec![expired_squad_id])),
    );
    let condition = Condition::new(1, ConditionType::BuildingCommandDone)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3);

    assert!(building_command_done(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(live)
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::SquadList(vec![live])
    );
}

#[test]
fn building_command_waiting_clears_optional_outputs() {
    let world = World::new();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::BuildingCommandState).with_value(
            TriggerValue::BuildingCommandState(BuildingCommandState::default()),
        ),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::Squad)
            .with_value(TriggerValue::Squad(EntityId::new(EntityClass::Squad, 5))),
    );
    script.add_variable(TriggerVar::new(3, VarType::SquadList).with_value(
        TriggerValue::SquadList(vec![EntityId::new(EntityClass::Squad, 5)]),
    ));
    let condition = Condition::new(1, ConditionType::BuildingCommandDone)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3);

    assert!(!building_command_done(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Squad(EntityId::INVALID)
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::SquadList(Vec::new())
    );
}

#[test]
fn custom_command_check_distinguishes_pending_consumed_and_invalid_ids() {
    let mut world = World::new();
    let id = world.add_custom_command(CustomCommand::default());
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(id)));
    script.add_variable(TriggerVar::new(2, VarType::Integer).with_value(TriggerValue::Int(99)));
    let condition = Condition::new(1, ConditionType::CustomCommandCheck)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    assert!(!custom_command_check(&condition, &mut script, &world));
    assert_eq!(script.get_variable(2).unwrap().value, TriggerValue::Int(0));

    world.remove_custom_command(id);
    assert!(custom_command_check(&condition, &mut script, &world));
    assert_eq!(script.get_variable(2).unwrap().value, TriggerValue::Int(1));

    script.get_variable_mut(1).unwrap().value = TriggerValue::Int(id + 10);
    assert!(!custom_command_check(&condition, &mut script, &world));
    assert_eq!(script.get_variable(2).unwrap().value, TriggerValue::Int(0));
}
