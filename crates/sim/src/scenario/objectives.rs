//! Application of scenario objective definitions to authoritative world state.

use crate::world::{ObjectiveState, World};
use pipeline::hw1::scenario::{ObjectiveDefinition, ObjectiveRef};

pub(super) fn configure_objective_references(world: &mut World, references: &[ObjectiveRef]) {
    world.configure_objectives(
        references
            .iter()
            .map(|reference| ObjectiveState::new(reference.id))
            .collect(),
    );
}

/// Replace the world objectives with the decoded SCN objective definitions.
pub(super) fn load_objectives(world: &mut World, definitions: &[ObjectiveDefinition]) {
    world.configure_objectives(definitions.iter().map(objective_state).collect());
}

fn objective_state(definition: &ObjectiveDefinition) -> ObjectiveState {
    let mut objective = ObjectiveState::new(definition.id);
    if definition.required {
        objective.set_required(true);
    }
    for &player in &definition.players {
        objective.assign_player(player);
    }
    if let Some(score) = definition.score {
        objective.set_score(score.cast_unsigned());
    }
    if let Some(duration) = definition.tracker_duration_ms {
        objective.set_tracker_duration_ms(duration);
    }
    if let Some(increment) = definition.min_tracker_increment {
        objective.set_min_tracker_increment(increment);
    }
    if let Some(count) = definition.final_count {
        objective.set_final_count(count);
    }
    objective
}

#[cfg(test)]
#[path = "objectives/tests.rs"]
mod tests;
