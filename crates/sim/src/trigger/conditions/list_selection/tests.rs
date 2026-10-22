use super::*;
use crate::trigger::{ConditionType, TriggerVar, TriggerVec3, VarType};

#[test]
fn first_and_last_do_not_skip_a_stale_selected_entity() {
    let mut world = World::new();
    world.init_players(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let mut script = selection_script(
        VarType::UnitList,
        TriggerValue::UnitList(vec![first, second]),
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    let condition = selection_condition(ConditionType::CanGetOneUnit);

    assert!(can_get_one_unit(&condition, &mut script, &mut world));
    assert_eq!(output_entity(&script), first);

    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    assert!(can_get_one_unit(&condition, &mut script, &mut world));
    assert_eq!(output_entity(&script), second);

    assert!(world.remove_unit(first).is_some());
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(0);
    assert!(!can_get_one_unit(&condition, &mut script, &mut world));
    assert_eq!(output_entity(&script), EntityId::INVALID);
}

#[test]
fn squad_selection_writes_the_requested_live_list_member() {
    let mut world = World::new();
    world.init_players(1);
    let first = world.create_squad(1);
    let second = world.create_squad(1);
    let mut script = selection_script(
        VarType::SquadList,
        TriggerValue::SquadList(vec![first, second]),
        VarType::Squad,
        TriggerValue::Squad(EntityId::INVALID),
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);

    assert!(can_get_one_squad(
        &selection_condition(ConditionType::CanGetOneSquad),
        &mut script,
        &mut world,
    ));
    assert_eq!(output_entity(&script), second);
}

#[test]
fn random_selection_consumes_the_synchronized_retail_rng() {
    let seed = 0x1357_2468;
    let mut world = World::with_seed(seed);
    world.init_players(1);
    let values = vec![
        world.create_unit(1),
        world.create_unit(1),
        world.create_unit(1),
    ];
    let mut oracle = World::with_seed(seed);
    let expected = values[oracle.trigger_random_index(2) as usize];
    let mut script = selection_script(
        VarType::UnitList,
        TriggerValue::UnitList(values),
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(2);

    assert!(can_get_one_unit(
        &selection_condition(ConditionType::CanGetOneUnit),
        &mut script,
        &mut world,
    ));
    assert_eq!(output_entity(&script), expected);
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn prototype_and_location_selection_preserve_typed_list_values() {
    let mut world = World::new();
    let mut prototypes = selection_script(
        VarType::ProtoSquadList,
        TriggerValue::ProtoSquadList(vec![11, 22]),
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(-1),
    );
    prototypes.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    assert!(can_get_one_proto_squad(
        &selection_condition(ConditionType::CanGetOneProtoSquad),
        &mut prototypes,
        &mut world,
    ));
    assert_eq!(
        prototypes.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::ProtoSquad(22)),
    );

    let location = TriggerVec3::new(10.0, 20.0, 30.0);
    let mut locations = selection_script(
        VarType::VectorList,
        TriggerValue::VectorList(vec![location]),
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    assert!(can_get_one_location(
        &selection_condition(ConditionType::CanGetOneLocation),
        &mut locations,
        &mut world,
    ));
    assert_eq!(
        locations.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::Vector(location)),
    );
}

#[test]
fn scalar_list_families_write_their_retail_output_types() {
    let mut world = World::new();
    let mut players = selection_script(
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![2, 1]),
        VarType::Player,
        TriggerValue::Player(-1),
    );
    assert!(can_get_one_player(
        &selection_condition(ConditionType::CanGetOnePlayer),
        &mut players,
        &mut world,
    ));
    assert_eq!(
        players.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::Player(2)),
    );

    let mut integers = selection_script(
        VarType::IntegerList,
        TriggerValue::IntegerList(vec![10, 20]),
        VarType::Integer,
        TriggerValue::Int(-1),
    );
    integers.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    assert!(can_get_one_integer(
        &selection_condition(ConditionType::CanGetOneInteger),
        &mut integers,
        &mut world,
    ));
    assert_eq!(
        integers.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::Int(20)),
    );
}

#[test]
fn proto_object_selection_uses_its_retail_slot_four_output() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::ProtoObjectList)
            .with_value(TriggerValue::ProtoObjectList(vec![17])),
    );
    script.add_variable(TriggerVar::new(2, VarType::ListPosition).with_value(TriggerValue::Int(0)));
    script.add_variable(
        TriggerVar::new(4, VarType::ProtoObject).with_value(TriggerValue::ProtoObject(-1)),
    );
    let condition = Condition::new(1, ConditionType::CanGetOneProtoObject)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(4, 4);

    assert!(can_get_one_proto_object(
        &condition,
        &mut script,
        &mut world,
    ));
    assert_eq!(
        script.get_variable(4).map(|variable| &variable.value),
        Some(&TriggerValue::ProtoObject(17)),
    );
}

#[test]
fn design_line_selection_preserves_typed_identity_and_empty_output() {
    let mut world = World::new();
    let mut script = selection_script(
        VarType::DesignLineList,
        TriggerValue::DesignLineList(vec![41, 72]),
        VarType::DesignLine,
        TriggerValue::DesignLine(-1),
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    let condition = selection_condition(ConditionType::CanGetOneDesignLine);

    assert!(can_get_one_design_line(&condition, &mut script, &mut world));
    assert_eq!(
        script.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::DesignLine(72))
    );

    script.get_variable_mut(1).unwrap().value = TriggerValue::DesignLineList(Vec::new());
    assert!(!can_get_one_design_line(
        &condition,
        &mut script,
        &mut world
    ));
    assert_eq!(
        script.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::DesignLine(72))
    );
}

/// `BTriggerCondition_tcCanRemoveOneLocation`: `First` and `Last` keep the order of the other
/// values, and an empty list gives false.
#[test]
fn can_remove_one_location_takes_first_then_last() {
    let mut world = World::new();
    let points = [1.0, 2.0, 3.0].map(|x| TriggerVec3::new(x, 0.0, 0.0));
    let mut script = selection_script(
        VarType::VectorList,
        TriggerValue::VectorList(points.to_vec()),
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::default()),
    );
    let condition = selection_condition(ConditionType::CanRemoveOneLocation);

    assert!(can_remove_one_location(&condition, &mut script, &mut world));
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Vector(points[0])
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(1);
    assert!(can_remove_one_location(&condition, &mut script, &mut world));
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Vector(points[2])
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::VectorList(vec![points[1]])
    );
    assert!(can_remove_one_location(&condition, &mut script, &mut world));
    assert!(!can_remove_one_location(
        &condition,
        &mut script,
        &mut world
    ));
}

/// `BTriggerCondition_tcCanRemoveOneFloat`: `Random` takes an index from the trigger random
/// stream and moves the last value into the free slot. An unknown position gives false.
#[test]
fn can_remove_one_float_random_swaps_in_the_last_value() {
    let mut world = World::with_seed(7);
    let mut expected_world = World::with_seed(7);
    let index = expected_world.trigger_random_index(3) as usize;
    let mut expected = vec![10.0, 20.0, 30.0, 40.0];
    let removed = expected.swap_remove(index);

    let mut script = selection_script(
        VarType::FloatList,
        TriggerValue::FloatList(vec![10.0, 20.0, 30.0, 40.0]),
        VarType::Float,
        TriggerValue::Float(0.0),
    );
    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(2);
    let condition = selection_condition(ConditionType::CanRemoveOneFloat);
    assert!(can_remove_one_float(&condition, &mut script, &mut world));
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Float(removed)
    );
    assert_eq!(
        script.get_variable(1).unwrap().value,
        TriggerValue::FloatList(expected)
    );

    script.get_variable_mut(2).unwrap().value = TriggerValue::Int(3);
    assert!(!can_remove_one_float(&condition, &mut script, &mut world));
}

fn selection_script(
    list_type: VarType,
    list: TriggerValue,
    output_type: VarType,
    output: TriggerValue,
) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, list_type).with_value(list));
    script.add_variable(TriggerVar::new(2, VarType::ListPosition).with_value(TriggerValue::Int(0)));
    script.add_variable(TriggerVar::new(3, output_type).with_value(output));
    script
}

fn selection_condition(condition_type: ConditionType) -> Condition {
    Condition::new(1, condition_type)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3)
}

fn output_entity(script: &TriggerScript) -> EntityId {
    script
        .get_variable(3)
        .and_then(|variable| variable.value.as_entity())
        .expect("entity output")
}
