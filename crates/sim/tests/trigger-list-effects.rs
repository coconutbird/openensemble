use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarType,
};
use sim::{EntityId, TriggerVec3, UnitState, World};

#[test]
fn get_units_applies_retail_live_area_and_parent_squad_filters() {
    let mut world = trigger_world(2);
    let marine_squad = world.create_squad_at(1, glam::Vec3::new(3.0, 0.0, 4.0));
    let marine = world.create_unit_at(1, glam::Vec3::new(3.0, 100.0, 4.0));
    configure_unit(&mut world, marine, "Marine", "Infantry");
    assert!(world.attach_unit_to_squad(marine, marine_squad));
    let vehicle = world.create_unit_at(1, glam::Vec3::ZERO);
    configure_unit(&mut world, vehicle, "Warthog", "Vehicle");
    let enemy_marine = world.create_unit_at(2, glam::Vec3::new(2.0, 0.0, 0.0));
    configure_unit(&mut world, enemy_marine, "Marine", "Infantry");
    let dead_marine = world.create_unit_at(1, glam::Vec3::ZERO);
    configure_unit(&mut world, dead_marine, "Marine", "Infantry");
    world.get_unit_mut(dead_marine).unwrap().state = UnitState::Dead;

    let mut script = TriggerScript::default();
    script.add_variable(player(0, 1));
    script.add_variable(object_type(1, "Infantry"));
    script.add_variable(unit_list(2, vec![marine_squad]));
    script.add_variable(output_unit_list(3));
    script.add_variable(output_integer(4));
    script.add_variable(location(5, 0.0, 0.0, 0.0));
    script.add_variable(float(6, 5.0));
    script.add_variable(output_unit_list(7));
    script.add_variable(output_integer(8));
    add_guard_variables(&mut script);

    let mut filtered = Effect::new(0, EffectType::GetUnits)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_output_at(5, 3)
        .with_output_at(6, 4)
        .with_input_at(7, 2);
    filtered.version = 3;
    let mut in_area = Effect::new(1, EffectType::GetUnits)
        .with_input_at(3, 5)
        .with_input_at(4, 6)
        .with_output_at(5, 7)
        .with_output_at(6, 8);
    in_area.version = 3;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(filtered)
            .with_effect_on_true(in_area),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert_eq!(script_entities(&world, script_id, 3), vec![marine]);
    assert_eq!(script_integer(&world, script_id, 4), 1);
    assert_eq!(
        script_entities(&world, script_id, 7),
        vec![marine, vehicle, enemy_marine]
    );
    assert_eq!(script_integer(&world, script_id, 8), 3);
}

#[test]
fn get_squads_combines_proto_type_filter_and_area_rules() {
    let mut world = trigger_world(2);
    let infantry = create_typed_squad(
        &mut world,
        1,
        70,
        glam::Vec3::new(3.0, 25.0, 4.0),
        "Infantry",
    );
    let vehicle = create_typed_squad(&mut world, 1, 70, glam::Vec3::ZERO, "Vehicle");
    let enemy = create_typed_squad(&mut world, 2, 70, glam::Vec3::ZERO, "Infantry");
    let empty = world.create_squad(1);
    world.get_squad_mut(empty).unwrap().proto_squad_id = 70;

    let mut script = TriggerScript::default();
    script.add_variable(player(0, 1));
    script.add_variable(proto_squad(1, 70));
    script.add_variable(object_type(2, "Infantry"));
    script.add_variable(squad_list(3, vec![infantry, empty, enemy]));
    script.add_variable(output_squad_list(4));
    script.add_variable(output_integer(5));
    script.add_variable(location(6, 0.0, 0.0, 0.0));
    script.add_variable(float(7, 5.0));
    script.add_variable(output_squad_list(8));
    add_guard_variables(&mut script);

    let mut filtered = Effect::new(0, EffectType::GetSquads)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_output_at(5, 4)
        .with_output_at(6, 5)
        .with_input_at(7, 3)
        .with_input_at(8, 2);
    filtered.version = 4;
    let mut in_area = Effect::new(1, EffectType::GetSquads)
        .with_input_at(3, 6)
        .with_input_at(4, 7)
        .with_output_at(5, 8)
        .with_input_at(8, 2);
    in_area.version = 4;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(filtered)
            .with_effect_on_true(in_area),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert_eq!(script_entities(&world, script_id, 4), vec![infantry, empty]);
    assert_eq!(script_integer(&world, script_id, 5), 2);
    assert_eq!(script_entities(&world, script_id, 8), vec![infantry, enemy]);
    assert_ne!(vehicle, infantry);
}

#[test]
fn list_add_remove_and_size_preserve_retail_order_and_stale_list_values() {
    let mut world = trigger_world(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let stale = world.create_unit(1);
    assert!(world.remove_unit(stale).is_some());

    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, vec![first]));
    script.add_variable(unit(1, second));
    script.add_variable(unit_list(2, vec![first, stale]));
    script.add_variable(unit_list(3, vec![second]));
    script.add_variable(boolean(4, false));
    script.add_variable(output_integer(5));
    script.add_variable(unit(6, first));
    script.add_variable(unit_list(7, vec![stale]));
    script.add_variable(output_integer(8));
    add_guard_variables(&mut script);

    let add = Effect::new(0, EffectType::UnitListAdd)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_input_at(4, 3)
        .with_input_at(5, 4);
    let size_after_add = Effect::new(1, EffectType::UnitListGetSize)
        .with_input_at(1, 0)
        .with_output_at(2, 5);
    let remove = Effect::new(2, EffectType::UnitListRemove)
        .with_input_at(1, 0)
        .with_input_at(2, 6)
        .with_input_at(3, 7)
        .with_input_at(4, 4);
    let size_after_remove = Effect::new(3, EffectType::UnitListGetSize)
        .with_input_at(1, 0)
        .with_output_at(2, 8);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(add)
            .with_effect_on_true(size_after_add)
            .with_effect_on_true(remove)
            .with_effect_on_true(size_after_remove),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 4);
    assert_eq!(script_integer(&world, script_id, 5), 3);
    assert_eq!(script_entities(&world, script_id, 0), vec![second]);
    assert_eq!(script_integer(&world, script_id, 8), 1);
}

#[test]
fn self_aliased_remove_list_matches_retail_mutating_iteration() {
    let mut world = trigger_world(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    let third = world.create_unit(1);
    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, vec![first, second, third]));
    script.add_variable(boolean(1, false));
    add_guard_variables(&mut script);
    let remove = Effect::new(0, EffectType::UnitListRemove)
        .with_input_at(1, 0)
        .with_input_at(3, 0)
        .with_input_at(4, 1);
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(remove));
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert_eq!(script_entities(&world, script_id, 0), vec![second]);
}

#[test]
fn partitions_preserve_retail_rounding_and_the_unit_source_clear_quirk() {
    let mut world = trigger_world(1);
    let units = (0..5).map(|_| world.create_unit(1)).collect::<Vec<_>>();
    let squads = (0..3).map(|_| world.create_squad(1)).collect::<Vec<_>>();
    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, units.clone()));
    script.add_variable(float(1, 0.5));
    script.add_variable(integer(2, 0).null());
    script.add_variable(output_unit_list(3));
    script.add_variable(output_unit_list(4));
    script.add_variable(squad_list(5, squads.clone()));
    script.add_variable(integer(6, 1));
    script.add_variable(output_squad_list(7));
    script.add_variable(output_squad_list(8));
    add_guard_variables(&mut script);

    let unit_partition = Effect::new(0, EffectType::UnitListPartition)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_output_at(4, 3)
        .with_output_at(5, 4);
    let squad_partition = Effect::new(1, EffectType::SquadListPartition)
        .with_input_at(1, 5)
        .with_input_at(3, 6)
        .with_output_at(4, 7)
        .with_output_at(5, 8);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(unit_partition)
            .with_effect_on_true(squad_partition),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert!(script_entities(&world, script_id, 0).is_empty());
    assert_eq!(script_entities(&world, script_id, 3), units[..2]);
    assert_eq!(script_entities(&world, script_id, 4), units[2..]);
    assert_eq!(script_entities(&world, script_id, 5), squads);
    assert_eq!(script_entities(&world, script_id, 7), squads[..1]);
    assert_eq!(script_entities(&world, script_id, 8), squads[1..]);
}

#[test]
fn list_diff_copies_sources_before_clearing_aliased_outputs() {
    let mut world = trigger_world(1);
    let only_a = world.create_unit(1);
    let shared = world.create_unit(1);
    let only_b = world.create_unit(1);
    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, vec![only_a, shared]));
    script.add_variable(unit_list(1, vec![shared, only_b]));
    script.add_variable(output_unit_list(2));
    script.add_variable(output_unit_list(3));
    add_guard_variables(&mut script);

    let difference = Effect::new(0, EffectType::UnitListDiff)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_output_at(3, 0)
        .with_output_at(4, 2)
        .with_output_at(5, 3);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(difference),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert_eq!(script_entities(&world, script_id, 0), vec![only_a]);
    assert_eq!(script_entities(&world, script_id, 2), vec![only_b]);
    assert_eq!(script_entities(&world, script_id, 3), vec![shared]);
}

#[test]
fn list_shuffle_uses_the_retail_csimrand_swap_sequence() {
    let mut world = World::with_seed(1);
    world.init_players(1);
    let values = (0..4).map(|_| world.create_unit(1)).collect::<Vec<_>>();
    let mut script = TriggerScript::default();
    script.add_variable(unit_list(0, values.clone()));
    add_guard_variables(&mut script);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(Effect::new(0, EffectType::UnitListShuffle).with_input_at(1, 0)),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert_eq!(
        script_entities(&world, script_id, 0),
        vec![values[1], values[2], values[0], values[3]]
    );
}

fn create_typed_squad(
    world: &mut World,
    player_id: u8,
    prototype_id: i32,
    position: glam::Vec3,
    object_type: &str,
) -> EntityId {
    let squad_id = world.create_squad_at(player_id, position);
    world.get_squad_mut(squad_id).unwrap().proto_squad_id = prototype_id;
    let unit_id = world.create_unit_at(player_id, position);
    configure_unit(world, unit_id, object_type, object_type);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn configure_unit(world: &mut World, unit_id: EntityId, name: &str, object_type: &str) {
    let unit = world.get_unit_mut(unit_id).unwrap();
    name.clone_into(&mut unit.proto_object_name);
    unit.object_types = vec![object_type.to_owned()];
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

fn boolean(id: u32, value: bool) -> TriggerVar {
    TriggerVar::new(id, VarType::Bool).with_value(TriggerValue::Bool(value))
}

fn float(id: u32, value: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::Float).with_value(TriggerValue::Float(value))
}

fn integer(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value))
}

fn location(id: u32, x: f32, y: f32, z: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::UILocation)
        .with_value(TriggerValue::Location(TriggerVec3::new(x, y, z)))
}

fn object_type(id: u32, value: &str) -> TriggerVar {
    TriggerVar::new(id, VarType::ObjectType).with_value(TriggerValue::ObjectType(value.to_owned()))
}

fn player(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(value))
}

fn proto_squad(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoSquad).with_value(TriggerValue::ProtoSquad(value))
}

fn squad_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::SquadList).with_value(TriggerValue::SquadList(values))
}

fn unit(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(value))
}

fn unit_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::UnitList).with_value(TriggerValue::UnitList(values))
}

fn output_integer(id: u32) -> TriggerVar {
    integer(id, 0).as_output()
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

fn script_integer(world: &World, script_id: u32, variable_id: u32) -> i32 {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_int())
        .expect("integer output")
}
