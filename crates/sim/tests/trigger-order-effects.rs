use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarType,
};
use sim::{EntityId, SquadContainmentState, TriggerVec3, UnitGarrison, World};

#[test]
fn move_and_work_location_paths_update_authoritative_squad_orders() {
    let mut world = trigger_world(2);
    let first = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let second = squad_with_unit(&mut world, 2, glam::Vec3::ZERO).0;
    let worker = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let stale_squad = world.create_squad(1);
    assert!(world.remove_squad(stale_squad).is_some());
    let stale_target = world.create_unit(1);
    assert!(world.remove_unit(stale_target).is_some());

    let move_target = glam::Vec3::new(20.0, 0.0, 30.0);
    let work_target = glam::Vec3::new(-5.0, 0.0, 8.0);
    let mut script = TriggerScript::default();
    script.add_variable(squad(0, first));
    script.add_variable(squad_list(1, vec![first, stale_squad, second]));
    script.add_variable(unit(2, stale_target));
    script.add_variable(location(3, move_target));
    script.add_variable(boolean(4, false));
    script.add_variable(squad(5, worker));
    script.add_variable(location(6, work_target));
    add_guard_variables(&mut script);

    let mut move_effect = Effect::new(0, EffectType::Move)
        .with_input_at(1, 0)
        .with_input_at(2, 3)
        .with_input_at(5, 1)
        .with_input_at(6, 2)
        .with_input_at(8, 4)
        .with_input_at(9, 4);
    move_effect.version = 6;
    let mut work_effect = Effect::new(1, EffectType::Work)
        .with_input_at(1, 5)
        .with_input_at(4, 6)
        .with_input_at(6, 4)
        .with_input_at(7, 4)
        .with_input_at(8, 4);
    work_effect.version = 4;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(move_effect)
            .with_effect_on_true(work_effect),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 2);
    assert_eq!(
        world.get_squad(first).unwrap().move_target,
        Some(move_target)
    );
    assert_eq!(
        world.get_squad(second).unwrap().move_target,
        Some(move_target)
    );
    assert_eq!(
        world.get_squad(worker).unwrap().move_target,
        Some(work_target)
    );
}

#[test]
fn unsupported_target_flags_and_ability_paths_are_atomic() {
    let mut world = trigger_world(1);
    let mover = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let worker = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let target = world.create_unit_at(1, glam::Vec3::new(10.0, 0.0, 0.0));
    let target_location = glam::Vec3::new(50.0, 0.0, 50.0);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, mover));
    script.add_variable(squad(1, worker));
    script.add_variable(unit(2, target));
    script.add_variable(location(3, target_location));
    script.add_variable(boolean(4, true));
    script.add_variable(boolean(5, false));
    add_guard_variables(&mut script);

    let mut attack_move = Effect::new(0, EffectType::Move)
        .with_input_at(1, 0)
        .with_input_at(2, 3)
        .with_input_at(8, 4)
        .with_input_at(9, 5);
    attack_move.version = 6;
    let mut targeted_work = Effect::new(1, EffectType::Work)
        .with_input_at(1, 1)
        .with_input_at(3, 2)
        .with_input_at(4, 3)
        .with_input_at(6, 5)
        .with_input_at(7, 5)
        .with_input_at(8, 5);
    targeted_work.version = 4;
    let mut ability_work = Effect::new(2, EffectType::Work)
        .with_input_at(1, 1)
        .with_input_at(4, 3)
        .with_input_at(6, 5)
        .with_input_at(7, 5)
        .with_input_at(8, 4);
    ability_work.version = 4;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(attack_move)
            .with_effect_on_true(targeted_work)
            .with_effect_on_true(ability_work),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 0);
    assert_eq!(update.unsupported_effect_types, vec![66, 117]);
    assert_eq!(world.get_squad(mover).unwrap().move_target, None);
    assert_eq!(world.get_squad(worker).unwrap().move_target, None);
}

#[test]
fn unload_v4_filters_passengers_and_v3_unloads_the_remainder() {
    let (mut world, container, first, second) = garrisoned_world();

    let mut selective = TriggerScript::default();
    selective.add_variable(squad(0, container));
    selective.add_variable(squad(1, first));
    add_guard_variables(&mut selective);
    let mut unload = Effect::new(0, EffectType::Unload)
        .with_input_at(3, 0)
        .with_input_at(6, 1);
    unload.version = 4;
    selective.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(unload));
    add_guard(&mut selective);
    install_script(&mut world, selective);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert!(matches!(
        world.get_squad(first).unwrap().garrison.state(),
        SquadContainmentState::Ungarrisoning { .. }
    ));
    assert!(matches!(
        world.get_squad(second).unwrap().garrison.state(),
        SquadContainmentState::Garrisoned { .. }
    ));
    advance(&mut world);
    assert_eq!(
        world.get_squad(first).unwrap().garrison.state(),
        SquadContainmentState::Free
    );

    let mut unload_all = TriggerScript::default();
    unload_all.add_variable(squad_list(0, vec![container]));
    add_guard_variables(&mut unload_all);
    let mut unload = Effect::new(0, EffectType::Unload).with_input_at(5, 0);
    unload.version = 3;
    unload_all.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(unload));
    add_guard(&mut unload_all);
    install_script(&mut world, unload_all);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert!(matches!(
        world.get_squad(second).unwrap().garrison.state(),
        SquadContainmentState::Ungarrisoning { .. }
    ));
}

fn garrisoned_world() -> (World, EntityId, EntityId, EntityId) {
    let mut world = trigger_world(2);
    let (container_squad, container_unit) = squad_with_unit(&mut world, 0, glam::Vec3::ZERO);
    world.get_unit_mut(container_unit).unwrap().garrison =
        UnitGarrison::container(0.0, false, false, Vec::new());
    let first = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let second = squad_with_unit(&mut world, 2, glam::Vec3::ZERO).0;
    world
        .issue_garrison_order(1, first, container_squad, 0.0)
        .expect("first passenger order");
    world
        .issue_garrison_order(2, second, container_squad, 0.0)
        .expect("second passenger order");
    advance(&mut world);
    assert!(matches!(
        world.get_squad(first).unwrap().garrison.state(),
        SquadContainmentState::Garrisoned { .. }
    ));
    assert!(matches!(
        world.get_squad(second).unwrap().garrison.state(),
        SquadContainmentState::Garrisoned { .. }
    ));
    (world, container_squad, first, second)
}

fn squad_with_unit(world: &mut World, player_id: u8, position: glam::Vec3) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

fn trigger_world(players: u8) -> World {
    let mut world = World::new();
    world.init_players(players);
    world
}

fn advance(world: &mut World) {
    world.advance_time(50);
    world.update_entities(0.05);
}

fn install_script(world: &mut World, script: TriggerScript) {
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);
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

fn integer(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value))
}

fn location(id: u32, value: glam::Vec3) -> TriggerVar {
    TriggerVar::new(id, VarType::UILocation).with_value(TriggerValue::Location(TriggerVec3::new(
        value.x, value.y, value.z,
    )))
}

fn squad(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(value))
}

fn squad_list(id: u32, values: Vec<EntityId>) -> TriggerVar {
    TriggerVar::new(id, VarType::SquadList).with_value(TriggerValue::SquadList(values))
}

fn unit(id: u32, value: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(value))
}
