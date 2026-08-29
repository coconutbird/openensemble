use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarType,
};
use sim::{EntityId, GameplayCatalog, SquadContainmentState, TriggerVec3, UnitGarrison, World};

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
fn work_prefers_live_entity_targets_and_accepts_v4_command_ability() {
    let mut world = trigger_world(1);
    let mover = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let targeted_worker = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let ability_worker = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let target_position = glam::Vec3::new(10.0, 0.0, 0.0);
    let target = world.create_unit_at(1, target_position);
    let target_location = glam::Vec3::new(50.0, 0.0, 50.0);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, mover));
    script.add_variable(squad(1, targeted_worker));
    script.add_variable(unit(2, target));
    script.add_variable(location(3, target_location));
    script.add_variable(boolean(4, true));
    script.add_variable(boolean(5, false));
    script.add_variable(squad(6, ability_worker));
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
        .with_input_at(1, 6)
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

    assert_eq!(update.effects_applied, 3);
    assert!(update.unsupported_effect_types.is_empty());
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(target_location)
    );
    assert_eq!(
        world.get_squad(targeted_worker).unwrap().move_target,
        Some(target_position)
    );
    assert_eq!(
        world.get_squad(ability_worker).unwrap().move_target,
        Some(target_location)
    );
}

#[test]
fn work_resolves_a_scenario_unit_reference_to_authoritative_garrison_state() {
    let mut world = trigger_world(1);
    let worker = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    let (container_squad, container_unit) =
        squad_with_unit(&mut world, 0, glam::Vec3::new(8.0, 0.0, 0.0));
    world.get_unit_mut(container_unit).unwrap().garrison =
        UnitGarrison::container(0.0, false, false, Vec::new());

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, worker));
    // Scenario proto-object placements are represented by their synthetic
    // squad while retail Unit variables still refer to the placed object.
    script.add_variable(unit(1, container_squad));
    script.add_variable(boolean(2, false));
    add_guard_variables(&mut script);
    let mut effect = Effect::new(0, EffectType::Work)
        .with_input_at(1, 0)
        .with_input_at(3, 1)
        .with_input_at(6, 2)
        .with_input_at(7, 2)
        .with_input_at(8, 2);
    effect.version = 4;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(effect));
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers();

    assert_eq!(update.effects_applied, 1);
    assert!(update.unsupported_effect_types.is_empty());
    assert_eq!(
        world.get_squad(worker).unwrap().garrison.state(),
        SquadContainmentState::Garrisoning {
            target: container_unit,
            range: 0.0,
            started_at_ms: 0,
        }
    );
}

#[test]
fn work_v4_command_ability_selects_a_tactic_attack() {
    use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule, Weapon};
    use pipeline::database::hw1::{Ability, Database, ProtoObject};

    let mut world = trigger_world(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let (attacker, attacker_unit) = squad_with_unit(&mut world, 1, glam::Vec3::ZERO);
    let (target, target_unit) = squad_with_unit(&mut world, 2, glam::Vec3::new(10.0, 0.0, 0.0));
    world.get_unit_mut(attacker_unit).unwrap().proto_object_name = "attacker".to_owned();
    world.get_unit_mut(target_unit).unwrap().proto_object_name = "target".to_owned();

    let mut database = Database::new();
    database.abilities.push(Ability {
        name: "Command".to_owned(),
        ..Ability::default()
    });
    database.objects.extend([
        ProtoObject {
            name: "attacker".to_owned(),
            tactics: Some("attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![Weapon {
            name: "CommandWeapon".to_owned(),
            max_range: Some(15.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "CommandAttack".to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some("CommandWeapon".to_owned()),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                action: Some("CommandAttack".to_owned()),
                ability: Some("Command".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("attacker".to_owned(), tactics)]);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, attacker));
    script.add_variable(unit(1, target));
    script.add_variable(boolean(2, false));
    script.add_variable(boolean(3, true));
    add_guard_variables(&mut script);
    let mut effect = Effect::new(0, EffectType::Work)
        .with_input_at(1, 0)
        .with_input_at(3, 1)
        .with_input_at(6, 2)
        .with_input_at(7, 2)
        .with_input_at(8, 3);
    effect.version = 4;
    script.add_trigger(Trigger::new(0).starts_active().with_effect_on_true(effect));
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers_with_gameplay(&database, &gameplay);

    assert_eq!(update.effects_applied, 1);
    assert!(update.unsupported_effect_types.is_empty());
    let squad = world.get_squad(attacker).unwrap();
    assert_eq!(squad.attack_target, Some(target));
    assert_eq!(squad.attack_ability_id, Some(0));
}

#[test]
fn move_entity_target_snapshots_the_live_unit_position() {
    let mut world = trigger_world(1);
    let mover = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    world.get_squad_mut(mover).unwrap().speed = 0.0;
    let target = world.create_unit_at(1, glam::Vec3::new(10.0, 0.0, 4.0));

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, mover));
    script.add_variable(unit(1, target));
    script.add_variable(location(2, glam::Vec3::new(99.0, 0.0, 99.0)));
    script.add_variable(boolean(3, false));
    add_guard_variables(&mut script);
    let mut move_effect = Effect::new(0, EffectType::Move)
        .with_input_at(1, 0)
        .with_input_at(2, 2)
        .with_input_at(6, 1)
        .with_input_at(8, 3)
        .with_input_at(9, 3);
    move_effect.version = 6;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(move_effect),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    assert_eq!(world.update_triggers().effects_applied, 1);
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(glam::Vec3::new(10.0, 0.0, 4.0))
    );

    world.get_unit_mut(target).unwrap().base.position = glam::Vec3::new(15.0, 0.0, 7.0);
    world.update_entities(0.05);
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(glam::Vec3::new(10.0, 0.0, 4.0))
    );

    assert!(world.remove_unit(target).is_some());
    world.update_entities(0.05);
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(glam::Vec3::new(10.0, 0.0, 4.0))
    );
}

#[test]
fn queued_move_waits_for_the_active_move_destination() {
    let mut world = trigger_world(1);
    let mover = squad_with_unit(&mut world, 1, glam::Vec3::ZERO).0;
    world.get_squad_mut(mover).unwrap().speed = 10.0;
    let first_target = glam::Vec3::X;
    let second_target = glam::Vec3::new(3.0, 0.0, 0.0);

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, mover));
    script.add_variable(location(1, first_target));
    script.add_variable(location(2, second_target));
    script.add_variable(boolean(3, false));
    script.add_variable(boolean(4, true));
    add_guard_variables(&mut script);
    let mut first = Effect::new(0, EffectType::Move)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(8, 3)
        .with_input_at(9, 3);
    first.version = 6;
    let mut queued = Effect::new(1, EffectType::Move)
        .with_input_at(1, 0)
        .with_input_at(2, 2)
        .with_input_at(8, 3)
        .with_input_at(9, 4);
    queued.version = 6;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(first)
            .with_effect_on_true(queued),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    assert_eq!(world.update_triggers().effects_applied, 2);
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(first_target)
    );

    world.update_entities(0.1);
    assert_eq!(
        world.get_squad(mover).unwrap().move_target,
        Some(second_target)
    );
    world.update_entities(0.2);
    assert_eq!(world.get_squad(mover).unwrap().move_target, None);
    assert_eq!(world.get_squad(mover).unwrap().position(), second_target);
}

#[test]
fn move_path_uses_retail_closest_segment_and_reverse_movement() {
    let mut world = trigger_world(1);
    let (mover, member) = squad_with_unit(&mut world, 1, glam::Vec3::new(14.0, 0.0, 0.0));
    let path = vec![
        glam::Vec3::ZERO,
        glam::Vec3::new(10.0, 0.0, 0.0),
        glam::Vec3::new(20.0, 0.0, 0.0),
    ];

    let mut script = TriggerScript::default();
    script.add_variable(squad(0, mover));
    script.add_variable(location_list(1, path));
    script.add_variable(boolean(2, false));
    script.add_variable(boolean(3, true));
    add_guard_variables(&mut script);
    let mut move_path = Effect::new(0, EffectType::MovePath)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_input_at(4, 2)
        .with_input_at(5, 2)
        .with_input_at(6, 3)
        .with_input_at(7, 3);
    move_path.version = 3;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(move_path),
    );
    add_guard(&mut script);
    install_script(&mut world, script);

    let update = world.update_triggers();
    assert_eq!(update.effects_applied, 1);
    assert!(update.unsupported_effect_types.is_empty());
    let closest_segment_point = glam::Vec3::new(13.0, 0.0, 0.0);
    let squad = world.get_squad(mover).unwrap();
    assert_eq!(squad.move_target, Some(closest_segment_point));
    assert!(squad.is_reverse_moving());
    assert!(world.get_unit(member).unwrap().is_reverse_moving());

    world.update_entities(0.05);
    let squad = world.get_squad(mover).unwrap();
    assert!(squad.position().x < 14.0);
    assert!(squad.base.forward.x > 0.0);
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

fn location_list(id: u32, values: Vec<glam::Vec3>) -> TriggerVar {
    TriggerVar::new(id, VarType::VectorList).with_value(TriggerValue::VectorList(
        values
            .into_iter()
            .map(|value| TriggerVec3::new(value.x, value.y, value.z))
            .collect(),
    ))
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
