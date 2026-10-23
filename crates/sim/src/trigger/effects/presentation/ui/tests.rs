use super::super::execute;
use crate::player::TeamRelation;
use crate::trigger::effects::EffectOutcome;
use crate::trigger::{
    Effect, EffectType, TriggerColor, TriggerScript, TriggerValue, TriggerVar, VarType,
};
use crate::world::World;

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn run(effect: &Effect, script: &mut TriggerScript, world: &mut World) -> Option<EffectOutcome> {
    execute(effect, script, world)
}

#[test]
fn ui_presentation_ids_are_typed() {
    let expected = [
        (132, EffectType::FlareMinimapSpoof),
        (135, EffectType::FlareMinimapNormal),
        (164, EffectType::UserMessage),
        (726, EffectType::UITogglePowerOverlay),
        (805, EffectType::HintMessageShow),
        (815, EffectType::HintMessageDestroy),
        (923, EffectType::FlashUIElement),
        (1006, EffectType::EnableMusicManager),
        (1049, EffectType::ChatForceSubtitles),
    ];
    for (raw_type, effect_type) in expected {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
    }
}

#[test]
fn hint_message_show_uses_whole_seconds_and_destroy_v2_matches_the_id() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![2]),
    );
    add(&mut script, 2, VarType::LocStringID, TriggerValue::Int(500));
    add(&mut script, 3, VarType::Time, TriggerValue::Time(4_999));
    add(&mut script, 4, VarType::LocStringID, TriggerValue::Int(501));
    let show = Effect::new(1, EffectType::HintMessageShow)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    assert_eq!(
        run(&show, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(world.hint_message(1).is_none());
    let hint = world.hint_message(2).unwrap();
    assert_eq!(hint.string_id(), 500);
    assert!((hint.duration_s() - 4.0).abs() < f32::EPSILON);

    let mut destroy = Effect::new(1, EffectType::HintMessageDestroy).with_input_at(1, 4);
    destroy.version = 2;
    let _ = run(&destroy, &mut script, &mut world);
    assert!(world.hint_message(2).is_some());
    destroy.version = 1;
    let _ = run(&destroy, &mut script, &mut world);
    assert!(world.hint_message(2).is_none());
    destroy.version = 3;
    assert_eq!(
        run(&destroy, &mut script, &mut world),
        Some(EffectOutcome::Unsupported(815))
    );
}

#[test]
fn normal_flare_goes_to_allies_and_spoof_flare_to_the_listed_players() {
    let mut world = World::new();
    world.init_players(3);
    for (player_id, team_id) in [(1, 1), (2, 1), (3, 2)] {
        world.get_player_mut(player_id).unwrap().team_id = team_id;
    }
    for from in 0..3 {
        for to in 0..3 {
            let relation = if from == to {
                TeamRelation::Ally
            } else {
                TeamRelation::Enemy
            };
            assert!(world.set_team_relation(from, to, relation));
        }
    }
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Player, TriggerValue::Player(1));
    add(
        &mut script,
        2,
        VarType::Vector,
        TriggerValue::Vector(crate::trigger::value::Vec3::new(1.0, 2.0, 3.0)),
    );
    add(&mut script, 3, VarType::Player, TriggerValue::Player(3));
    add(
        &mut script,
        4,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![3, 2]),
    );
    add(&mut script, 5, VarType::Player, TriggerValue::Player(7));
    let normal = Effect::new(1, EffectType::FlareMinimapNormal)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    let _ = run(&normal, &mut script, &mut world);
    assert_eq!(world.minimap_flares(1).count(), 1);
    assert_eq!(world.minimap_flares(2).count(), 1);
    assert_eq!(world.minimap_flares(3).count(), 0);

    let spoof = Effect::new(1, EffectType::FlareMinimapSpoof)
        .with_input_at(1, 2)
        .with_input_at(2, 5)
        .with_input_at(3, 3)
        .with_input_at(4, 4);
    let _ = run(&spoof, &mut script, &mut world);
    assert_eq!(world.minimap_flares(3).count(), 1);
    assert_eq!(world.minimap_flares(3).next().unwrap().from_player(), 7);
    assert_eq!(world.minimap_flares(2).count(), 2);
    assert_eq!(world.minimap_flares(1).count(), 1);
}

#[test]
fn flash_ui_element_runs_only_v2_and_ignores_the_dpad_items() {
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::FlashableUIItem,
        TriggerValue::Int(18),
    );
    add(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add(&mut script, 3, VarType::Player, TriggerValue::Player(1));
    add(
        &mut script,
        4,
        VarType::FlashableUIItem,
        TriggerValue::Int(13),
    );
    let mut effect = Effect::new(1, EffectType::FlashUIElement)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    effect.version = 2;
    let _ = run(&effect, &mut script, &mut world);
    assert_eq!(
        world.player_presentation_state(1).flashing_ui_items,
        1 << 18
    );
    let mut dpad = Effect::new(1, EffectType::FlashUIElement)
        .with_input_at(1, 4)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    dpad.version = 2;
    let _ = run(&dpad, &mut script, &mut world);
    assert_eq!(
        world.player_presentation_state(1).flashing_ui_items,
        1 << 18
    );
    effect.version = 1;
    assert_eq!(
        run(&effect, &mut script, &mut world),
        Some(EffectOutcome::Unsupported(923))
    );
}

#[test]
fn user_message_v3_reads_retail_slots_and_keeps_unused_values() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1]),
    );
    add(
        &mut script,
        2,
        VarType::String,
        TriggerValue::String("Hold".to_owned()),
    );
    add(&mut script, 3, VarType::MessageIndex, TriggerValue::Int(4));
    add(
        &mut script,
        4,
        VarType::MessagePoint,
        TriggerValue::Float(100.0),
    );
    add(
        &mut script,
        6,
        VarType::MessageJustify,
        TriggerValue::Int(1),
    );
    add(
        &mut script,
        7,
        VarType::MessagePoint,
        TriggerValue::Float(12.0),
    );
    add(
        &mut script,
        9,
        VarType::Color,
        TriggerValue::Color(TriggerColor::new(1, 2, 3, 255)),
    );
    add(&mut script, 10, VarType::Bool, TriggerValue::Bool(true));
    add(&mut script, 11, VarType::Player, TriggerValue::Player(2));
    let mut effect = Effect::new(1, EffectType::UserMessage);
    for slot in [1_u16, 2, 3, 4, 6, 7, 9, 10, 11] {
        effect = effect.with_input_at(slot, u32::from(slot));
    }
    effect.version = 3;
    assert_eq!(
        run(&effect, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    let slot = world.user_message(4).unwrap();
    assert_eq!(slot.text(), "Hold");
    assert!(slot.shown_to(1) && slot.shown_to(2));
    assert_eq!(slot.position(), [100.0, 0.0]);
    assert_eq!(slot.justify(), crate::world::UserMessageJustify::Center);
    assert!((slot.scale() - 0.5).abs() < f32::EPSILON);
    assert!((slot.alpha() - 1.0).abs() < f32::EPSILON);
    assert_eq!(slot.color(), [1, 2, 3]);
    assert!(slot.enabled());

    effect.version = 2;
    assert_eq!(
        run(&effect, &mut script, &mut world),
        Some(EffectOutcome::Unsupported(164))
    );
}

#[test]
fn power_overlay_off_cancels_and_global_flags_follow_slot_one() {
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Bool, TriggerValue::Bool(false));
    add(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add(&mut script, 3, VarType::Player, TriggerValue::Player(1));
    let on = Effect::new(1, EffectType::UITogglePowerOverlay)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    let _ = run(&on, &mut script, &mut world);
    assert_eq!(
        world
            .player_presentation_state(1)
            .power_targeting_cancel_revision,
        0
    );
    let off = Effect::new(1, EffectType::UITogglePowerOverlay)
        .with_input_at(2, 1)
        .with_input_at(3, 3);
    let _ = run(&off, &mut script, &mut world);
    assert_eq!(
        world
            .player_presentation_state(1)
            .power_targeting_cancel_revision,
        1
    );

    let music = Effect::new(1, EffectType::EnableMusicManager).with_input_at(1, 1);
    let _ = run(&music, &mut script, &mut world);
    assert!(!world.music_manager_enabled());
    let subtitles = Effect::new(1, EffectType::ChatForceSubtitles).with_input_at(1, 2);
    let _ = run(&subtitles, &mut script, &mut world);
    assert!(world.chat_subtitles_forced());
}
