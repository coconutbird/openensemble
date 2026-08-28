use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarType,
};
use sim::{Entity, EntityId, PopulationCost, ShieldCoverage, TriggerVec3, World};

#[test]
fn spatial_and_owner_queries_write_retail_outputs() {
    let mut world = trigger_world(2);
    let squad_id = world.create_squad_at(1, glam::Vec3::new(10.0, 2.0, 20.0));
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .base
        .set_forward(glam::Vec3::X);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, squad_id));
    script.add_variable(output_location(1));
    script.add_variable(output_vector(2));
    script.add_variable(output_vector(3));
    script.add_variable(output_vector(4));
    script.add_variable(location(5, 1.0, 0.0, 2.0));
    script.add_variable(location(6, 1.0, 0.0, 5.0));
    script.add_variable(output_vector(7));
    script.add_variable(output_player(8));

    let mut get_location = Effect::new(0, EffectType::GetLocation)
        .with_input_at(2, 0)
        .with_output_at(3, 1);
    get_location.version = 2;
    let mut get_direction = Effect::new(1, EffectType::GetDirection)
        .with_input_at(2, 0)
        .with_output_at(4, 2)
        .with_output_at(5, 3)
        .with_output_at(6, 4);
    get_direction.version = 2;
    let direction_between = Effect::new(2, EffectType::GetDirectionFromLocations)
        .with_input_at(1, 5)
        .with_input_at(2, 6)
        .with_output_at(4, 7);
    let get_owner = Effect::new(3, EffectType::GetOwner)
        .with_input_at(2, 0)
        .with_output_at(3, 8);
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(get_location)
            .with_effect_on_true(get_direction)
            .with_effect_on_true(direction_between)
            .with_effect_on_true(get_owner),
    );
    keep_script_alive(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 4);
    assert_vec3(script_vector(&world, script_id, 1), [10.0, 2.0, 20.0]);
    assert_vec3(script_vector(&world, script_id, 2), [1.0, 0.0, 0.0]);
    assert_vec3(script_vector(&world, script_id, 3), [0.0, 0.0, -1.0]);
    assert_vec3(script_vector(&world, script_id, 4), [0.0, 1.0, 0.0]);
    assert_vec3(script_vector(&world, script_id, 7), [0.0, 0.0, 1.0]);
    assert_eq!(script_player(&world, script_id, 8), 1);
}

#[test]
fn set_direction_and_direct_teleport_update_authoritative_formation_state() {
    let mut world = trigger_world(1);
    let squad_id = world.create_squad_at(1, glam::Vec3::ZERO);
    let first = world.create_unit_at(1, glam::Vec3::ZERO);
    let second = world.create_unit_at(1, glam::Vec3::ZERO);
    assert!(world.attach_unit_to_squad(first, squad_id));
    assert!(world.attach_unit_to_squad(second, squad_id));
    world.get_unit_mut(first).unwrap().formation_offset = glam::Vec3::X;
    world.get_unit_mut(second).unwrap().formation_offset = -glam::Vec3::X;

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, squad_id));
    script.add_variable(vector(1, 1.0, 0.0, 0.0));
    script.add_variable(location(2, 10.0, 0.0, 20.0));
    script.add_variable(boolean(3, true));
    let mut set_direction = Effect::new(0, EffectType::SetDirection)
        .with_input_at(3, 0)
        .with_input_at(7, 1);
    set_direction.version = 1;
    let mut teleport = Effect::new(1, EffectType::Teleport)
        .with_input_at(3, 0)
        .with_input_at(5, 2)
        .with_input_at(8, 3);
    teleport.version = 3;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(set_direction)
            .with_effect_on_true(teleport),
    );
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.base.position, glam::Vec3::new(10.0, 0.0, 20.0));
    assert_eq!(squad.base.forward, glam::Vec3::X);
    let first = world.get_unit(first).unwrap();
    let second = world.get_unit(second).unwrap();
    assert_eq!(first.base.position, glam::Vec3::new(10.0, 0.0, 19.0));
    assert_eq!(second.base.position, glam::Vec3::new(10.0, 0.0, 21.0));
    assert_eq!(first.base.forward, glam::Vec3::X);
    assert_eq!(second.base.forward, glam::Vec3::X);
}

#[test]
fn unavailable_plot_turret_and_revive_paths_do_not_partially_mutate() {
    let mut world = trigger_world(1);
    let squad_id = world.create_squad_at(1, glam::Vec3::new(2.0, 0.0, 3.0));
    let member_id = world.create_unit_at(1, glam::Vec3::new(2.0, 0.0, 3.0));
    assert!(world.attach_unit_to_squad(member_id, squad_id));
    let object_id = world.create_unit_at(1, glam::Vec3::new(4.0, 0.0, 5.0));

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, squad_id));
    script.add_variable(object(1, object_id));
    script.add_variable(location(2, 100.0, 0.0, 100.0));
    script.add_variable(boolean(3, false));
    script.add_variable(vector(4, 1.0, 0.0, 0.0));
    script.add_variable(boolean(5, true));
    script.add_variable(unit(6, member_id));
    script.add_variable(float(7, 50.0));

    let mut teleport = Effect::new(0, EffectType::Teleport)
        .with_input_at(3, 0)
        .with_input_at(5, 2)
        .with_input_at(6, 1)
        .with_input_at(8, 3);
    teleport.version = 3;
    let mut set_turret_direction = Effect::new(1, EffectType::SetDirection)
        .with_input_at(5, 1)
        .with_input_at(7, 4)
        .with_input_at(8, 5);
    set_turret_direction.version = 2;
    let mut override_revive = Effect::new(2, EffectType::CombatDamage)
        .with_input_at(3, 6)
        .with_input_at(5, 7)
        .with_input_at(7, 5);
    override_revive.version = 2;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(teleport)
            .with_effect_on_true(set_turret_direction)
            .with_effect_on_true(override_revive),
    );
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 0);
    assert_eq!(update.unsupported_effect_types, vec![336, 354, 489]);
    assert_eq!(
        world.get_squad(squad_id).unwrap().base.position,
        glam::Vec3::new(2.0, 0.0, 3.0)
    );
    let object = world.get_unit(object_id).unwrap();
    assert_eq!(object.base.position, glam::Vec3::new(4.0, 0.0, 5.0));
    assert_eq!(object.base.forward, glam::Vec3::Z);
    assert_near(world.get_unit(member_id).unwrap().hitpoints, 100.0);
}

#[test]
fn change_owner_moves_members_population_and_cap_contributions() {
    let mut world = trigger_world(2);
    for player_id in 1..=2 {
        let player = world.get_player_mut(player_id).unwrap();
        player.configure_population_slots(1);
        assert!(player.set_population_limits(0, 10.0, 20.0));
    }
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    let squad_cost = vec![PopulationCost::new(0, 2.0)];
    let unit_cost = vec![PopulationCost::new(0, 1.0)];
    let cap = vec![PopulationCost::new(0, 3.0)];
    world.get_squad_mut(squad_id).unwrap().population_costs = squad_cost.clone();
    {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.population_costs = unit_cost.clone();
        unit.population_cap_additions = cap.clone();
    }
    {
        let player = world.get_player_mut(1).unwrap();
        player.add_population(&squad_cost);
        player.add_population(&unit_cost);
        player.adjust_population_cap(&cap, true);
    }

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, squad_id));
    script.add_variable(player(1, 2));
    script.add_variable(output_player(2));
    let mut change = Effect::new(0, EffectType::ChangeOwner)
        .with_input_at(3, 1)
        .with_input_at(6, 0);
    change.version = 3;
    let get_owner = Effect::new(1, EffectType::GetOwner)
        .with_input_at(1, 3)
        .with_output_at(3, 2);
    script.add_variable(unit(3, unit_id));
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(change)
            .with_effect_on_true(get_owner),
    );
    keep_script_alive(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert_eq!(world.get_squad(squad_id).unwrap().base.player_id, 2);
    assert_eq!(world.get_unit(unit_id).unwrap().base.player_id, 2);
    assert_eq!(script_player(&world, script_id, 2), 2);
    let first = world.get_player(1).unwrap().get_population(0).unwrap();
    let second = world.get_player(2).unwrap().get_population(0).unwrap();
    assert_eq!((first.count, first.cap), (0.0, 10.0));
    assert_eq!((second.count, second.cap), (3.0, 13.0));
}

#[test]
fn direct_health_effects_keep_channels_separate_and_report_aggregates() {
    let mut world = trigger_world(1);
    let squad_id = world.create_squad(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first, squad_id));
    assert!(world.attach_unit_to_squad(second, squad_id));
    configure_health(&mut world, first, 50.0, 10.0);
    configure_health(&mut world, second, 80.0, 30.0);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, squad_id));
    script.add_variable(float(1, 40.0));
    script.add_variable(float(2, 0.2));
    script.add_variable(boolean(3, true));
    script.add_variable(float(4, 0.1));
    script.add_variable(float(5, 20.0));
    script.add_variable(unit_list(6, vec![first, second]));
    for id in 7..=10 {
        script.add_variable(output_float(id));
    }
    let damage = Effect::new(0, EffectType::Damage)
        .with_input_at(1, 0)
        .with_input_at(5, 1)
        .with_input_at(8, 2)
        .with_input_at(9, 3);
    let repair = Effect::new(1, EffectType::Repair)
        .with_input_at(4, 6)
        .with_input_at(6, 4)
        .with_input_at(7, 5)
        .with_input_at(9, 3);
    let mut get_health = Effect::new(2, EffectType::GetHealth)
        .with_input_at(1, 0)
        .with_output_at(3, 7)
        .with_output_at(4, 8)
        .with_output_at(5, 9)
        .with_output_at(6, 10);
    get_health.version = 3;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(damage)
            .with_effect_on_true(repair)
            .with_effect_on_true(get_health),
    );
    keep_script_alive(&mut script);
    let script_id = install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 3);
    assert_health(&world, first, 40.0, 10.0);
    assert_health(&world, second, 70.0, 30.0);
    assert_near(script_float(&world, script_id, 7), 110.0);
    assert_near(script_float(&world, script_id, 8), 0.55);
    assert_near(script_float(&world, script_id, 9), 40.0);
    assert_near(script_float(&world, script_id, 10), 0.4);
    assert_eq!(world.get_squad(squad_id).unwrap().last_damaged_time, 0);

    assert!(world.damage_unit_direct(first, 500.0, 0.0));
    assert_near(world.get_unit(first).unwrap().hitpoints, 0.0);
    assert!(world.get_unit(first).unwrap().is_alive());
    world.update_entities(0.05);
    assert!(world.get_unit(first).is_some());
}

#[test]
fn combat_damage_uses_shields_and_emits_the_damage_event() {
    let mut world = trigger_world(1);
    world.game_time_ms = 77;
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    configure_health(&mut world, unit_id, 20.0, 10.0);

    let mut script = TriggerScript::default();
    script.add_variable(unit(0, unit_id));
    script.add_variable(float(1, 25.0));
    let mut damage = Effect::new(0, EffectType::CombatDamage)
        .with_input_at(3, 0)
        .with_input_at(5, 1);
    damage.version = 1;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(damage));
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert_health(&world, unit_id, 5.0, 0.0);
    assert_eq!(world.get_squad(squad_id).unwrap().last_damaged_time, 77);
    assert!(world.damage_unit(unit_id, 5.0));
    assert!(!world.get_unit(unit_id).unwrap().is_alive());
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

fn keep_script_alive(script: &mut TriggerScript) {
    script.add_variable(TriggerVar::new(90, VarType::Integer).with_value(TriggerValue::Int(5)));
    script.add_variable(TriggerVar::new(91, VarType::Time).with_value(TriggerValue::Time(100)));
    script.add_trigger(
        Trigger::new(90).starts_active().with_condition(
            Condition::new(90, ConditionType::GameTime)
                .with_input_at(1, 90)
                .with_input_at(2, 91),
        ),
    );
}

fn configure_health(world: &mut World, unit_id: EntityId, hitpoints: f32, shields: f32) {
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.set_max_hitpoints(100.0);
    unit.hitpoints = hitpoints;
    unit.shields.configure(ShieldCoverage::Full, 50.0);
    unit.shields.set_current(shields);
}

fn squad(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(value))
}

fn unit(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(value))
}

fn object(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Object).with_value(TriggerValue::Object(value))
}

fn unit_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::UnitList).with_value(TriggerValue::UnitList(values))
}

fn player(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(value))
}

fn boolean(id: u32, value: bool) -> TriggerVar {
    TriggerVar::new(id, VarType::Bool).with_value(TriggerValue::Bool(value))
}

fn float(id: u32, value: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::Float).with_value(TriggerValue::Float(value))
}

fn location(id: u32, x: f32, y: f32, z: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::UILocation)
        .with_value(TriggerValue::Location(TriggerVec3::new(x, y, z)))
}

fn vector(id: u32, x: f32, y: f32, z: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::Vector).with_value(TriggerValue::Vector(TriggerVec3::new(x, y, z)))
}

fn output_location(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::UILocation)
        .with_value(TriggerValue::Location(TriggerVec3::zero()))
        .as_output()
}

fn output_vector(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Vector)
        .with_value(TriggerValue::Vector(TriggerVec3::zero()))
        .as_output()
}

fn output_float(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Float)
        .with_value(TriggerValue::Float(0.0))
        .as_output()
}

fn output_player(id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player)
        .with_value(TriggerValue::Player(-1))
        .as_output()
}

fn script_vector(world: &World, script_id: u32, variable_id: u32) -> TriggerVec3 {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_location())
        .expect("vector output")
}

fn script_float(world: &World, script_id: u32, variable_id: u32) -> f32 {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_float())
        .expect("float output")
}

fn script_player(world: &World, script_id: u32, variable_id: u32) -> i32 {
    let value = &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("player output")
        .value;
    match value {
        TriggerValue::Player(player_id) => *player_id,
        value => panic!("expected player output, got {value:?}"),
    }
}

fn assert_health(world: &World, unit_id: EntityId, hitpoints: f32, shields: f32) {
    let unit = world.get_unit(unit_id).unwrap();
    assert_near(unit.hitpoints, hitpoints);
    assert_near(unit.shields.current, shields);
}

fn assert_vec3(actual: TriggerVec3, expected: [f32; 3]) {
    assert_near(actual.x, expected[0]);
    assert_near(actual.y, expected[1]);
    assert_near(actual.z, expected[2]);
}

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs()
            <= f32::EPSILON * actual.abs().max(expected.abs()).max(1.0) * 16.0,
        "expected {expected}, got {actual}"
    );
}
