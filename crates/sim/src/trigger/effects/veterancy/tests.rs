use super::*;
use crate::entities::{Squad, Unit};
use crate::trigger::{EffectType, TriggerValue, TriggerVar, VarType};
use pipeline::database::hw1::objects::VeterancyLevel;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn retail_id_scalar_precedence_and_duplicate_list_entries_are_preserved() {
    assert_eq!(EffectType::from_u16(852), Some(EffectType::AddXP));
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(&database, []);
    let (mut world, first, second) = world();
    let mut script = TriggerScript::new(1);
    script.add_variable(variable(1, VarType::Squad, TriggerValue::Squad(first)));
    script.add_variable(variable(
        2,
        VarType::SquadList,
        TriggerValue::SquadList(vec![second, second]),
    ));
    script.add_variable(variable(3, VarType::Float, TriggerValue::Float(11.0)));
    let scalar = Effect::new(1, EffectType::AddXP)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);

    assert_eq!(
        add_experience(&scalar, &script, &mut world, Some(&gameplay)),
        EffectOutcome::Applied
    );
    assert_close(world.get_squad(first).unwrap().experience(), 11.0);
    assert_close(world.get_squad(second).unwrap().experience(), 0.0);
    assert_eq!(world.get_squad(first).unwrap().veterancy_level(), 1);

    let list = Effect::new(2, EffectType::AddXP)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    assert_eq!(
        add_experience(&list, &script, &mut world, Some(&gameplay)),
        EffectOutcome::Applied
    );
    assert_close(world.get_squad(second).unwrap().experience(), 22.0);
    assert_eq!(world.get_squad(second).unwrap().veterancy_level(), 1);
}

#[test]
fn add_xp_is_applied_but_cannot_mutate_a_veterancy_disabled_world() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(&database, []);
    let (mut world, first, _) = world();
    world.set_veterancy_enabled(false);
    let mut script = TriggerScript::new(1);
    script.add_variable(variable(1, VarType::Squad, TriggerValue::Squad(first)));
    script.add_variable(variable(3, VarType::Float, TriggerValue::Float(11.0)));
    let effect = Effect::new(1, EffectType::AddXP)
        .with_input_at(1, 1)
        .with_input_at(3, 3);

    assert_eq!(
        add_experience(&effect, &script, &mut world, Some(&gameplay)),
        EffectOutcome::Applied
    );
    let squad = world.get_squad(first).unwrap();
    assert_close(squad.experience(), 0.0);
    assert_close(squad.banked_experience(), 0.0);
    assert_eq!(squad.veterancy_level(), 0);
}

fn world() -> (World, crate::EntityId, crate::EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let first = add_squad(&mut world);
    let second = add_squad(&mut world);
    (world, first, second)
}

fn add_squad(world: &mut World) -> crate::EntityId {
    let squad_id = world.squads.allocate_id();
    let mut squad = Squad::new(squad_id, 1);
    squad.proto_squad_name = "test_squad".to_owned();
    world.squads.insert(squad_id, squad);
    let unit_id = world.units.allocate_id();
    let mut unit = Unit::new(unit_id, 1);
    unit.proto_object_name = "test_unit".to_owned();
    world.units.insert(unit_id, unit);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "test_unit".to_owned(),
            veterancy: vec![VeterancyLevel {
                level: 1,
                xp: Some(10.0),
                damage: Some(2.0),
                ..VeterancyLevel::default()
            }],
            ..ProtoObject::default()
        }],
        squads: vec![ProtoSquad {
            name: "test_squad".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "test_unit".to_owned(),
                    count: 1,
                    ..UnitEntry::default()
                }],
            }),
            ..ProtoSquad::default()
        }],
        ..Database::default()
    }
}

fn variable(id: u32, var_type: VarType, value: TriggerValue) -> TriggerVar {
    TriggerVar::new(id, var_type).with_value(value)
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
