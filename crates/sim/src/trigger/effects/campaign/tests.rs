use super::*;
use crate::player::{PlayerState, PlayerType};
use crate::trigger::{TriggerVar, VarType};

fn script() -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::PlayerState).with_value(TriggerValue::Int(4)));
    script.add_variable(
        TriggerVar::new(2, VarType::String)
            .with_value(TriggerValue::String("ACH_SCN01".to_owned())),
    );
    script
}

#[test]
fn campaign_end_ids_are_typed() {
    assert_eq!(EffectType::from_u16(1002), Some(EffectType::MissionResult));
    assert_eq!(
        EffectType::from_u16(1030),
        Some(EffectType::GrantAchievement)
    );
}

#[test]
fn mission_result_is_kept_for_each_human_player_that_still_plays() {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(2).unwrap().state = PlayerState::Defeated;
    world.get_player_mut(3).unwrap().player_type = PlayerType::ComputerAi;
    let effect = Effect::new(1, EffectType::MissionResult).with_input_at(1, 1);
    let before = world.checksum();

    assert_eq!(
        execute(&effect, &script(), &mut world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(world.mission_result(1), Some(4));
    assert_eq!(world.mission_result(2), None);
    assert_eq!(world.mission_result(3), None);
    assert_ne!(world.checksum(), before);
    // Retail does not end the game: only the end screen of the user shows.
    assert!(!world.is_game_over());
}

#[test]
fn grant_achievement_keeps_each_name_once_and_null_name_is_empty() {
    let mut world = World::new();
    let named = Effect::new(1, EffectType::GrantAchievement).with_input_at(1, 2);
    let unnamed = Effect::new(2, EffectType::GrantAchievement);
    let script = script();

    for effect in [&named, &named, &unnamed] {
        assert_eq!(
            execute(effect, &script, &mut world),
            Some(EffectOutcome::Applied)
        );
    }
    assert_eq!(
        world.granted_achievements(),
        &["ACH_SCN01".to_owned(), String::new()]
    );
}
