use super::*;

#[test]
fn loads_retail_defaults_flags_and_positive_final_counts() {
    let definitions = [
        ObjectiveDefinition {
            id: 8,
            required: true,
            players: vec![1, 5],
            score: Some(750),
            tracker_duration_ms: Some(9_000),
            min_tracker_increment: Some(2),
            final_count: Some(100),
        },
        ObjectiveDefinition {
            id: 9,
            score: Some(-1),
            ..ObjectiveDefinition::default()
        },
    ];
    let mut world = World::new();

    load_objectives(&mut world, &definitions);

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
    let plain = world.objective(9).expect("plain objective");
    assert!(!plain.required());
    assert_eq!(plain.score(), u32::MAX);
    assert_eq!(plain.final_count(), -1);
}
