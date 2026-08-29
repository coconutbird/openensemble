use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use glam::Vec3;

#[test]
fn version_two_sets_global_rally_and_get_follows_entity_target() {
    let mut world = World::new();
    world.init_players(1);
    let target = world.create_unit_at(1, Vec3::new(10.0, 0.0, 20.0));
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(1.0, 2.0, 3.0)),
    );
    add_null_unit(&mut script, 3);
    add_value(&mut script, 4, VarType::Unit, TriggerValue::Unit(target));
    let mut set_effect = Effect::new(1, EffectType::RallyPointSet)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    set_effect.version = 2;

    assert_eq!(
        set(&set_effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(
        world.player_rally_point(1).unwrap().target_entity_id(),
        Some(target)
    );

    world.get_unit_mut(target).unwrap().base.position = Vec3::new(30.0, 0.0, 40.0);
    let mut get_script = TriggerScript::new(2);
    add_value(
        &mut get_script,
        10,
        VarType::Player,
        TriggerValue::Player(1),
    );
    add_value(
        &mut get_script,
        11,
        VarType::Bool,
        TriggerValue::Bool(false),
    );
    add_value(
        &mut get_script,
        12,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    let get_effect = Effect::new(2, EffectType::RallyPointGet)
        .with_input_at(1, 10)
        .with_output_at(2, 11)
        .with_output_at(3, 12);
    assert_eq!(
        get(&get_effect, &mut get_script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        get_script.get_variable(11).unwrap().value,
        TriggerValue::Bool(true)
    );
    assert_eq!(
        get_script.get_variable(12).unwrap().value,
        TriggerValue::Vector(TriggerVec3::new(30.0, 0.0, 40.0))
    );
}

#[test]
fn version_two_base_slot_sets_and_clears_primary_unit_rally() {
    let mut world = World::new();
    world.init_players(2);
    let building = world.create_building(2);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(15.0, 0.0, 25.0)),
    );
    add_value(&mut script, 3, VarType::Unit, TriggerValue::Unit(building));
    add_null_unit(&mut script, 4);
    let mut set_effect = Effect::new(1, EffectType::RallyPointSet)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    set_effect.version = 2;

    assert_eq!(
        set(&set_effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!(world.unit_rally_point(building, 1).is_none());
    assert_eq!(
        world
            .unit_rally_point(building, 2)
            .map(crate::RallyPoint::position),
        Some(Vec3::new(15.0, 0.0, 25.0))
    );

    let mut clear_effect = Effect::new(2, EffectType::RallyPointClear)
        .with_input_at(1, 1)
        .with_input_at(2, 3);
    clear_effect.version = 2;
    assert_eq!(
        clear(&clear_effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!(world.unit_rally_point(building, 2).is_none());
}

#[test]
fn version_one_clear_and_empty_get_use_global_state() {
    let mut world = World::new();
    world.init_players(1);
    assert!(world.set_player_rally_point(1, Vec3::new(4.0, 0.0, 8.0), None));
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    let mut clear_effect = Effect::new(1, EffectType::RallyPointClear).with_input_at(1, 1);
    clear_effect.version = 1;
    assert_eq!(
        clear(&clear_effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!(world.player_rally_point(1).is_none());

    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add_value(
        &mut script,
        3,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::new(1.0, 1.0, 1.0)),
    );
    let get_effect = Effect::new(2, EffectType::RallyPointGet)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3);
    assert_eq!(
        get(&get_effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Bool(false)
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::Vector(TriggerVec3::zero())
    );
}

#[test]
fn exact_dbids_and_versions_are_enforced() {
    assert_eq!(EffectType::from_u16(717), Some(EffectType::RallyPointSet));
    assert_eq!(EffectType::from_u16(718), Some(EffectType::RallyPointClear));
    assert_eq!(EffectType::from_u16(719), Some(EffectType::RallyPointGet));

    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(TriggerVec3::zero()),
    );
    let mut effect = Effect::new(1, EffectType::RallyPointSet)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    effect.version = 3;
    assert_eq!(
        set(&effect, &script, &mut world),
        EffectOutcome::Unsupported(717)
    );
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn add_null_unit(script: &mut TriggerScript, id: u32) {
    let mut variable =
        TriggerVar::new(id, VarType::Unit).with_value(TriggerValue::Unit(EntityId::INVALID));
    variable.is_null = true;
    script.add_variable(variable);
}
