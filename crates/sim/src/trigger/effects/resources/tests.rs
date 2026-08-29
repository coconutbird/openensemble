use super::*;
use crate::player::{PlayerState, PopulationCost};
use crate::trigger::{EffectType, TriggerCost, TriggerVar, VarType};
use pipeline::database::hw1::gamedata::PopsWrapper;
use pipeline::database::hw1::{Database, GameData};

#[test]
fn set_trickle_rate_unions_player_inputs_and_accrues_per_second() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(
        TriggerVar::new(2, VarType::PlayerList).with_value(TriggerValue::PlayerList(vec![1, 2])),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::Cost).with_value(TriggerValue::Cost(TriggerCost {
            supplies: 4.0,
            power: 2.0,
            population: 0.0,
            resource_3: 0.0,
        })),
    );
    let effect = Effect::new(1, EffectType::SetTrickleRate)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);

    assert_eq!(
        set_trickle_rate(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    world.update_player_resources(0.25);

    for player_id in [1, 2] {
        let player = world.get_player(player_id).unwrap();
        assert_close(player.resources.get(0), 1.0);
        assert_close(player.resources.get(1), 0.5);
        assert_close(player.total_resources.get(0), 1.0);
    }
}

#[test]
fn stopped_player_does_not_receive_trickle_and_get_always_writes() {
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().state = PlayerState::Defeated;
    world
        .get_player_mut(1)
        .unwrap()
        .set_resource_trickle_rate(Resources {
            amounts: [8.0, 0.0, 0.0, 0.0],
        });
    world.update_player_resources(1.0);
    assert_close(world.get_player(1).unwrap().resources.get(0), 0.0);

    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(
        TriggerVar::new(2, VarType::Cost).with_value(TriggerValue::Cost(TriggerCost::default())),
    );
    let effect = Effect::new(1, EffectType::GetTrickleRate)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    assert_eq!(
        get_trickle_rate(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::Cost(TriggerCost {
            supplies: 8.0,
            power: 0.0,
            population: 0.0,
            resource_3: 0.0,
        })
    );
}

#[test]
fn get_player_pop_resolves_unit_slot_from_scenario_database() {
    let database = Database {
        game_data: Some(GameData {
            pops: Some(PopsWrapper {
                entries: vec!["Leader".to_owned(), "Unit".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(2);
    assert!(player.set_population_limits(1, 12.0, 20.0));
    player.add_population(&[PopulationCost::new(1, 5.0)]);
    assert!(player.reserve_population(&[PopulationCost::new(1, 3.0)]));

    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Player).with_value(TriggerValue::Player(1)));
    for id in 2..=5 {
        script.add_variable(
            TriggerVar::new(id, VarType::Float).with_value(TriggerValue::Float(-1.0)),
        );
    }
    let effect = Effect::new(1, EffectType::GetPlayerPop)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3)
        .with_output_at(4, 4)
        .with_output_at(5, 5);

    assert_eq!(
        get_player_pop(&effect, &mut script, &world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_close(float(&script, 2), 8.0);
    assert_close(float(&script, 3), 12.0);
    assert_close(float(&script, 4), 3.0);
    assert_close(float(&script, 5), 5.0);
}

#[test]
fn set_player_pop_writes_only_used_unit_population_fields_without_clamping() {
    let database = Database {
        game_data: Some(GameData {
            pops: Some(PopsWrapper {
                entries: vec!["Leader".to_owned(), "Unit".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(2);
    assert!(player.set_population_limits(1, 12.0, 20.0));
    player.add_population(&[PopulationCost::new(1, 5.0)]);

    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(TriggerVar::new(2, VarType::Float).with_value(TriggerValue::Float(30.0)));
    script.add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(25.0)));
    script.add_variable(
        TriggerVar::new(4, VarType::Float)
            .with_value(TriggerValue::Float(9.0))
            .null(),
    );
    script.add_variable(TriggerVar::new(5, VarType::Float).with_value(TriggerValue::Float(7.0)));
    let effect = Effect::new(1, EffectType::SetPlayerPop)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_input_at(5, 5);

    assert_eq!(
        set_player_pop(&effect, &script, &mut world, Some(&database)),
        EffectOutcome::Applied
    );
    let population = world.get_player(1).unwrap().get_population(1).unwrap();
    assert_close(population.cap, 30.0);
    assert_close(population.max, 25.0);
    assert_close(population.future, 0.0);
    assert_close(population.count, 7.0);
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}

fn float(script: &TriggerScript, id: u32) -> f32 {
    match script.get_variable(id).expect("float").value {
        TriggerValue::Float(value) => value,
        ref value => panic!("expected float, got {value:?}"),
    }
}
