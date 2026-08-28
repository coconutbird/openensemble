use super::*;
use crate::player::PlayerState;
use crate::trigger::{EffectType, TriggerVar, VarType};

#[test]
fn team_queries_follow_world_roster_order_and_uniqueness() {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 1;
    world.get_player_mut(3).unwrap().team_id = 3;
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::TeamList,
        TriggerValue::TeamList(Vec::new()),
    );
    add_value(&mut script, 2, VarType::Team, TriggerValue::Team(1));
    add_value(
        &mut script,
        3,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );

    let get_teams_effect = Effect::new(1, EffectType::GetTeams).with_output_at(1, 1);
    let get_players_effect = Effect::new(2, EffectType::GetTeamPlayers)
        .with_input_at(1, 2)
        .with_output_at(2, 3);
    assert_eq!(
        get_teams(&get_teams_effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        get_team_players(&get_players_effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        script.get_variable(1).map(|variable| &variable.value),
        Some(&TriggerValue::TeamList(vec![0, 1, 3]))
    );
    assert_eq!(
        script.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::PlayerList(vec![1, 2]))
    );
}

#[test]
fn version_two_id_lists_clear_unique_add_remove_and_preserve_alias_quirk() {
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![7]),
    );
    add_value(&mut script, 2, VarType::Player, TriggerValue::Player(8));
    add_value(
        &mut script,
        3,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![8, 9]),
    );
    add_value(&mut script, 4, VarType::Bool, TriggerValue::Bool(false));
    add_value(
        &mut script,
        5,
        VarType::TeamList,
        TriggerValue::TeamList(vec![1, 2, 3]),
    );

    let mut add = Effect::new(1, EffectType::PlayerListAdd)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 1)
        .with_input_at(5, 4);
    add.version = 2;
    assert_eq!(
        list_add(&add, &mut script, IdListKind::Player),
        EffectOutcome::Applied
    );
    assert_eq!(player_list(&script, 1), &[7, 8, 9]);

    let mut remove = Effect::new(2, EffectType::PlayerListRemove)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    remove.version = 2;
    assert_eq!(
        list_remove(&remove, &mut script, IdListKind::Player),
        EffectOutcome::Applied
    );
    assert_eq!(player_list(&script, 1), &[7]);

    let mut alias_remove = Effect::new(3, EffectType::TeamListRemove)
        .with_input_at(1, 5)
        .with_input_at(3, 5)
        .with_input_at(4, 4);
    alias_remove.version = 2;
    assert_eq!(
        list_remove(&alias_remove, &mut script, IdListKind::Team),
        EffectOutcome::Applied
    );
    assert_eq!(team_list(&script, 5), &[2]);

    add.version = 1;
    assert_eq!(
        list_add(&add, &mut script, IdListKind::Player),
        EffectOutcome::Unsupported(EffectType::PlayerListAdd as u16)
    );
}

#[test]
fn game_time_effects_use_retail_wrapping_and_saturating_rules() {
    let mut world = World::new();
    world.game_time_ms = u32::MAX - 5;
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Time, TriggerValue::Time(10));
    add_value(&mut script, 2, VarType::Time, TriggerValue::Time(0));
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(100));
    add_value(&mut script, 4, VarType::Time, TriggerValue::Time(0));

    let current = Effect::new(1, EffectType::GetGameTime)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    assert_eq!(
        get_game_time(&current, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(time(&script, 2), 4);

    world.game_time_ms = 75;
    let remaining = Effect::new(2, EffectType::GetGameTimeRemaining)
        .with_input_at(1, 3)
        .with_output_at(2, 4);
    assert_eq!(
        get_game_time_remaining(&remaining, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(time(&script, 4), 25);
}

#[test]
fn get_players_2_filters_in_roster_order_with_retail_relation_rules() {
    let mut world = World::new();
    world.init_players(4);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.get_player_mut(3).unwrap().team_id = 1;
    world.get_player_mut(4).unwrap().team_id = 1;
    world.get_player_mut(4).unwrap().state = PlayerState::Defeated;
    world.configure_standard_team_relations();

    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![99]),
    );
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(false));
    add_value(&mut script, 3, VarType::Bool, TriggerValue::Bool(false));
    add_value(&mut script, 4, VarType::Player, TriggerValue::Player(1));
    add_value(&mut script, 5, VarType::RelationType, TriggerValue::Int(2));
    add_value(
        &mut script,
        6,
        VarType::PlayerState,
        TriggerValue::Int(PlayerState::Playing as i32),
    );
    let mut effect = Effect::new(1, EffectType::GetPlayers2)
        .with_output_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(5, 4)
        .with_input_at(6, 5)
        .with_input_at(7, 6);
    effect.version = 2;

    assert_eq!(
        get_players_2(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(player_list(&script, 1), &[3]);

    script.get_variable_mut(2).unwrap().value = TriggerValue::Bool(true);
    script.get_variable_mut(3).unwrap().value = TriggerValue::Bool(true);
    script.get_variable_mut(5).unwrap().value = TriggerValue::Int(0);
    script.get_variable_mut(6).unwrap().is_null = true;
    assert_eq!(
        get_players_2(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(player_list(&script, 1), &[0, 1, 2, 3, 4]);

    script.get_variable_mut(4).unwrap().value = TriggerValue::Player(-1);
    assert_eq!(
        get_players_2(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(player_list(&script, 1), &[0, 1, 2, 3, 4]);

    script.get_variable_mut(5).unwrap().value = TriggerValue::Int(2);
    assert_eq!(
        get_players_2(&effect, &mut script, &world),
        EffectOutcome::Applied
    );
    assert!(player_list(&script, 1).is_empty());
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn player_list(script: &TriggerScript, id: u32) -> &[i32] {
    match &script.get_variable(id).expect("player list").value {
        TriggerValue::PlayerList(values) => values,
        value => panic!("expected player list, got {value:?}"),
    }
}

fn team_list(script: &TriggerScript, id: u32) -> &[i32] {
    match &script.get_variable(id).expect("team list").value {
        TriggerValue::TeamList(values) => values,
        value => panic!("expected team list, got {value:?}"),
    }
}

fn time(script: &TriggerScript, id: u32) -> u32 {
    match script.get_variable(id).expect("time").value {
        TriggerValue::Time(value) => value,
        ref value => panic!("expected time, got {value:?}"),
    }
}
