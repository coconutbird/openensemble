use super::*;

#[test]
fn retail_event_ids_and_names_are_exact() {
    assert_eq!(GeneralEventType::GameEntityBuilt as u16, 57);
    assert_eq!(GeneralEventType::ChatCompleted as u16, 61);
    assert_eq!(GeneralEventType::CinematicCompleted as u16, 77);
    assert_eq!(
        GeneralEventType::from_name("SelectUnits"),
        Some(GeneralEventType::SelectUnits)
    );
    assert_eq!(GeneralEventType::SelectUnits.name(), "SelectUnits");
    assert_eq!(GeneralEventType::from_name("selectunits"), None);
}

#[test]
fn camera_filters_and_presentation_requests_are_checksummed() {
    let mut world = World::new();
    let subscriber = world.subscribe_general_event(GeneralEventType::CameraLookingAt, None, false);
    assert!(world.add_general_event_camera_filter(
        subscriber,
        10.0,
        Some(Vec3::new(5.0, 0.0, 5.0)),
        None,
        false,
    ));
    let filtered_checksum = world.checksum();
    assert_eq!(
        world.fire_general_event(
            &GeneralEvent::new(GeneralEventType::CameraLookingAt, 1).with_camera_focus(Vec3::ZERO),
        ),
        1
    );
    assert_ne!(world.checksum(), filtered_checksum);

    let fired_checksum = world.checksum();
    world.request_chat(ChatRequest {
        id: 0,
        sound_cue: "cue".to_owned(),
        queue_sound: false,
        string_id: 1,
        duration_ms: 100,
        talking_head_id: None,
    });
    assert_ne!(world.checksum(), fired_checksum);
}
