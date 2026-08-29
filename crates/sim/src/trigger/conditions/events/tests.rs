use super::*;
use crate::trigger::{ConditionType, TriggerVar, VarType};
use crate::world::ScreenFadeSequence;
use crate::world::{ChatRequest, CinematicRequest, GeneralEvent, GeneralEventType};

#[test]
fn event_triggered_tracks_normal_and_counted_subscribers() {
    let mut world = World::new();
    let normal_id = world.subscribe_general_event(GeneralEventType::CommandBowl, Some(1), false);
    let counted_id = world.subscribe_general_event(GeneralEventType::GameEntityKilled, None, true);
    let mut script = TriggerScript::new(1);
    add_int(&mut script, 1, normal_id);
    add_int(&mut script, 2, counted_id);
    let normal = Condition::new(1, ConditionType::EventTriggered).with_input_at(1, 1);
    let counted = Condition::new(2, ConditionType::EventTriggered).with_input_at(1, 2);

    assert!(!event_triggered(&normal, &script, &world));
    assert_eq!(
        world.fire_general_event(&GeneralEvent::new(GeneralEventType::CommandBowl, 2)),
        0
    );
    assert_eq!(
        world.fire_general_event(&GeneralEvent::new(GeneralEventType::CommandBowl, 1)),
        1
    );
    assert!(event_triggered(&normal, &script, &world));

    world.fire_general_event(&GeneralEvent::new(GeneralEventType::GameEntityKilled, 1));
    world.fire_general_event(&GeneralEvent::new(GeneralEventType::GameEntityKilled, 1));
    assert!(event_triggered(&counted, &script, &world));
    assert_eq!(world.general_event_fire_count(counted_id), 2);
    world.reset_general_event(counted_id, true);
    assert_eq!(world.general_event_fire_count(counted_id), 1);
}

#[test]
fn presentation_conditions_wait_for_acknowledgement_and_chat_delay() {
    let mut world = World::new();
    let chat_id = world.request_chat(ChatRequest {
        id: 0,
        sound_cue: "line".to_owned(),
        queue_sound: false,
        string_id: 7,
        duration_ms: 1_000,
        talking_head_id: None,
    });
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Time).with_value(TriggerValue::Time(500)));
    let mut chat = Condition::new(1, ConditionType::ChatCompleted).with_input_at(2, 1);
    chat.version = 2;
    assert!(!chat_completed(&chat, &script, &world));
    assert!(world.acknowledge_presentation(chat_id, 1));
    assert!(!chat_completed(&chat, &script, &world));
    world.advance_time(500);
    assert!(chat_completed(&chat, &script, &world));

    let cinematic_id = world
        .request_cinematic(CinematicRequest {
            id: 0,
            cinematic_id: 2,
            possessed_squads: Vec::new(),
            pre_rendered: false,
        })
        .unwrap();
    assert!(!cinematic_completed(&world));
    assert!(world.acknowledge_presentation(cinematic_id, 1));
    assert!(cinematic_completed(&world));
}

#[test]
fn fade_completed_waits_for_authoritative_transition_time() {
    let mut world = World::new();
    world.start_screen_fade(
        [0, 0, 0],
        ScreenFadeSequence::ToColor {
            duration_ms: 100,
            fade_in: false,
        },
    );
    assert!(!fade_completed(&world));
    world.advance_time(99);
    assert!(!fade_completed(&world));
    world.advance_time(1);
    assert!(fade_completed(&world));
}

fn add_int(script: &mut TriggerScript, id: u32, value: u32) {
    script.add_variable(
        TriggerVar::new(id, VarType::Integer)
            .with_value(TriggerValue::Int(i32::from_ne_bytes(value.to_ne_bytes()))),
    );
}
