use super::*;
use crate::player::{PlayerStateChange, PlayerStateEvent};
use crate::trigger::{TriggerVar, VarType};

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn versioned(mut effect: Effect, version: u8) -> Effect {
    effect.version = version;
    effect
}

fn players_value(script: &TriggerScript, id: u32) -> TriggerValue {
    script.get_variable(id).unwrap().value.clone()
}

/// Case `0x82`: version 2 reads the player from signature 1 and the state from signature 3.
/// The world queues the change with retail world event 54.
#[test]
fn set_player_state_v2_uses_signatures_one_and_three() {
    let mut world = World::new();
    world.init_players(2);
    world.game_time_ms = 3_000;
    let mut script = TriggerScript::new(0);
    add(&mut script, 0, VarType::Player, TriggerValue::Player(2));
    add(&mut script, 1, VarType::PlayerState, TriggerValue::Int(0));
    add(&mut script, 2, VarType::PlayerState, TriggerValue::Int(2));
    let effect = versioned(
        Effect::new(0, EffectType::SetPlayerState)
            .with_input_at(1, 0)
            .with_input_at(2, 1)
            .with_input_at(3, 2),
        2,
    );

    assert_eq!(
        set_player_state(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    assert_eq!(world.get_player(2).unwrap().state, PlayerState::Defeated);
    assert_eq!(world.get_player(2).unwrap().end_time_seconds(), Some(3));
    assert_eq!(
        world.take_player_state_changes(),
        vec![PlayerStateChange {
            player_id: 2,
            previous: PlayerState::Playing,
            state: PlayerState::Defeated,
            event: Some(PlayerStateEvent::Defeated),
        }]
    );

    let version_one = versioned(effect, 1);
    assert_eq!(
        set_player_state(&version_one, &script, &mut world),
        EffectOutcome::Unsupported(130)
    );
}

/// `sub_14045B6E0`: signature 2 includes Gaia, a non-null signature 4 filters the state.
#[test]
fn get_players_v2_filters_gaia_and_optional_state() {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(2).unwrap().state = PlayerState::Defeated;
    let mut script = TriggerScript::new(0);
    add(
        &mut script,
        0,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![9]),
    );
    add(&mut script, 1, VarType::Bool, TriggerValue::Bool(false));
    add(&mut script, 2, VarType::PlayerState, TriggerValue::Int(0));
    script.add_variable(TriggerVar::new(3, VarType::PlayerState).null());
    add(&mut script, 4, VarType::Bool, TriggerValue::Bool(true));

    let playing = versioned(
        Effect::new(0, EffectType::GetPlayers)
            .with_output_at(1, 0)
            .with_input_at(2, 1)
            .with_input_at(4, 2),
        2,
    );
    assert_eq!(
        get_players(&playing, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        players_value(&script, 0),
        TriggerValue::PlayerList(vec![1, 3])
    );

    let everyone = versioned(
        Effect::new(1, EffectType::GetPlayers)
            .with_output_at(1, 0)
            .with_input_at(2, 4)
            .with_input_at(4, 3),
        2,
    );
    assert_eq!(
        get_players(&everyone, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        players_value(&script, 0),
        TriggerValue::PlayerList(vec![0, 1, 2, 3])
    );
}

/// `BTriggerEffect_tePlayerListGetSize`: signature 2 gets the size of signature 1.
#[test]
fn player_list_get_size_writes_the_count() {
    let mut script = TriggerScript::new(0);
    add(
        &mut script,
        0,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1, 4, 2]),
    );
    add(&mut script, 1, VarType::Integer, TriggerValue::Int(0));
    let effect = Effect::new(0, EffectType::PlayerListGetSize)
        .with_input_at(1, 0)
        .with_output_at(2, 1);
    assert_eq!(
        player_list_get_size(&effect, &mut script),
        EffectOutcome::Applied
    );
    assert_eq!(players_value(&script, 1), TriggerValue::Int(3));
}

/// `BTriggerEffect_teGetNPCPlayersByName` keeps NPC players (type 0) whose name matches without
/// case. A non-null signature 2 filters the state.
#[test]
fn get_npc_players_by_name_matches_npc_type_and_name() {
    let mut world = World::new();
    world.init_players(3);
    for (player_id, name, player_type) in [
        (1, "Creeps", PlayerType::ComputerAi),
        (2, "creeps", PlayerType::Npc),
        (3, "Creeps2", PlayerType::Npc),
    ] {
        let player = world.get_player_mut(player_id).unwrap();
        player.name = name.to_owned();
        player.player_type = player_type;
    }
    let mut script = TriggerScript::new(0);
    add(
        &mut script,
        0,
        VarType::String,
        TriggerValue::String("Creeps".to_owned()),
    );
    script.add_variable(TriggerVar::new(1, VarType::PlayerState).null());
    add(
        &mut script,
        2,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );
    add(&mut script, 3, VarType::PlayerState, TriggerValue::Int(2));
    let any_state = Effect::new(0, EffectType::GetNPCPlayersByName)
        .with_input_at(1, 0)
        .with_input_at(2, 1)
        .with_output_at(3, 2);
    assert_eq!(
        get_npc_players_by_name(&any_state, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(players_value(&script, 2), TriggerValue::PlayerList(vec![2]));

    let defeated = Effect::new(1, EffectType::GetNPCPlayersByName)
        .with_input_at(1, 0)
        .with_input_at(2, 3)
        .with_output_at(3, 2);
    assert_eq!(
        get_npc_players_by_name(&defeated, &mut script, &world),
        EffectOutcome::Applied
    );
    assert_eq!(
        players_value(&script, 2),
        TriggerValue::PlayerList(Vec::new())
    );
}
