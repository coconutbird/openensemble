use super::*;
use crate::entity_id::EntityClass;
use crate::trigger::{ConditionType, TriggerVar, VarType};

#[test]
fn socket_lists_preserve_association_order_and_can_filter_plugged_sockets() {
    let (mut world, parent_id, [building_0, turret_0, building_1]) = socket_world();
    let plug_id = world.create_building(1);
    assert!(world.connect_socket_plug(building_0, plug_id));
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(parent_id));
    add_value(
        &mut script,
        2,
        VarType::UnitList,
        TriggerValue::UnitList(Vec::new()),
    );
    add_value(
        &mut script,
        3,
        VarType::UnitList,
        TriggerValue::UnitList(Vec::new()),
    );
    add_value(&mut script, 4, VarType::Bool, TriggerValue::Bool(true));
    let mut condition = Condition::new(1, ConditionType::CanGetSocketUnits)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3)
        .with_input_at(4, 4);
    condition.version = 2;

    assert!(can_get_socket_units(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::UnitList(vec![building_1])
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::UnitList(vec![turret_0])
    );
}

#[test]
fn one_socket_uses_independent_building_and_turret_indexes() {
    let (world, parent_id, [_building_0, turret_0, building_1]) = socket_world();
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(parent_id));
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(1));
    add_value(&mut script, 3, VarType::Integer, TriggerValue::Int(0));
    add_value(
        &mut script,
        4,
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    add_value(
        &mut script,
        5,
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    let mut condition = Condition::new(1, ConditionType::CanGetOneSocketUnit)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4)
        .with_output_at(5, 5);
    condition.version = 1;

    assert!(can_get_one_socket_unit(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Unit(building_1)
    );
    assert_eq!(
        script.get_variable(5).unwrap().value,
        TriggerValue::Unit(turret_0)
    );
}

#[test]
fn one_socket_writes_invalid_outputs_for_a_stale_source() {
    let world = World::new();
    let stale = EntityId::new(EntityClass::Unit, 4_000);
    let sentinel = EntityId::new(EntityClass::Unit, 99);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(stale));
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(0));
    add_value(&mut script, 3, VarType::Integer, TriggerValue::Int(0));
    add_value(&mut script, 4, VarType::Unit, TriggerValue::Unit(sentinel));
    add_value(&mut script, 5, VarType::Unit, TriggerValue::Unit(sentinel));
    let mut condition = Condition::new(1, ConditionType::CanGetOneSocketUnit)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4)
        .with_output_at(5, 5);
    condition.version = 1;

    assert!(!can_get_one_socket_unit(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Unit(EntityId::INVALID)
    );
    assert_eq!(
        script.get_variable(5).unwrap().value,
        TriggerValue::Unit(EntityId::INVALID)
    );
}

#[test]
fn socket_parent_empty_and_plug_conditions_share_authoritative_relationships() {
    let (mut world, parent_id, [building_socket, _, _]) = socket_world();
    let plug_id = world.create_building(1);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Unit,
        TriggerValue::Unit(building_socket),
    );
    add_value(
        &mut script,
        2,
        VarType::Unit,
        TriggerValue::Unit(EntityId::INVALID),
    );
    let parent_condition = Condition::new(1, ConditionType::CanGetSocketParentBuilding)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    let empty_condition = Condition::new(2, ConditionType::IsEmptySocketUnit).with_input_at(1, 1);
    let plug_condition = Condition::new(3, ConditionType::CanGetSocketPlugUnit)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    assert!(can_get_socket_parent_building(
        &parent_condition,
        &mut script,
        &world
    ));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Unit(parent_id)
    );
    assert!(is_empty_socket_unit(&empty_condition, &script, &world));
    assert!(!can_get_socket_plug_unit(
        &plug_condition,
        &mut script,
        &world
    ));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Unit(EntityId::INVALID)
    );

    assert!(world.connect_socket_plug(building_socket, plug_id));
    assert!(!is_empty_socket_unit(&empty_condition, &script, &world));
    assert!(can_get_socket_plug_unit(
        &plug_condition,
        &mut script,
        &world
    ));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Unit(plug_id)
    );
}

fn socket_world() -> (World, EntityId, [EntityId; 3]) {
    let mut world = World::new();
    let parent_id = world.create_building(1);
    let building_0 = typed_socket(&mut world, "BuildingSocket");
    let turret_0 = typed_socket(&mut world, "TurretSocket");
    let building_1 = typed_socket(&mut world, "BuildingSocket");
    for socket_id in [building_0, turret_0, building_1] {
        assert!(world.associate_socket(parent_id, socket_id));
    }
    (world, parent_id, [building_0, turret_0, building_1])
}

fn typed_socket(world: &mut World, object_type: &str) -> EntityId {
    let socket_id = world.create_building(1);
    let socket = world.get_unit_mut(socket_id).unwrap();
    socket.proto_object_name = format!("test_{object_type}");
    socket.object_types = vec![object_type.to_owned()];
    socket_id
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
