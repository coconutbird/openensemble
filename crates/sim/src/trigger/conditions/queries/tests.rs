use super::*;
use crate::entities::UnitState;
use crate::trigger::{ConditionType, TriggerVar, VarType};

#[test]
fn unit_versions_preserve_empty_player_filter_and_output_semantics() {
    let mut world = World::new();
    world.init_players(2);
    let player_two = world.create_unit(2);
    let player_one = world.create_unit(1);
    let dead = world.create_unit(1);
    world.get_unit_mut(dead).unwrap().state = UnitState::Dead;
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(-1));
    add_value(
        &mut script,
        3,
        VarType::UnitList,
        TriggerValue::UnitList(Vec::new()),
    );
    let mut condition = unit_query_condition(5);

    assert!(can_get_units(&condition, &mut script, &world));
    assert_eq!(integer(&script, 2), 2);
    assert_eq!(entities(&script, 3), &[player_two, player_one]);

    condition.version = 6;
    assert!(!can_get_units(&condition, &mut script, &world));
    assert_eq!(integer(&script, 2), 0);
    assert!(entities(&script, 3).is_empty());

    script.get_variable_mut(1).unwrap().value = TriggerValue::PlayerList(vec![1]);
    assert!(can_get_units(&condition, &mut script, &world));
    assert_eq!(entities(&script, 3), &[player_one]);
}

#[test]
fn unit_query_applies_sphere_box_type_and_unordered_list_filters() {
    let mut world = World::new();
    world.init_players(1);
    let included = world.create_unit_at(1, Vec3::new(2.0, 1.0, 0.0));
    let filtered = world.create_unit_at(1, Vec3::new(0.0, 1.0, 0.0));
    let outside = world.create_unit_at(1, Vec3::new(0.0, 1.0, 3.0));
    for unit_id in [included, filtered, outside] {
        world.get_unit_mut(unit_id).unwrap().object_types = vec!["Infantry".to_owned()];
    }
    let mut script = spatial_unit_script(included);
    let mut sphere = unit_query_condition(6)
        .with_input_at(1, 4)
        .with_input_at(2, 5)
        .with_input_at(4, 6)
        .with_input_at(9, 7);

    assert!(can_get_units(&sphere, &mut script, &world));
    assert_eq!(entities(&script, 3), &[included]);

    sphere.inputs.retain(|binding| binding.signature_id != 2);
    sphere = sphere
        .with_input_at(11, 8)
        .with_input_at(13, 9)
        .with_input_at(14, 10)
        .with_input_at(15, 11);
    assert!(can_get_units(&sphere, &mut script, &world));
    assert_eq!(entities(&script, 3), &[included]);
}

#[test]
fn squad_versions_preserve_player_order_and_ignore_flying_leaders() {
    let mut world = World::new();
    world.init_players(2);
    let player_two = typed_squad(&mut world, 2, false);
    let ground = typed_squad(&mut world, 1, false);
    let flying = typed_squad(&mut world, 1, true);
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(-1));
    add_value(
        &mut script,
        3,
        VarType::SquadList,
        TriggerValue::SquadList(Vec::new()),
    );
    add_value(&mut script, 4, VarType::Bool, TriggerValue::Bool(true));
    let mut condition = squad_query_condition(8);

    assert!(can_get_squads(&condition, &mut script, &world));
    assert_eq!(entities(&script, 3), &[player_two, ground, flying]);

    condition.version = 9;
    assert!(!can_get_squads(&condition, &mut script, &world));
    script.get_variable_mut(1).unwrap().value = TriggerValue::PlayerList(vec![1]);
    condition = condition.with_input_at(19, 4);
    assert!(can_get_squads(&condition, &mut script, &world));
    assert_eq!(entities(&script, 3), &[ground]);
}

fn unit_query_condition(version: u8) -> Condition {
    let mut condition = Condition::new(1, ConditionType::CanGetUnits)
        .with_output_at(6, 2)
        .with_output_at(8, 3)
        .with_input_at(10, 1);
    condition.version = version;
    condition
}

fn squad_query_condition(version: u8) -> Condition {
    let mut condition = Condition::new(1, ConditionType::CanGetSquads)
        .with_output_at(7, 2)
        .with_output_at(9, 3)
        .with_input_at(12, 1);
    condition.version = version;
    condition
}

fn spatial_unit_script(included: EntityId) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1]),
    );
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(-1));
    add_value(
        &mut script,
        3,
        VarType::UnitList,
        TriggerValue::UnitList(Vec::new()),
    );
    add_value(
        &mut script,
        4,
        VarType::Vector,
        TriggerValue::Vector(crate::TriggerVec3::zero()),
    );
    add_value(&mut script, 5, VarType::Float, TriggerValue::Float(2.0));
    add_value(
        &mut script,
        6,
        VarType::ObjectType,
        TriggerValue::ObjectType("Infantry".to_owned()),
    );
    add_value(
        &mut script,
        7,
        VarType::UnitList,
        TriggerValue::UnitList(vec![included]),
    );
    add_value(
        &mut script,
        8,
        VarType::Vector,
        TriggerValue::Vector(crate::TriggerVec3::new(0.0, 0.0, 1.0)),
    );
    for (id, value) in [(9, 3.0), (10, 1.0), (11, 1.0)] {
        add_value(&mut script, id, VarType::Float, TriggerValue::Float(value));
    }
    script
}

fn typed_squad(world: &mut World, player_id: u8, flying: bool) -> EntityId {
    let squad_id = world.create_squad(player_id);
    let unit_id = world.create_unit(player_id);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.object_types = vec!["Infantry".to_owned()];
    unit.flying = flying;
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn add_value(script: &mut TriggerScript, id: VarId, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn integer(script: &TriggerScript, id: VarId) -> i32 {
    script
        .get_variable(id)
        .and_then(|variable| variable.value.as_int())
        .expect("integer output")
}

fn entities(script: &TriggerScript, id: VarId) -> &[EntityId] {
    match &script.get_variable(id).expect("entity list").value {
        TriggerValue::UnitList(values) | TriggerValue::SquadList(values) => values,
        value => panic!("expected entity list, got {value:?}"),
    }
}
