use super::*;
use crate::trigger::{BuildingCommandState, EffectType, TriggerVar, VarType};
use crate::world::World;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::ObjectCommand;
use pipeline::database::hw1::squads::{Cost as SquadCost, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::techs::TechCost;
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad, Tech};

const TRAINER: &str = "trigger_trainer";
const TRAINEE: &str = "trigger_trainee";
const INSTANT_TECH: &str = "trigger_instant_tech";

#[test]
fn clear_building_command_state_marks_done_and_discards_trained_squads() {
    let mut state = BuildingCommandState::default();
    state.record_trained_squad(crate::EntityId::new(
        crate::entity_id::EntityClass::Squad,
        3,
    ));
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::BuildingCommandState)
            .with_value(TriggerValue::BuildingCommandState(state)),
    );
    let effect = Effect::new(1, EffectType::ClearBuildingCommandState).with_output_at(1, 1);

    assert_eq!(
        clear_building_command_state(&effect, &mut script),
        EffectOutcome::Applied
    );
    let TriggerValue::BuildingCommandState(state) = &script.get_variable(1).unwrap().value else {
        panic!("building command state changed type");
    };
    assert!(state.is_done());
    assert!(state.trained_squads().is_empty());
}

#[test]
fn custom_command_add_v2_preserves_authoritative_ui_and_execution_fields() {
    let mut world = World::new();
    let unit_id = crate::EntityId::new(crate::entity_id::EntityClass::Unit, 7);
    let mut script = TriggerScript::new(1);
    for variable in [
        TriggerVar::new(1, VarType::Unit).with_value(TriggerValue::Unit(unit_id)),
        TriggerVar::new(2, VarType::Integer).with_value(TriggerValue::Int(3)),
        TriggerVar::new(3, VarType::String)
            .with_value(TriggerValue::String("miscicon,door".to_owned())),
        TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(0.25)),
        TriggerVar::new(5, VarType::Bool).with_value(TriggerValue::Bool(true)),
        TriggerVar::new(6, VarType::Integer).with_value(TriggerValue::Int(-1)),
    ] {
        script.add_variable(variable);
    }
    let mut effect = Effect::new(1, EffectType::CustomCommandAdd)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(5, 4)
        .with_input_at(15, 5)
        .with_output_at(17, 6);
    effect.version = 2;

    assert_eq!(
        custom_command_add(&effect, &mut script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(script.get_variable(6).unwrap().value, TriggerValue::Int(0));
    let command = world.custom_command(0).unwrap();
    assert_eq!(command.unit_id, unit_id);
    assert_eq!(command.icon_position, 3);
    assert_eq!(command.icon_name.as_deref(), Some("miscicon,door"));
    assert_close(command.timer_seconds, 0.25);
    assert!(command.flags.persistent());
}

#[test]
fn custom_command_remove_consumes_the_assigned_id() {
    let mut world = World::new();
    let id = world.add_custom_command(crate::world::CustomCommand::default());
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Integer).with_value(TriggerValue::Int(id)));
    let effect = Effect::new(1, EffectType::CustomCommandRemove).with_input_at(1, 1);

    assert_eq!(
        custom_command_remove(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert!(world.custom_command(id).is_none());
    assert_eq!(world.next_custom_command_id(), id + 1);
}

#[test]
fn building_command_no_cost_squads_complete_immediately_without_recharge() {
    let mut database = building_command_database();
    database.objects[1].population = vec![pipeline::database::hw1::objects::PopulationAmount {
        population_type: Some("Unit".to_owned()),
        amount: 1.0,
    }];
    database.game_data.as_mut().unwrap().pops =
        Some(pipeline::database::hw1::gamedata::PopsWrapper {
            entries: vec!["Unit".to_owned()],
        });
    let (mut world, building_id) = building_command_world();
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, 0.0, 0.0));
    let mut script = building_command_script(building_id);
    script.add_variable(
        TriggerVar::new(2, VarType::ProtoSquad).with_value(TriggerValue::ProtoSquad(0)),
    );
    script.add_variable(TriggerVar::new(3, VarType::Integer).with_value(TriggerValue::Int(2)));
    script.add_variable(TriggerVar::new(4, VarType::Bool).with_value(TriggerValue::Bool(true)));
    let mut effect = Effect::new(1, EffectType::BuildingCommand)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 5);
    effect.version = 4;

    assert_eq!(
        building_command(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let state = command_state(&script);
    assert!(state.is_done());
    assert_eq!(state.trained_squads().len(), 2);
    assert!(
        state
            .trained_squads()
            .iter()
            .all(|squad_id| world.get_squad(*squad_id).is_some())
    );
    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert!(
        world
            .training_recharge(building_id, crate::entities::TrainingKind::Squad, 0)
            .is_none()
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 0.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 2.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
}

#[test]
fn instant_recharge_training_finishes_trigger_state_without_queueing() {
    let mut database = building_command_database();
    database.squads[0]
        .flags
        .push("InstantTrainWithRecharge".to_owned());
    database.squads[0].build_points = Some(3.0);
    let (mut world, building_id) = building_command_world();
    let mut script = building_command_script(building_id);
    script.add_variable(
        TriggerVar::new(2, VarType::ProtoSquad).with_value(TriggerValue::ProtoSquad(0)),
    );
    script.add_variable(TriggerVar::new(3, VarType::Integer).with_value(TriggerValue::Int(2)));
    script.add_variable(TriggerVar::new(4, VarType::Bool).with_value(TriggerValue::Bool(true)));
    let mut effect = Effect::new(1, EffectType::BuildingCommand)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 5);
    effect.version = 4;

    assert_eq!(
        building_command(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let state = command_state(&script);
    assert!(state.is_done());
    assert_eq!(state.trained_squads().len(), 2);
    assert!(
        state
            .trained_squads()
            .iter()
            .all(|squad_id| world.get_squad(*squad_id).is_some())
    );
    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert_close(
        world
            .training_recharge(building_id, crate::entities::TrainingKind::Squad, 0)
            .unwrap()
            .time_remaining(),
        3.0,
    );
}

#[test]
fn instant_trigger_research_finishes_state_without_queueing() {
    let database = building_command_database();
    let (mut world, building_id) = building_command_world();
    let mut script = building_command_script(building_id);
    script.add_variable(TriggerVar::new(4, VarType::Bool).with_value(TriggerValue::Bool(true)));
    script.add_variable(TriggerVar::new(6, VarType::Tech).with_value(TriggerValue::Tech(0)));
    let mut effect = Effect::new(1, EffectType::BuildingCommand)
        .with_input_at(1, 1)
        .with_input_at(4, 4)
        .with_input_at(6, 6)
        .with_output_at(5, 5);
    effect.version = 4;

    assert_eq!(
        building_command(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );

    assert!(command_state(&script).is_done());
    assert!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .is_active(INSTANT_TECH)
    );
    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 0.0);
}

#[test]
fn no_cost_trigger_research_finishes_immediately_without_instant_flag() {
    let mut database = building_command_database();
    database.techs[0].flags.clear();
    let (mut world, building_id) = building_command_world();
    let mut script = building_command_script(building_id);
    script.add_variable(TriggerVar::new(4, VarType::Bool).with_value(TriggerValue::Bool(true)));
    script.add_variable(TriggerVar::new(6, VarType::Tech).with_value(TriggerValue::Tech(0)));
    let mut effect = Effect::new(1, EffectType::BuildingCommand)
        .with_input_at(1, 1)
        .with_input_at(4, 4)
        .with_input_at(6, 6)
        .with_output_at(5, 5);
    effect.version = 4;

    assert_eq!(
        building_command(&effect, &mut script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    assert!(command_state(&script).is_done());
    assert!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .is_active(INSTANT_TECH)
    );
    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 0.0);
}

fn building_command_world() -> (World, crate::EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let building_id = world.create_building(1);
    world
        .get_building_mut(building_id)
        .unwrap()
        .proto_object_name = TRAINER.to_owned();
    (world, building_id)
}

fn building_command_script(building_id: crate::EntityId) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Unit).with_value(TriggerValue::Unit(building_id)),
    );
    script.add_variable(
        TriggerVar::new(5, VarType::BuildingCommandState).with_value(
            TriggerValue::BuildingCommandState(BuildingCommandState::default()),
        ),
    );
    script
}

fn command_state(script: &TriggerScript) -> &BuildingCommandState {
    let TriggerValue::BuildingCommandState(state) = &script.get_variable(5).unwrap().value else {
        panic!("building command state changed type");
    };
    state
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}

fn building_command_database() -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: TRAINER.to_owned(),
                object_class: Some("Building".to_owned()),
                commands: vec![
                    ObjectCommand {
                        target: TRAINEE.to_owned(),
                        command_type: Some("TrainSquad".to_owned()),
                        ..ObjectCommand::default()
                    },
                    ObjectCommand {
                        target: INSTANT_TECH.to_owned(),
                        command_type: Some("Research".to_owned()),
                        ..ObjectCommand::default()
                    },
                ],
                ..ProtoObject::default()
            },
            ProtoObject {
                name: TRAINEE.to_owned(),
                object_class: Some("Unit".to_owned()),
                ..ProtoObject::default()
            },
        ],
        squads: vec![ProtoSquad {
            name: TRAINEE.to_owned(),
            build_points: Some(0.05),
            costs: vec![SquadCost {
                resource_type: "Supplies".to_owned(),
                amount: 100.0,
            }],
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: TRAINEE.to_owned(),
                    count: 1,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        }],
        techs: vec![Tech {
            name: INSTANT_TECH.to_owned(),
            research_points: Some(10.0),
            costs: vec![TechCost {
                resource_type: "Supplies".to_owned(),
                amount: 50.0,
            }],
            flags: vec!["Instant".to_owned()],
            ..Tech::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    deductable: Some(true),
                }],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}
