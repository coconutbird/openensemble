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
        TriggerValue::String("CampaignDebug".to_owned()),
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
