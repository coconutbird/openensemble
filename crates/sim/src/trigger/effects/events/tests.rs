use super::*;
use crate::trigger::{TriggerVar, VarType};
use crate::world::{GeneralEvent, PresentationRequest};

#[test]
fn retail_subscribe_filter_and_count_reset_flow() {
    let mut world = World::new();
    world.init_players(1);
    let source = world.create_unit(1);
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::EventType, TriggerValue::Int(58));
    add_value(&mut script, 2, VarType::Integer, TriggerValue::Int(0));
    add_value(
        &mut script,
        3,
        VarType::EntityFilterSet,
        TriggerValue::EntityFilterSet(EntityFilterSet::default()),
    );
    let subscribe = Effect::new(1, EffectType::EventSubscribeUseCount)
        .with_input_at(1, 1)
        .with_output_at(2, 2);
    assert_eq!(
        super::subscribe(&subscribe, &mut script, &mut world, true),
        EffectOutcome::Applied
    );
    let filter = Effect::new(2, EffectType::EventFilterEntity)
        .with_input_at(1, 2)
        .with_input_at(2, 3);
    assert_eq!(
        filter_entity(&filter, &script, &mut world),
        EffectOutcome::Applied
    );
    let subscriber_id = subscriber_id_at(&filter, &script, 1).unwrap();

    assert_eq!(
        world.fire_general_event(
            &GeneralEvent::new(GeneralEventType::GameEntityKilled, 1)
                .with_entities(Some(source), None),
        ),
        1
    );
    assert_eq!(world.general_event_fire_count(subscriber_id), 1);
    let reset_effect = Effect::new(3, EffectType::EventReset).with_input_at(1, 2);
    assert_eq!(
        reset(&reset_effect, &script, &mut world, true),
        EffectOutcome::Applied
    );
    assert_eq!(world.general_event_fire_count(subscriber_id), 0);
}

#[test]
fn play_chat_and_cinematic_create_ui_requests_without_auto_completion() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::Sound,
        TriggerValue::String("play_line".to_owned()),
    );
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add_value(&mut script, 3, VarType::LocStringID, TriggerValue::Int(42));
    add_value(&mut script, 4, VarType::Time, TriggerValue::Time(3_000));
    add_value(&mut script, 5, VarType::TalkingHead, TriggerValue::Int(9));
    add_value(&mut script, 6, VarType::Cinematic, TriggerValue::Int(4));

    let mut chat = Effect::new(1, EffectType::PlayChat)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(6, 4)
        .with_input_at(7, 5);
    chat.version = 4;
    assert_eq!(
        play_chat(&chat, &script, &mut world),
        EffectOutcome::Presentation
    );

    let mut cinematic = Effect::new(2, EffectType::LaunchCinematic).with_input_at(1, 6);
    cinematic.version = 4;
    assert_eq!(
        launch_cinematic(&cinematic, &mut script, &mut world),
        EffectOutcome::Presentation
    );
    let requests = world.presentation_requests().collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert!(matches!(requests[0], PresentationRequest::Chat(_)));
    assert!(matches!(requests[1], PresentationRequest::Cinematic(_)));
    assert_eq!(
        launch_cinematic(&cinematic, &mut script, &mut world),
        EffectOutcome::Skipped
    );
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
