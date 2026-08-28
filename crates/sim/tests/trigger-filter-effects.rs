use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, EntityFilterSet, Trigger, TriggerScript,
    TriggerValue, TriggerVar, VarType,
};
use sim::{EntityId, UnitState, World};

#[test]
fn unit_filters_and_predicates_match_retail_order_and_invalid_handling() {
    let mut world = trigger_world(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let matching = create_typed_unit(&mut world, 1, 10, "Infantry");
    let wrong_owner = create_typed_unit(&mut world, 2, 10, "Infantry");
    let wrong_type = create_typed_unit(&mut world, 1, 11, "Vehicle");
    let dead = create_typed_unit(&mut world, 1, 10, "Infantry");
    world.get_unit_mut(dead).unwrap().state = UnitState::Dead;
    let stale = world.create_unit(1);
    assert!(world.remove_unit(stale).is_some());

    let mut script = TriggerScript::default();
    script.add_variable(filter_set(0));
    script.add_variable(boolean(1, false));
    script.add_variable(player_list(2, vec![1]));
    script.add_variable(team_list(3, vec![1]));
    script.add_variable(proto_object_list(4, vec![10]));
    script.add_variable(object_type_list(5, vec!["Infantry"]));
    script.add_variable(unit_list(
        6,
        vec![matching, wrong_owner, wrong_type, dead, stale],
    ));
    script.add_variable(output_unit_list(7));
    script.add_variable(output_unit_list(8));
    add_guard_variables(&mut script);

    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(
                Effect::new(0, EffectType::EntityFilterAddPlayers)
                    .with_input_at(1, 0)
                    .with_input_at(2, 2)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(1, EffectType::EntityFilterAddTeams)
                    .with_input_at(1, 0)
                    .with_input_at(2, 3)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(2, EffectType::EntityFilterAddProtoObjects)
                    .with_input_at(1, 0)
                    .with_input_at(2, 4)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(3, EffectType::EntityFilterAddObjectTypes)
                    .with_input_at(1, 0)
                    .with_input_at(2, 5)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(4, EffectType::EntityFilterAddIsAlive)
                    .with_input_at(1, 0)
                    .with_input_at(2, 1),
            )
            .with_effect_on_true(
                Effect::new(5, EffectType::UnitListFilter)
                    .with_input_at(1, 6)
                    .with_input_at(2, 0)
                    .with_output_at(3, 7)
                    .with_output_at(4, 8),
            ),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 6);
    assert_eq!(script_entities(&world, script_id, 7), vec![matching]);
    assert_eq!(
        script_entities(&world, script_id, 8),
        vec![wrong_owner, wrong_type, dead]
    );
    assert_eq!(script_filter_set(&world, script_id, 0).filter_count(), 5);
}

#[test]
fn proto_squad_and_all_child_filters_apply_to_both_entity_classes() {
    let mut world = trigger_world(1);
    let matching_squad = world.create_squad(1);
    world.get_squad_mut(matching_squad).unwrap().proto_squad_id = 70;
    let matching_unit = create_typed_unit(&mut world, 1, 10, "Infantry");
    assert!(world.attach_unit_to_squad(matching_unit, matching_squad));

    let failed_squad = world.create_squad(1);
    world.get_squad_mut(failed_squad).unwrap().proto_squad_id = 70;
    let failed_unit = create_typed_unit(&mut world, 1, 11, "Vehicle");
    assert!(world.attach_unit_to_squad(failed_unit, failed_squad));

    let empty_squad = world.create_squad(1);
    world.get_squad_mut(empty_squad).unwrap().proto_squad_id = 70;
    let standalone = create_typed_unit(&mut world, 1, 10, "Infantry");

    let mut script = TriggerScript::default();
    script.add_variable(filter_set(0));
    script.add_variable(boolean(1, false));
    script.add_variable(proto_object_list(2, vec![10]));
    script.add_variable(proto_squad_list(3, vec![70]));
    script.add_variable(object_type_list(4, vec!["Infantry"]));
    script.add_variable(squad_list(
        5,
        vec![matching_squad, failed_squad, empty_squad],
    ));
    script.add_variable(output_squad_list(6));
    script.add_variable(output_squad_list(7));
    script.add_variable(unit_list(8, vec![matching_unit, failed_unit, standalone]));
    script.add_variable(output_unit_list(9));
    script.add_variable(output_unit_list(10));
    add_guard_variables(&mut script);

    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(
                Effect::new(0, EffectType::EntityFilterAddProtoObjects)
                    .with_input_at(1, 0)
                    .with_input_at(2, 2)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(1, EffectType::EntityFilterAddProtoSquads)
                    .with_input_at(1, 0)
                    .with_input_at(2, 3)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(2, EffectType::EntityFilterAddObjectTypes)
                    .with_input_at(1, 0)
                    .with_input_at(2, 4)
                    .with_input_at(3, 1),
            )
            .with_effect_on_true(
                Effect::new(3, EffectType::SquadListFilter)
                    .with_input_at(1, 5)
                    .with_input_at(2, 0)
                    .with_output_at(3, 6)
                    .with_output_at(4, 7),
            )
            .with_effect_on_true(
                Effect::new(4, EffectType::UnitListFilter)
                    .with_input_at(1, 8)
                    .with_input_at(2, 0)
                    .with_output_at(3, 9)
                    .with_output_at(4, 10),
            ),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 5);
    assert_eq!(
        script_entities(&world, script_id, 6),
        vec![matching_squad, empty_squad]
    );
    assert_eq!(script_entities(&world, script_id, 7), vec![failed_squad]);
    assert_eq!(script_entities(&world, script_id, 9), vec![matching_unit]);
    assert_eq!(
        script_entities(&world, script_id, 10),
        vec![failed_unit, standalone]
    );
}

#[test]
fn in_list_copies_payload_clear_empties_set_and_failed_alias_write_wins() {
    let mut world = trigger_world(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let stale = world.create_unit(1);
    assert!(world.remove_unit(stale).is_some());

    let mut script = TriggerScript::default();
    script.add_variable(filter_set(0));
    script.add_variable(unit_list(1, vec![first]));
    script.add_variable(unit_list(2, vec![first, second, stale]));
    script.add_variable(boolean(3, false));
    script.add_variable(boolean(4, true));
    script.add_variable(output_unit_list(5));
    script.add_variable(output_unit_list(6));
    add_guard_variables(&mut script);

    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(
                Effect::new(0, EffectType::EntityFilterAddInList)
                    .with_input_at(1, 0)
                    .with_input_at(2, 1)
                    .with_input_at(4, 3),
            )
            .with_effect_on_true(
                Effect::new(1, EffectType::UnitListRemove)
                    .with_input_at(1, 1)
                    .with_input_at(4, 4),
            )
            .with_effect_on_true(
                Effect::new(2, EffectType::UnitListFilter)
                    .with_input_at(1, 2)
                    .with_input_at(2, 0)
                    .with_output_at(3, 5)
                    .with_output_at(4, 5),
            )
            .with_effect_on_true(Effect::new(3, EffectType::EntityFilterClear).with_input_at(1, 0))
            .with_effect_on_true(
                Effect::new(4, EffectType::UnitListFilter)
                    .with_input_at(1, 2)
                    .with_input_at(2, 0)
                    .with_output_at(3, 6),
            ),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 5);
    assert!(script_entities(&world, script_id, 1).is_empty());
    assert_eq!(script_entities(&world, script_id, 5), vec![second]);
    assert_eq!(script_entities(&world, script_id, 6), vec![first, second]);
    assert_eq!(script_filter_set(&world, script_id, 0).filter_count(), 0);
}

#[test]
fn idle_and_diplomacy_filters_use_live_actions_children_and_directed_teams() {
    let mut world = trigger_world(3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.get_player_mut(3).unwrap().team_id = 1;
    world.configure_standard_team_relations();

    let idle_enemy = world.create_unit(2);
    let moving_enemy = world.create_unit(2);
    let idle_ally = world.create_unit(3);
    let idle_enemy_squad = world.create_squad(2);
    let moving_enemy_squad = world.create_squad(2);
    let moving_member = world.create_unit(2);
    assert!(world.attach_unit_to_squad(moving_member, moving_enemy_squad));
    let idle_ally_squad = world.create_squad(3);

    world.update_entities(0.05);
    assert!(
        world
            .get_unit_mut(moving_enemy)
            .unwrap()
            .move_to(glam::Vec3::new(10.0, 0.0, 0.0))
    );
    world
        .get_squad_mut(moving_enemy_squad)
        .unwrap()
        .move_to(glam::Vec3::new(10.0, 0.0, 0.0));
    world.update_entities(0.05);

    let mut script = TriggerScript::default();
    script.add_variable(filter_set(0));
    script.add_variable(boolean(1, false));
    script.add_variable(relation_type(2, 3));
    script.add_variable(player(3, 1));
    script.add_variable(unit_list(4, vec![idle_enemy, moving_enemy, idle_ally]));
    script.add_variable(output_unit_list(5));
    script.add_variable(output_unit_list(6));
    script.add_variable(squad_list(
        7,
        vec![idle_enemy_squad, moving_enemy_squad, idle_ally_squad],
    ));
    script.add_variable(output_squad_list(8));
    script.add_variable(output_squad_list(9));
    add_guard_variables(&mut script);

    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(
                Effect::new(0, EffectType::EntityFilterAddIsIdle)
                    .with_input_at(1, 0)
                    .with_input_at(2, 1),
            )
            .with_effect_on_true(
                Effect::new(1, EffectType::EntityFilterAddDiplomacy)
                    .with_input_at(1, 0)
                    .with_input_at(2, 2)
                    .with_input_at(3, 3)
                    .with_input_at(5, 1),
            )
            .with_effect_on_true(
                Effect::new(2, EffectType::UnitListFilter)
                    .with_input_at(1, 4)
                    .with_input_at(2, 0)
                    .with_output_at(3, 5)
                    .with_output_at(4, 6),
            )
            .with_effect_on_true(
                Effect::new(3, EffectType::SquadListFilter)
                    .with_input_at(1, 7)
                    .with_input_at(2, 0)
                    .with_output_at(3, 8)
                    .with_output_at(4, 9),
            ),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 4);
    assert_eq!(script_entities(&world, script_id, 5), vec![idle_enemy]);
    assert_eq!(
        script_entities(&world, script_id, 6),
        vec![moving_enemy, idle_ally]
    );
    assert_eq!(
        script_entities(&world, script_id, 8),
        vec![idle_enemy_squad]
    );
    assert_eq!(
        script_entities(&world, script_id, 9),
        vec![moving_enemy_squad, idle_ally_squad]
    );
    assert_eq!(script_filter_set(&world, script_id, 0).filter_count(), 2);
}

#[test]
fn diplomacy_filter_does_not_fall_back_when_a_used_player_is_invalid() {
    let mut world = trigger_world(1);
    world.get_player_mut(1).unwrap().team_id = 1;
    let mut script = TriggerScript::default();
    script.add_variable(filter_set(0));
    script.add_variable(relation_type(1, 3));
    script.add_variable(player(2, 99));
    script.add_variable(team(3, 1));
    script.add_variable(boolean(4, false));
    add_guard_variables(&mut script);
    script.add_trigger(
        Trigger::new(0).starts_active().with_effect_on_true(
            Effect::new(0, EffectType::EntityFilterAddDiplomacy)
                .with_input_at(1, 0)
                .with_input_at(2, 1)
                .with_input_at(3, 2)
                .with_input_at(4, 3)
                .with_input_at(5, 4),
        ),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_skipped, 1);
    assert_eq!(script_filter_set(&world, script_id, 0).filter_count(), 0);
}

fn create_typed_unit(
    world: &mut World,
    player_id: u8,
    prototype_id: i32,
    object_type: &str,
) -> EntityId {
    let unit_id = world.create_unit(player_id);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_id = prototype_id;
    unit.proto_object_name = format!("Prototype{prototype_id}");
    unit.object_types = vec![object_type.to_owned()];
    unit_id
}

fn trigger_world(players: u8) -> World {
    let mut world = World::new();
    world.init_players(players);
    world
}

fn install_script(world: &mut World, script: TriggerScript) -> u32 {
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);
    script_id
}

fn add_guard_variables(script: &mut TriggerScript) {
    script.add_variable(integer(90, 5));
    script.add_variable(TriggerVar::new(91, VarType::Time).with_value(TriggerValue::Time(100)));
}

fn add_guard(script: &mut TriggerScript) {
    script.add_trigger(
        Trigger::new(90).starts_active().with_condition(
            Condition::new(90, ConditionType::GameTime)
                .with_input_at(1, 90)
                .with_input_at(2, 91),
        ),
    );
}

fn filter_set(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::EntityFilterSet)
        .with_value(TriggerValue::EntityFilterSet(EntityFilterSet::default()))
}

fn boolean(id: u32, value: bool) -> TriggerVar {
    TriggerVar::new(id, VarType::Bool).with_value(TriggerValue::Bool(value))
}

fn integer(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value))
}

fn object_type_list(id: u32, values: Vec<&str>) -> TriggerVar {
    TriggerVar::new(id, VarType::ObjectTypeList).with_value(TriggerValue::ObjectTypeList(
        values.into_iter().map(str::to_owned).collect(),
    ))
}

fn player_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::PlayerList).with_value(TriggerValue::PlayerList(values))
}

fn player(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(value))
}

fn relation_type(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::RelationType).with_value(TriggerValue::Int(value))
}

fn proto_object_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoObjectList).with_value(TriggerValue::ProtoObjectList(values))
}

fn proto_squad_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoSquadList).with_value(TriggerValue::ProtoSquadList(values))
}

fn squad_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::SquadList).with_value(TriggerValue::SquadList(values))
}

fn team_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::TeamList).with_value(TriggerValue::TeamList(values))
}

fn team(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Team).with_value(TriggerValue::Team(value))
}

fn unit_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::UnitList).with_value(TriggerValue::UnitList(values))
}

fn output_squad_list(id: u32) -> TriggerVar {
    squad_list(id, Vec::new()).as_output()
}

fn output_unit_list(id: u32) -> TriggerVar {
    unit_list(id, Vec::new()).as_output()
}

fn script_entities(world: &World, script_id: u32, variable_id: u32) -> Vec<EntityId> {
    match &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("entity-list output")
        .value
    {
        TriggerValue::UnitList(values) | TriggerValue::SquadList(values) => values.clone(),
        value => panic!("expected entity list, got {value:?}"),
    }
}

fn script_filter_set(world: &World, script_id: u32, variable_id: u32) -> &EntityFilterSet {
    match &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("entity-filter-set output")
        .value
    {
        TriggerValue::EntityFilterSet(value) => value,
        value => panic!("expected entity filter set, got {value:?}"),
    }
}
