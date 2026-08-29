//! Loading of scenario objective definitions into authoritative world state.

use crate::world::{ObjectiveState, World};
use pipeline::hw1::scenario::ObjectiveRef;
use pipeline::xmb::{Document, Node};

/// Failure while parsing an objective's sparse retail identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectiveLoadError {
    /// An objective did not provide the lowercase retail `id` attribute.
    #[error("a scenario objective is missing its id attribute")]
    MissingId,
    /// An objective ID was not a signed retail integer.
    #[error("objective ID '{value}' is not a valid integer")]
    InvalidId { value: String },
}

pub(super) fn configure_objective_references(world: &mut World, references: &[ObjectiveRef]) {
    world.configure_objectives(
        references
            .iter()
            .map(|reference| ObjectiveState::new(reference.id))
            .collect(),
    );
}

pub(super) fn load_objectives(
    world: &mut World,
    document: &Document,
) -> Result<(), ObjectiveLoadError> {
    let objectives = parse_objectives(document)?;
    world.configure_objectives(objectives);
    Ok(())
}

fn parse_objectives(document: &Document) -> Result<Vec<ObjectiveState>, ObjectiveLoadError> {
    let Some(wrapper) = document.root().and_then(|root| child(root, "Objectives")) else {
        return Ok(Vec::new());
    };
    wrapper
        .children
        .iter()
        .filter(|node| node.name == "Objective")
        .map(parse_objective)
        .collect()
}

fn parse_objective(node: &Node) -> Result<ObjectiveState, ObjectiveLoadError> {
    let attribute = node
        .get_attribute("id")
        .ok_or(ObjectiveLoadError::MissingId)?;
    let id_text = attribute.value_string();
    let objective_id = id_text
        .trim()
        .parse()
        .map_err(|_| ObjectiveLoadError::InvalidId { value: id_text })?;
    if objective_id == -1 {
        return Err(ObjectiveLoadError::MissingId);
    }

    let mut objective = ObjectiveState::new(objective_id);
    for field in &node.children {
        let text = field.text_string();
        let value = text.trim();
        match field.name.as_str() {
            "Flag" => apply_flag(&mut objective, value),
            "Score" => {
                let score = value.parse::<i32>().unwrap_or(0);
                objective.set_score(score.cast_unsigned());
            }
            "TrackerDuration" => {
                if let Ok(duration) = value.parse() {
                    objective.set_tracker_duration_ms(duration);
                }
            }
            "MinTrackerIncrement" => {
                if let Ok(increment) = value.parse() {
                    objective.set_min_tracker_increment(increment);
                }
            }
            "FinalCount" => {
                if let Ok(count) = value.parse()
                    && count > 0
                {
                    objective.set_final_count(count);
                }
            }
            _ => {}
        }
    }
    Ok(objective)
}

fn apply_flag(objective: &mut ObjectiveState, flag: &str) {
    match flag {
        "Required" => objective.set_required(true),
        "Player1" => objective.assign_player(1),
        "Player2" => objective.assign_player(2),
        "Player3" => objective.assign_player(3),
        "Player4" => objective.assign_player(4),
        "Player5" => objective.assign_player(5),
        "Player6" => objective.assign_player(6),
        _ => {}
    }
}

fn child<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children.iter().find(|child| child.name == name)
}

#[cfg(test)]
#[path = "objectives/tests.rs"]
mod tests;
