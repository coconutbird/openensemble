use super::*;
use crate::trigger::{EffectType, TriggerValue, TriggerVar, VarType};
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn retail_tower_wall_dbid_is_typed() {
    assert_eq!(
        EffectType::from_u16(1001),
        Some(EffectType::SetTowerWallDestination)
    );
}

#[test]
fn effect_requires_tactic_action_and_applies_retail_link_state() {
    let (mut world, source, source_unit, target, target_unit) = tower_world();
    let gameplay = tower_gameplay(true);
    let (effect, script) = tower_effect(source, target);

    assert_eq!(
        set_destination(&effect, &script, &mut world, Some(&gameplay)),
        EffectOutcome::Applied
    );
    let action = world
        .get_unit(source_unit)
        .and_then(|unit| unit.tower_wall)
        .expect("source tower action state");
    assert_eq!(action.target_squad_id(), target);
    assert_eq!(action.beam_start_position(), Vec3::new(10.0, 2.0, 20.0));
    assert_eq!(action.beam_end_position(), Vec3::new(40.0, 4.0, 60.0));
    assert_eq!(
        world.get_squad(source).unwrap().associated_wall_towers(),
        &[target]
    );
    assert!(
        world
            .get_unit(source_unit)
            .unwrap()
            .base
            .forward
            .abs_diff_eq(Vec3::new(-0.6, 0.0, -0.8), f32::EPSILON)
    );
    assert!(
        world
            .get_unit(target_unit)
            .unwrap()
            .base
            .forward
            .abs_diff_eq(Vec3::new(0.6, 0.0, 0.8), f32::EPSILON)
    );

    let no_action = tower_gameplay(false);
    let (mut untouched, source, source_unit, target, _) = tower_world();
    let (effect, script) = tower_effect(source, target);
    assert_eq!(
        set_destination(&effect, &script, &mut untouched, Some(&no_action)),
        EffectOutcome::Skipped
    );
    assert!(
        untouched
            .get_unit(source_unit)
            .unwrap()
            .tower_wall
            .is_none()
    );
    assert!(
        untouched
            .get_squad(source)
            .unwrap()
            .associated_wall_towers()
            .is_empty()
    );
}

fn tower_world() -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    let source = world.create_squad_at(0, Vec3::new(10.0, 2.0, 20.0));
    let source_unit = world.create_building_at(0, Vec3::new(10.0, 2.0, 20.0));
    world.get_unit_mut(source_unit).unwrap().proto_object_name = "wall_source".to_owned();
    assert!(world.attach_unit_to_squad(source_unit, source));
    let target = world.create_squad_at(0, Vec3::new(40.0, 4.0, 60.0));
    let target_unit = world.create_building_at(0, Vec3::new(40.0, 4.0, 60.0));
    world.get_unit_mut(target_unit).unwrap().proto_object_name = "wall_target".to_owned();
    assert!(world.attach_unit_to_squad(target_unit, target));
    (world, source, source_unit, target, target_unit)
}

fn tower_gameplay(with_action: bool) -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "wall_source".to_owned(),
        tactics: Some("wall_source.tactics".to_owned()),
        ..ProtoObject::default()
    });
    let actions = with_action.then(|| Action {
        name: "TowerWall".to_owned(),
        action_type: Some("TowerWall".to_owned()),
        ..Action::default()
    });
    GameplayCatalog::from_tactics(
        &database,
        [(
            "wall_source".to_owned(),
            TacticData {
                actions: actions.into_iter().collect(),
                ..TacticData::default()
            },
        )],
    )
}

fn tower_effect(source: EntityId, target: EntityId) -> (Effect, TriggerScript) {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Squad).with_value(TriggerValue::Squad(source)));
    script.add_variable(TriggerVar::new(2, VarType::Squad).with_value(TriggerValue::Squad(target)));
    let effect = Effect::new(1, EffectType::SetTowerWallDestination)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    (effect, script)
}
