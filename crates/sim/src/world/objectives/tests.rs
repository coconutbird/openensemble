use super::*;

#[test]
fn retail_defaults_and_invalid_query_fallback_are_preserved() {
    let mut world = World::new();
    let mut objective = ObjectiveState::new(11);
    objective.set_required(true);
    objective.assign_player(1);
    objective.set_final_count(45);
    world.configure_objectives(vec![objective]);

    let state = world.objective(11).expect("configured objective");
    assert!(state.required());
    assert!(state.assigned_to_player(1));
    assert!(!state.assigned_to_player(2));
    assert_eq!(state.current_count(), -1);
    assert_eq!(state.final_count(), 45);
    assert_eq!(world.objective_current_count(999), 0);
    assert_eq!(world.objective_final_count(999), 0);
}

#[test]
fn counter_mutation_is_authoritative_and_checksummed() {
    let mut world = World::new();
    world.configure_objectives(vec![ObjectiveState::new(8)]);
    let initial = world.checksum();

    assert!(world.set_objective_current_count(8, 2));
    assert_eq!(world.objective_current_count(8), 2);
    assert_ne!(world.checksum(), initial);
    assert!(!world.set_objective_current_count(9, 3));
}
