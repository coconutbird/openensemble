use super::*;
use crate::trigger::{ConditionType, TriggerVar, VarType};
use pipeline::database::hw1::GameData;

#[test]
fn check_difficulty_uses_player_scalar_database_thresholds_and_versions() {
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().difficulty = 0.6;
    let mut database = Database::new();
    database.game_data = Some(GameData {
        difficulty_normal: Some(0.5),
        difficulty_hard: Some(0.75),
        difficulty_legendary: Some(1.0),
        ..GameData::default()
    });
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add_value(&mut script, 2, VarType::Difficulty, TriggerValue::Int(1));
    add_value(&mut script, 3, VarType::Operator, TriggerValue::Int(4));
    add_value(&mut script, 4, VarType::Difficulty, TriggerValue::Int(0));

    let mut version_one = Condition::new(1, ConditionType::CheckDifficulty)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    version_one.version = 1;
    assert!(check_difficulty(
        &version_one,
        &script,
        &world,
        Some(&database)
    ));

    let mut version_two = Condition::new(2, ConditionType::CheckDifficulty)
        .with_input_at(1, 1)
        .with_input_at(2, 4)
        .with_input_at(3, 3);
    version_two.version = 2;
    assert!(check_difficulty(
        &version_two,
        &script,
        &world,
        Some(&database)
    ));

    version_two.version = 3;
    assert!(!check_difficulty(
        &version_two,
        &script,
        &world,
        Some(&database)
    ));
}

#[test]
fn game_setting_conditions_use_checksummed_world_configuration() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::String,
        TriggerValue::String("cAmPaIgNdEbUg".to_owned()),
    );
    add_value(
        &mut script,
        2,
        VarType::String,
        TriggerValue::String("Café".to_owned()),
    );
    let defined = Condition::new(1, ConditionType::IsConfigDefined).with_input_at(1, 1);
    let non_ansi = Condition::new(2, ConditionType::IsConfigDefined).with_input_at(1, 2);

    assert!(!is_coop(&world));
    assert!(!is_config_defined(&defined, &script, &world));
    let initial_checksum = world.checksum();

    world.set_coop(true);
    assert!(world.define_config("CampaignDebug"));
    assert!(is_coop(&world));
    assert!(is_config_defined(&defined, &script, &world));
    assert!(!is_config_defined(&non_ansi, &script, &world));
    assert_ne!(world.checksum(), initial_checksum);
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn setting_database() -> Database {
    use pipeline::database::hw1::AIDifficultySetting;
    use pipeline::database::hw1::aidifficultysettings::{AIDifficultyDefaultValue, PiecewiseFunc};
    let point = |input, output| PiecewiseDataPoint { input, output };
    let mut database = Database::new();
    database.ai_difficulty_settings = vec![AIDifficultySetting {
        name: "EconMultiplier".to_owned(),
        default_values: vec![AIDifficultyDefaultValue {
            name: "SPCEasy".to_owned(),
            value: 9.0,
        }],
        function: Some(PiecewiseFunc {
            points: vec![point(0.0, 1.0), point(0.5, 2.0), point(1.0, 4.0)],
        }),
        ..AIDifficultySetting::default()
    }];
    database
}

/// `BTriggerCondition_tcAICanGetDifficultySetting`: a null signature 3 evaluates the setting
/// function at the player difficulty; a named signature 3 reads the `DefaultValue`. A human
/// player has no `BAI`, and an unknown setting or value gives false.
#[test]
fn ai_can_get_difficulty_setting_reads_function_or_default() {
    let database = setting_database();
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(2).unwrap().player_type = PlayerType::ComputerAi;
    world.get_player_mut(2).unwrap().difficulty = 0.75;
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Player, TriggerValue::Player(2));
    add_value(
        &mut script,
        2,
        VarType::String,
        TriggerValue::String("econmultiplier".to_owned()),
    );
    script.add_variable(TriggerVar::new(3, VarType::String).null());
    add_value(&mut script, 4, VarType::Float, TriggerValue::Float(0.0));
    add_value(
        &mut script,
        5,
        VarType::String,
        TriggerValue::String("spceasy".to_owned()),
    );
    let condition = |default_id| {
        Condition::new(1, ConditionType::AICanGetDifficultySetting)
            .with_input_at(1, 1)
            .with_input_at(2, 2)
            .with_input_at(3, default_id)
            .with_output_at(4, 4)
    };
    let output = |script: &TriggerScript| script.get_variable(4).unwrap().value.clone();

    assert!(ai_can_get_difficulty_setting(
        &condition(3),
        &mut script,
        &world,
        Some(&database)
    ));
    assert_eq!(output(&script), TriggerValue::Float(3.0));
    assert!(ai_can_get_difficulty_setting(
        &condition(5),
        &mut script,
        &world,
        Some(&database)
    ));
    assert_eq!(output(&script), TriggerValue::Float(9.0));

    script.get_variable_mut(5).unwrap().value = TriggerValue::String("Missing".to_owned());
    assert!(!ai_can_get_difficulty_setting(
        &condition(5),
        &mut script,
        &world,
        Some(&database)
    ));
    script.get_variable_mut(2).unwrap().value = TriggerValue::String("Missing".to_owned());
    assert!(!ai_can_get_difficulty_setting(
        &condition(3),
        &mut script,
        &world,
        Some(&database)
    ));
    script.get_variable_mut(2).unwrap().value = TriggerValue::String("EconMultiplier".to_owned());
    script.get_variable_mut(1).unwrap().value = TriggerValue::Player(1);
    assert!(!ai_can_get_difficulty_setting(
        &condition(3),
        &mut script,
        &world,
        Some(&database)
    ));
}

/// `BPiecewiseFunc_evaluate` clamps outside the points and interpolates inside. A function with
/// fewer than two points, a decreasing input or three equal inputs gives 0.
#[test]
fn piecewise_value_clamps_interpolates_and_rejects_invalid_points() {
    let point = |input, output| PiecewiseDataPoint { input, output };
    let points = [
        point(0.2, 1.0),
        point(0.4, 3.0),
        point(0.4, 5.0),
        point(1.0, 6.0),
    ];
    assert_close(piecewise_value(&points, 0.0), 1.0);
    assert_close(piecewise_value(&points, 0.3), 2.0);
    assert_close(piecewise_value(&points, 0.4), 3.0);
    assert_close(piecewise_value(&points, 0.7), 5.5);
    assert_close(piecewise_value(&points, 2.0), 6.0);
    assert_close(piecewise_value(&points[..1], 0.5), 0.0);
    assert_close(
        piecewise_value(&[point(1.0, 1.0), point(0.0, 2.0)], 0.5),
        0.0,
    );
    let triple = [point(0.0, 1.0), point(0.0, 2.0), point(0.0, 3.0)];
    assert_close(piecewise_value(&triple, 0.0), 0.0);
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-6, "{actual} != {expected}");
}
