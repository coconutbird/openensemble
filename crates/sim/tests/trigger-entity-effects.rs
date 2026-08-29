use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarType,
};
use sim::{Entity, EntityId, TriggerVec3, World, spawn_object_at, spawn_squad_at};

#[test]
fn create_object_writes_retail_outputs_and_honors_no_physics() {
    let database = entity_database();
    let mut world = trigger_world();
    let retained = world.create_unit(1);
    let mut script = TriggerScript::default();
    script.add_variable(proto_object(0, 101));
    script.add_variable(location(1, 10.0, 2.0, 20.0));
    script.add_variable(player(2, 1));
    script.add_variable(object(3, EntityId::INVALID).as_output());
    script.add_variable(
        TriggerVar::new(4, VarType::ObjectList)
            .with_value(TriggerValue::ObjectList(vec![retained]))
            .as_output(),
    );
    script.add_variable(boolean(5, true));
    script.add_variable(vector(6, 1.0, 0.0, 0.0));
    script.add_variable(boolean(7, true));
    add_guard_variables(&mut script);
    let mut create = Effect::new(0, EffectType::CreateObject)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_output_at(4, 3)
        .with_output_at(8, 4)
        .with_input_at(9, 5)
        .with_input_at(10, 6)
        .with_input_at(11, 7);
    create.version = 6;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(create));
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 1);
    let created = script_entity(&world, script_id, 3);
    let unit = world.get_unit(created).expect("created Warthog object");
    assert_eq!(unit.base.position, glam::Vec3::new(10.0, 2.0, 20.0));
    assert_eq!(unit.base.forward, glam::Vec3::X);
    assert!(
        unit.physics.is_none(),
        "NoPhysics removes the configured body"
    );
    assert_eq!(script_entities(&world, script_id, 4), vec![created]);
}

#[test]
fn create_squad_and_create_unit_share_database_backed_spawn_state() {
    let database = entity_database();
    let mut world = trigger_world();
    let mut script = TriggerScript::default();
    script.add_variable(proto_squad(0, 201));
    script.add_variable(location(1, 3.0, 0.0, 4.0));
    script.add_variable(player(2, 1));
    script.add_variable(squad(3, EntityId::INVALID).as_output());
    script.add_variable(empty_squad_list(4));
    script.add_variable(boolean(5, true));
    script.add_variable(vector(6, -1.0, 0.0, 0.0));
    script.add_variable(proto_object(7, 102));
    script.add_variable(location(8, 30.0, 0.0, 40.0));
    script.add_variable(boolean(9, false));
    script.add_variable(unit(10, EntityId::INVALID).as_output());
    script.add_variable(empty_unit_list(11));
    script.add_variable(squad(12, EntityId::INVALID).as_output());
    script.add_variable(empty_squad_list(13));
    script.add_variable(vector(14, 0.0, 0.0, -1.0));
    add_guard_variables(&mut script);

    let mut create_squad = Effect::new(0, EffectType::CreateSquad)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_output_at(4, 3)
        .with_output_at(5, 4)
        .with_input_at(6, 5)
        .with_input_at(12, 6);
    create_squad.version = 7;
    let mut create_unit = Effect::new(1, EffectType::CreateUnit)
        .with_input_at(1, 7)
        .with_input_at(2, 2)
        .with_input_at(3, 8)
        .with_input_at(4, 9)
        .with_output_at(5, 10)
        .with_output_at(6, 11)
        .with_output_at(7, 12)
        .with_output_at(8, 13)
        .with_input_at(9, 5)
        .with_input_at(10, 14);
    create_unit.version = 2;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(create_squad)
            .with_effect_on_true(create_unit),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 2);
    let proto_squad_id = script_entity(&world, script_id, 3);
    let proto_squad = world
        .get_squad(proto_squad_id)
        .expect("created proto squad");
    assert_eq!(proto_squad.base.position, glam::Vec3::new(3.0, 0.0, 4.0));
    assert_eq!(proto_squad.base.forward, -glam::Vec3::X);
    assert_eq!(proto_squad.unit_ids.len(), 2);
    assert_eq!(script_entities(&world, script_id, 4), vec![proto_squad_id]);

    let unit_id = script_entity(&world, script_id, 10);
    let synthetic_squad_id = script_entity(&world, script_id, 12);
    let unit = world.get_unit(unit_id).expect("created unit leader");
    let synthetic_squad = world
        .get_squad(synthetic_squad_id)
        .expect("created unit wrapper squad");
    assert_eq!(unit.squad_id, Some(synthetic_squad_id));
    assert_eq!(synthetic_squad.unit_ids, vec![unit_id]);
    assert_eq!(unit.base.forward, -glam::Vec3::Z);
    assert!(!unit.built, "StartBuilt=false is retained for buildings");
    assert_eq!(script_entities(&world, script_id, 11), vec![unit_id]);
    assert_eq!(
        script_entities(&world, script_id, 13),
        vec![synthetic_squad_id]
    );
}

#[test]
fn create_squads_preserves_duplicate_prototypes_and_retail_list_outputs() {
    let database = entity_database();
    let mut world = trigger_world();
    let retained = world.create_squad(1);
    let mut script = TriggerScript::default();
    script.add_variable(proto_squad_list(0, vec![201, 201]));
    script.add_variable(player(1, 1));
    script.add_variable(location(2, 3.0, 0.0, 4.0));
    script.add_variable(vector(3, -1.0, 0.0, 0.0));
    script.add_variable(location(4, 30.0, 0.0, 40.0));
    script.add_variable(empty_squad_list(5));
    script.add_variable(
        TriggerVar::new(6, VarType::SquadList)
            .with_value(TriggerValue::SquadList(vec![retained]))
            .as_output(),
    );
    script.add_variable(boolean(7, true));
    add_guard_variables(&mut script);
    let mut create = Effect::new(0, EffectType::CreateSquads)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_input_at(4, 3)
        .with_input_at(7, 4)
        .with_output_at(9, 5)
        .with_output_at(10, 6)
        .with_input_at(11, 7);
    create.version = 1;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(create));
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 1);
    assert!(update.unsupported_effect_types.is_empty());
    let created = script_entities(&world, script_id, 5);
    assert_eq!(created.len(), 2, "duplicate entries create distinct squads");
    assert_eq!(script_entities(&world, script_id, 6), created);
    for squad_id in created {
        let squad = world.get_squad(squad_id).expect("created batch squad");
        assert_eq!(squad.base.position, glam::Vec3::new(3.0, 0.0, 4.0));
        assert_eq!(squad.base.forward, -glam::Vec3::X);
        assert_eq!(squad.move_target, Some(glam::Vec3::new(30.0, 0.0, 40.0)));
    }
}

#[test]
fn kill_and_destroy_preserve_the_retail_lifecycle_distinction() {
    let database = entity_database();
    let mut world = trigger_world();
    let killed_squad = spawn_squad_at(
        &mut world,
        &database,
        1,
        201,
        glam::Vec3::ZERO,
        glam::Vec3::Z,
    )
    .unwrap();
    let killed_members = world.get_squad(killed_squad).unwrap().unit_ids.clone();
    let destroyed_unit =
        spawn_object_at(&mut world, &database, 1, 101, glam::Vec3::X, glam::Vec3::Z).unwrap();
    let mut script = TriggerScript::default();
    script.add_variable(squad(0, killed_squad));
    script.add_variable(unit(1, destroyed_unit));
    add_guard_variables(&mut script);
    let mut kill = Effect::new(0, EffectType::Kill).with_input_at(5, 0);
    kill.version = 4;
    let mut destroy = Effect::new(1, EffectType::Destroy).with_input_at(3, 1);
    destroy.version = 4;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(kill)
            .with_effect_on_true(destroy),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert!(world.get_unit(destroyed_unit).is_none());
    assert!(
        world
            .get_squad(killed_squad)
            .is_some_and(|squad| !squad.is_alive())
    );
    assert!(killed_members.iter().all(|member_id| {
        world
            .get_unit(*member_id)
            .is_some_and(|unit| !unit.is_alive())
    }));

    world.update_entities(0.05);
    assert!(world.get_squad(killed_squad).is_none());
    assert!(
        killed_members
            .iter()
            .all(|member_id| world.get_unit(*member_id).is_none())
    );
}

#[test]
fn creation_failures_preserve_each_retail_output_contract() {
    let database = entity_database();
    let mut world = trigger_world();
    let retained_unit = world.create_unit(1);
    let retained_squad = world.create_squad(1);
    let mut script = TriggerScript::default();
    script.add_variable(proto_object(0, -1));
    script.add_variable(location(1, 0.0, 0.0, 0.0));
    script.add_variable(player(2, 1));
    script.add_variable(object(3, retained_unit).as_output());
    script.add_variable(
        TriggerVar::new(4, VarType::ObjectList)
            .with_value(TriggerValue::ObjectList(vec![retained_unit]))
            .as_output(),
    );
    script.add_variable(boolean(5, true));
    script.add_variable(unit(6, retained_unit).as_output());
    script.add_variable(
        TriggerVar::new(7, VarType::UnitList)
            .with_value(TriggerValue::UnitList(vec![retained_unit]))
            .as_output(),
    );
    script.add_variable(squad(8, retained_squad).as_output());
    script.add_variable(
        TriggerVar::new(9, VarType::SquadList)
            .with_value(TriggerValue::SquadList(vec![retained_squad]))
            .as_output(),
    );
    script.add_variable(boolean(10, true));
    add_guard_variables(&mut script);
    let mut create_object = Effect::new(0, EffectType::CreateObject)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_output_at(4, 3)
        .with_output_at(8, 4)
        .with_input_at(9, 5);
    create_object.version = 5;
    let mut create_unit = Effect::new(1, EffectType::CreateUnit)
        .with_input_at(1, 0)
        .with_input_at(2, 2)
        .with_input_at(3, 1)
        .with_input_at(4, 10)
        .with_output_at(5, 6)
        .with_output_at(6, 7)
        .with_output_at(7, 8)
        .with_output_at(8, 9)
        .with_input_at(9, 5);
    create_unit.version = 1;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(create_object)
            .with_effect_on_true(create_unit),
    );
    add_guard(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 2);
    assert_eq!(script_entity(&world, script_id, 3), EntityId::INVALID);
    assert!(script_entities(&world, script_id, 4).is_empty());
    assert_eq!(script_entity(&world, script_id, 6), EntityId::INVALID);
    assert_eq!(script_entity(&world, script_id, 8), EntityId::INVALID);
    assert_eq!(script_entities(&world, script_id, 7), vec![retained_unit]);
    assert_eq!(script_entities(&world, script_id, 9), vec![retained_squad]);
}

#[test]
fn create_squad_fly_in_without_civ_transport_keeps_retail_ground_fallback() {
    let database = entity_database();
    let mut world = trigger_world();
    let mut script = TriggerScript::default();
    script.add_variable(proto_squad(0, 201));
    script.add_variable(location(1, 0.0, 0.0, 0.0));
    script.add_variable(player(2, 1));
    script.add_variable(location(3, 20.0, 10.0, 20.0));
    let mut create = Effect::new(0, EffectType::CreateSquad)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(3, 2)
        .with_input_at(8, 3);
    create.version = 6;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(create));
    install_script(&mut world, script);

    let update = world.update_triggers_with_database(&database);

    assert!(update.unsupported_effect_types.is_empty());
    assert_eq!(update.effects_applied, 1);
    let squads = world.squads.iter().collect::<Vec<_>>();
    let [(_, squad)] = squads.as_slice() else {
        panic!("the ground fallback should retain one created squad");
    };
    assert_eq!(squad.proto_squad_name, "test_squad");
    assert_eq!(squad.base.position, glam::Vec3::ZERO);
    assert!(squad.transport_fly_in().is_none());
    assert_eq!(squad.unit_ids.len(), 2);
}

fn entity_database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "unsc_veh_warthog_01".to_owned(),
        dbid: Some(101),
        object_class: Some("Unit".to_owned()),
        hitpoints: Some(500.0),
        ..ProtoObject::default()
    });
    database.objects.push(ProtoObject {
        name: "test_building".to_owned(),
        dbid: Some(102),
        object_class: Some("Building".to_owned()),
        hitpoints: Some(1_000.0),
        ..ProtoObject::default()
    });
    database.objects.push(ProtoObject {
        name: "test_member".to_owned(),
        dbid: Some(103),
        object_class: Some("Unit".to_owned()),
        hitpoints: Some(100.0),
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: "test_squad".to_owned(),
        dbid: Some(201),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                count: 2,
                proto_object: "test_member".to_owned(),
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    });
    database
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

fn add_guard_variables(script: &mut TriggerScript) {
    script.add_variable(integer(90, 5));
    script.add_variable(TriggerVar::new(91, VarType::Time).with_value(TriggerValue::Time(100)));
}

fn add_guard(script: &mut TriggerScript) {
    script.add_trigger(
        Trigger::new(1).starts_active().with_condition(
            Condition::new(1, ConditionType::GameTime)
                .with_input_at(1, 90)
                .with_input_at(2, 91),
        ),
    );
}

fn integer(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value))
}

fn boolean(id: u32, value: bool) -> TriggerVar {
    TriggerVar::new(id, VarType::Bool).with_value(TriggerValue::Bool(value))
}

fn player(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(value))
}

fn proto_object(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoObject).with_value(TriggerValue::ProtoObject(value))
}

fn proto_squad(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoSquad).with_value(TriggerValue::ProtoSquad(value))
}

fn proto_squad_list(id: u32, values: Vec<i32>) -> TriggerVar {
    TriggerVar::new(id, VarType::ProtoSquadList).with_value(TriggerValue::ProtoSquadList(values))
}

fn location(id: u32, x: f32, y: f32, z: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::UILocation)
        .with_value(TriggerValue::Location(TriggerVec3::new(x, y, z)))
}

fn vector(id: u32, x: f32, y: f32, z: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::Vector).with_value(TriggerValue::Vector(TriggerVec3::new(x, y, z)))
}

fn object(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Object).with_value(TriggerValue::Object(value))
}

fn unit(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(value))
}

fn squad(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(value))
}

fn empty_unit_list(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::UnitList)
        .with_value(TriggerValue::UnitList(Vec::new()))
        .as_output()
}

fn empty_squad_list(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::SquadList)
        .with_value(TriggerValue::SquadList(Vec::new()))
        .as_output()
}

fn script_entity(world: &World, script_id: u32, variable_id: u32) -> EntityId {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_entity())
        .expect("entity output should exist")
}

fn script_entities(world: &World, script_id: u32, variable_id: u32) -> Vec<EntityId> {
    let value = &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("entity-list output should exist")
        .value;
    match value {
        TriggerValue::UnitList(values)
        | TriggerValue::SquadList(values)
        | TriggerValue::ObjectList(values) => values.clone(),
        value => panic!("expected entity list, got {value:?}"),
    }
}
