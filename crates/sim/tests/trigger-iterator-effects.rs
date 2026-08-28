use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerIterator, TriggerScript,
    TriggerValue, TriggerVar, TriggerVec3, VarType,
};
use sim::{EntityId, World};

#[test]
fn unit_iterator_consumes_current_list_order_and_tracks_visited_ids() {
    let mut world = trigger_world();
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let inserted = world.create_unit(1);
    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, vec![first, second]));
    script.add_variable(iterator(1));
    script.add_variable(unit(2, EntityId::INVALID));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorUnitList,
        ConditionType::NextUnit,
    );
    let script_id = install_script(&mut world, script);

    let first_update = world.update_triggers();

    assert_eq!(first_update.triggers_fired, 3);
    assert_eq!(script_entity(&world, script_id, 2), second);
    let iterator = script_iterator(&world, script_id, 1);
    assert_eq!(iterator.source_list_id(), Some(0));
    assert_eq!(iterator.visited_unit_count(), 2);

    let script = world
        .trigger_engine_mut()
        .get_script_mut(script_id)
        .unwrap();
    let TriggerValue::UnitList(values) = &mut script.get_variable_mut(0).unwrap().value else {
        panic!("unit-list variable changed type");
    };
    values.insert(0, inserted);

    let second_update = world.update_triggers();

    assert_eq!(second_update.triggers_fired, 1);
    assert_eq!(script_entity(&world, script_id, 2), inserted);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_unit_count(),
        3
    );
}

#[test]
fn squad_iterator_reattachment_resets_its_visited_set() {
    let mut world = trigger_world();
    let first = world.create_squad(1);
    let second = world.create_squad(1);
    let mut script = TriggerScript::default();
    script.add_variable(squad_list(0, vec![first, second]));
    script.add_variable(iterator(1));
    script.add_variable(squad(2, EntityId::INVALID));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorSquadList,
        ConditionType::NextSquad,
    );
    let script_id = install_script(&mut world, script);

    world.update_triggers();
    assert_eq!(script_entity(&world, script_id, 2), second);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_squad_count(),
        2
    );

    let current_time = world.game_time_ms;
    world
        .trigger_engine_mut()
        .get_script_mut(script_id)
        .unwrap()
        .activate_trigger(0, current_time);
    world.update_triggers();

    assert_eq!(script_entity(&world, script_id, 2), second);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_squad_count(),
        2
    );
}

#[test]
fn player_iterator_preserves_list_order_and_visits_duplicate_ids_once() {
    let mut world = trigger_world();
    let mut script = TriggerScript::default();
    script.add_variable(player_list(0, vec![2, 1, 2]));
    script.add_variable(iterator(1));
    script.add_variable(player(2, -1));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorPlayerList,
        ConditionType::NextPlayer,
    );
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.triggers_fired, 3);
    assert_eq!(script_scalar(&world, script_id, 2), 1);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_player_count(),
        2
    );
}

#[test]
fn team_iterator_reattachment_resets_its_visited_set() {
    let mut world = trigger_world();
    let mut script = TriggerScript::default();
    script.add_variable(team_list(0, vec![4, 3]));
    script.add_variable(iterator(1));
    script.add_variable(team(2, -1));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorTeamList,
        ConditionType::NextTeam,
    );
    let script_id = install_script(&mut world, script);

    world.update_triggers();
    assert_eq!(script_scalar(&world, script_id, 2), 3);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_team_count(),
        2
    );

    let current_time = world.game_time_ms;
    world
        .trigger_engine_mut()
        .get_script_mut(script_id)
        .unwrap()
        .activate_trigger(0, current_time);
    world.update_triggers();

    assert_eq!(script_scalar(&world, script_id, 2), 3);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_team_count(),
        2
    );
}

#[test]
fn location_iterator_tracks_equal_vectors_once_in_current_list_order() {
    let mut world = trigger_world();
    let first = TriggerVec3::new(1.0, 2.0, 3.0);
    let second = TriggerVec3::new(4.0, 5.0, 6.0);
    let mut script = TriggerScript::default();
    script.add_variable(vector_list(0, vec![first, first, second]));
    script.add_variable(iterator(1));
    script.add_variable(vector(2, TriggerVec3::zero()));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorLocationList,
        ConditionType::NextLocation,
    );
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.triggers_fired, 3);
    assert_eq!(script_vector(&world, script_id, 2), second);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_vector_count(),
        2
    );
}

#[test]
fn object_iterator_uses_object_specific_visited_state() {
    let mut world = trigger_world();
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let mut script = TriggerScript::default();
    script.add_variable(object_list(0, vec![first, second]));
    script.add_variable(iterator(1));
    script.add_variable(object(2, EntityId::INVALID));
    script.add_variable(trigger(3, 10));
    add_iterator_loop(
        &mut script,
        EffectType::IteratorObjectList,
        ConditionType::NextObject,
    );
    let script_id = install_script(&mut world, script);

    world.update_triggers();

    assert_eq!(script_entity(&world, script_id, 2), second);
    assert_eq!(
        script_iterator(&world, script_id, 1).visited_object_count(),
        2
    );
}

fn add_iterator_loop(
    script: &mut TriggerScript,
    iterator_effect_type: EffectType,
    next_condition_type: ConditionType,
) {
    let attach = Effect::new(0, iterator_effect_type)
        .with_input_at(1, 0)
        .with_output_at(2, 1);
    let activate_loop = Effect::new(1, EffectType::TriggerActivate).with_input_at(1, 3);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(attach)
            .with_effect_on_true(activate_loop.clone()),
    );
    script.add_trigger(
        Trigger::new(10)
            .with_condition(
                Condition::new(0, next_condition_type)
                    .with_input_at(1, 1)
                    .with_output_at(2, 2),
            )
            .with_effect_on_true(activate_loop),
    );
}

fn trigger_world() -> World {
    let mut world = World::new();
    world.init_players(1);
    world
}

fn install_script(world: &mut World, script: TriggerScript) -> u32 {
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);
    script_id
}

fn unit_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::UnitList).with_value(TriggerValue::UnitList(values))
}

fn squad_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::SquadList).with_value(TriggerValue::SquadList(values))
}

fn player_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::PlayerList).with_value(TriggerValue::PlayerList(values))
}

fn team_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::TeamList).with_value(TriggerValue::TeamList(values))
}

fn vector_list(id: u32, values: Vec<TriggerVec3>) -> TriggerVar {
    TriggerVar::new(id, VarType::VectorList).with_value(TriggerValue::VectorList(values))
}

fn object_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::ObjectList).with_value(TriggerValue::ObjectList(values))
}

fn iterator(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Iterator)
        .with_value(TriggerValue::Iterator(TriggerIterator::default()))
}

fn unit(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(value))
}

fn squad(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(value))
}

fn player(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(value))
}

fn team(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Team).with_value(TriggerValue::Team(value))
}

fn vector(id: u32, value: TriggerVec3) -> TriggerVar {
    TriggerVar::new(id, VarType::Vector).with_value(TriggerValue::Vector(value))
}

fn object(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Object).with_value(TriggerValue::Object(value))
}

fn trigger(id: u32, value: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Trigger).with_value(TriggerValue::Trigger(value))
}

fn script_entity(world: &World, script_id: u32, variable_id: u32) -> EntityId {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_entity())
        .expect("entity output")
}

fn script_scalar(world: &World, script_id: u32, variable_id: u32) -> i32 {
    let value = &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("scalar output")
        .value;
    match value {
        TriggerValue::Player(value) | TriggerValue::Team(value) => *value,
        _ => panic!("scalar output changed type"),
    }
}

fn script_vector(world: &World, script_id: u32, variable_id: u32) -> TriggerVec3 {
    let value = &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("vector output")
        .value;
    match value {
        TriggerValue::Vector(value) => *value,
        _ => panic!("vector output changed type"),
    }
}

fn script_iterator(world: &World, script_id: u32, variable_id: u32) -> &TriggerIterator {
    let variable = world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("iterator variable");
    let TriggerValue::Iterator(iterator) = &variable.value else {
        panic!("iterator variable changed type");
    };
    iterator
}
