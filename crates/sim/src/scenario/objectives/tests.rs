use super::*;

#[test]
fn loads_retail_defaults_flags_and_positive_final_counts() {
    let document = Document::from_xml(
        r#"<Scenario><Objectives>
            <Objective id="8">ojvCounter
                <Flag>Required</Flag><Flag>Player1</Flag><Flag>Player5</Flag>
                <Score>750</Score><TrackerDuration>9000</TrackerDuration>
                <MinTrackerIncrement>2</MinTrackerIncrement><FinalCount>100</FinalCount>
            </Objective>
            <Objective id="9">ojvNoCounter<FinalCount>0</FinalCount></Objective>
        </Objectives></Scenario>"#,
    )
    .expect("objective XML");
    let mut world = World::new();

    load_objectives(&mut world, &document).expect("objective state");

    let counter = world.objective(8).expect("counter objective");
    assert!(counter.required());
    assert!(counter.assigned_to_player(1));
    assert!(counter.assigned_to_player(5));
    assert!(!counter.assigned_to_player(2));
    assert_eq!(counter.score(), 750);
    assert_eq!(counter.tracker_duration_ms(), 9_000);
    assert_eq!(counter.min_tracker_increment(), 2);
    assert_eq!(counter.current_count(), -1);
    assert_eq!(counter.final_count(), 100);
    assert_eq!(world.objective(9).unwrap().final_count(), -1);
}

#[test]
fn rejects_missing_and_malformed_objective_ids() {
    for (xml, expected) in [
        (
            "<Scenario><Objectives><Objective /></Objectives></Scenario>",
            ObjectiveLoadError::MissingId,
        ),
        (
            "<Scenario><Objectives><Objective id=\"counter\" /></Objectives></Scenario>",
            ObjectiveLoadError::InvalidId {
                value: "counter".to_owned(),
            },
        ),
    ] {
        let document = Document::from_xml(xml).expect("objective XML");
        assert_eq!(parse_objectives(&document), Err(expected));
    }
}
