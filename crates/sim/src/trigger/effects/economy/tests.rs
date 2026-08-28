use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};
use pipeline::database::hw1::gamedata::{PopsWrapper, RatesWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::{PopulationAmount, ResourceCost};
use pipeline::database::hw1::squads::{Cost as SquadCost, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::techs::{EffectsWrapper, TechCost, TechEffect};
use pipeline::database::hw1::{GameData, Squad};

#[test]
fn player_economy_resolves_layered_resource_and_rate_tables() {
    let database = database_with_tables();
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.configure_rate_slots(3);
    player.set_resource(0, 10.0);
    player.set_resource(2, 20.0);
    player.set_resource(3, 3.0);
    assert!(player.set_rate_amount(0, 4.0));
    assert!(player.set_rate_multiplier(0, 1.5));
    assert!(player.set_rate_amount(1, 2.0));
    assert!(player.set_rate_multiplier(1, 2.0));
    assert!(player.set_rate_amount(2, 1.0));
    assert!(player.set_rate_multiplier(2, 0.5));

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    for id in 2..=8 {
        add_value(&mut script, id, VarType::Float, TriggerValue::Float(-1.0));
    }
    let effect = Effect::new(1, EffectType::GetPlayerEconomy)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3)
        .with_output_at(4, 4)
        .with_output_at(5, 5)
        .with_output_at(6, 6)
        .with_output_at(7, 7)
        .with_output_at(8, 8);

    assert_eq!(
        get_player_economy(&effect, &mut script, &world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_eq!(
        floats(&script, 2..=8),
        vec![10.0, 6.0, 20.0, 4.0, 3.0, 0.5, 0.0]
    );
    assert_eq!(
        EffectType::from_u16(647),
        Some(EffectType::GetPlayerEconomy)
    );
}

#[test]
fn cost_to_float_uses_named_runtime_slots_and_all_retail_coefficients() {
    let database = database_with_tables();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Cost,
        TriggerValue::Cost(Cost::from_amounts([10.0, 99.0, 20.0, 3.0])),
    );
    for (id, value) in [(2, 2.0), (3, 3.0), (4, 4.0), (5, 0.0)] {
        add_value(&mut script, id, VarType::Float, TriggerValue::Float(value));
    }
    let effect = Effect::new(1, EffectType::CostToFloat)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 5);

    assert_eq!(
        cost_to_float(&effect, &mut script, Some(&database)),
        EffectOutcome::Applied
    );
    assert_close(float(&script, 5), 92.0);
}

#[test]
fn get_cost_honors_input_priority_and_player_squad_transforms() {
    let mut database = database_with_tables();
    database.squads = vec![
        Squad {
            name: "logical".to_owned(),
            dbid: Some(7),
            costs: vec![squad_cost("Supplies", 100.0)],
            ..Squad::default()
        },
        Squad {
            name: "upgraded".to_owned(),
            dbid: Some(8),
            costs: vec![squad_cost("Power", 60.0)],
            ..Squad::default()
        },
    ];
    database.techs.push(Tech {
        name: "upgrade".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![TechEffect {
                effect_type: "TransformProtoSquad".to_owned(),
                from_type: Some("logical".to_owned()),
                to_type: Some("upgraded".to_owned()),
                ..TechEffect::default()
            }],
        }),
        costs: vec![TechCost {
            resource_type: "LeaderPowerCharge".to_owned(),
            amount: 9.0,
        }],
        ..Tech::default()
    });
    database.objects.push(ProtoObject {
        name: "object".to_owned(),
        dbid: Some(9),
        costs: vec![ResourceCost {
            resource_type: "Other".to_owned(),
            amount: 5.0,
        }],
        ..ProtoObject::default()
    });
    let mut world = World::new();
    world.init_players(1);
    assert_eq!(world.activate_technology(1, &database, "upgrade"), Ok(true));

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(
        &mut script,
        2,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(7),
    );
    add_value(&mut script, 3, VarType::Tech, TriggerValue::Tech(0));
    add_value(
        &mut script,
        4,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(9),
    );
    add_value(
        &mut script,
        5,
        VarType::Cost,
        TriggerValue::Cost(Cost::default()),
    );
    let effect = Effect::new(1, EffectType::GetCost)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4)
        .with_output_at(5, 5);

    assert_eq!(
        get_cost(&effect, &mut script, &world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_amounts(cost(&script, 5).amounts(), [0.0, 0.0, 60.0, 0.0]);

    script.get_variable_mut(2).unwrap().is_null = true;
    assert_eq!(
        get_cost(&effect, &mut script, &world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_amounts(cost(&script, 5).amounts(), [0.0, 0.0, 0.0, 9.0]);

    script.get_variable_mut(3).unwrap().is_null = true;
    assert_eq!(
        get_cost(&effect, &mut script, &world, Some(&database)),
        EffectOutcome::Applied
    );
    assert_amounts(cost(&script, 5).amounts(), [0.0, 5.0, 0.0, 0.0]);
}

#[test]
fn get_pop_matches_version_one_unit_and_version_two_last_nonzero_rules() {
    let mut database = database_with_tables();
    database.objects.push(ProtoObject {
        name: "member".to_owned(),
        dbid: Some(21),
        population: vec![population("Unit", 1.0), population("Leader", 2.0)],
        ..ProtoObject::default()
    });
    database.squads.push(Squad {
        name: "members".to_owned(),
        dbid: Some(31),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: "member".to_owned(),
                count: 2,
                role: None,
            }],
        }),
        ..Squad::default()
    });
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::ProtoObject,
        TriggerValue::ProtoObject(21),
    );
    add_value(
        &mut script,
        2,
        VarType::ProtoSquad,
        TriggerValue::ProtoSquad(31),
    );
    add_value(&mut script, 3, VarType::Float, TriggerValue::Float(-1.0));
    add_value(&mut script, 4, VarType::Bool, TriggerValue::Bool(false));
    add_value(&mut script, 5, VarType::Integer, TriggerValue::Int(-1));
    let mut effect = Effect::new(1, EffectType::GetPop)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_output_at(3, 3)
        .with_output_at(4, 4)
        .with_output_at(5, 5);

    effect.version = 1;
    assert_eq!(
        get_pop(&effect, &mut script, Some(&database)),
        EffectOutcome::Applied
    );
    assert_close(float(&script, 3), 2.0);

    effect.version = 2;
    assert_eq!(
        get_pop(&effect, &mut script, Some(&database)),
        EffectOutcome::Applied
    );
    assert_close(float(&script, 3), 4.0);
    assert_eq!(
        script.get_variable(4).unwrap().value,
        TriggerValue::Bool(true)
    );
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(1));
}

fn database_with_tables() -> Database {
    Database {
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: ["Supplies", "Other", "Power", "LeaderPowerCharge"]
                    .into_iter()
                    .map(resource)
                    .collect(),
            }),
            rates: Some(RatesWrapper {
                entries: vec![
                    "Supplies".to_owned(),
                    "Power".to_owned(),
                    "LeaderPowerCharge".to_owned(),
                ],
            }),
            pops: Some(PopsWrapper {
                entries: vec!["Unit".to_owned(), "Leader".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn resource(name: &str) -> ResourceDef {
    ResourceDef {
        name: name.to_owned(),
        deductable: Some(true),
    }
}

fn squad_cost(resource_type: &str, amount: f32) -> SquadCost {
    SquadCost {
        resource_type: resource_type.to_owned(),
        amount,
    }
}

fn population(population_type: &str, amount: f32) -> PopulationAmount {
    PopulationAmount {
        population_type: Some(population_type.to_owned()),
        amount,
    }
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn float(script: &TriggerScript, id: u32) -> f32 {
    match script.get_variable(id).unwrap().value {
        TriggerValue::Float(value) => value,
        ref value => panic!("expected float, got {value:?}"),
    }
}

fn floats(script: &TriggerScript, ids: impl Iterator<Item = u32>) -> Vec<f32> {
    ids.map(|id| float(script, id)).collect()
}

fn cost(script: &TriggerScript, id: u32) -> Cost {
    match script.get_variable(id).unwrap().value {
        TriggerValue::Cost(value) => value,
        ref value => panic!("expected cost, got {value:?}"),
    }
}

fn assert_amounts(actual: [f32; 4], expected: [f32; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_close(actual, expected);
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}
