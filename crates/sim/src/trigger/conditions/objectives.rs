//! Retail objective state queries.

use super::value_at;
use crate::trigger::{Condition, TriggerScript, TriggerValue};
use crate::world::World;

/// `IsObjectiveComplete` (746), `BTriggerCondition_evaluate` case 746.
///
/// Retail uses the objective value of slot 1 as a position in the objective list, not as an
/// objective ID. A negative or too large position gives `false`.
pub(super) fn is_objective_complete(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(TriggerValue::Objective(index)) = value_at(condition, script, 1) else {
        return false;
    };
    world
        .objective_at_index(*index)
        .is_some_and(|objective| objective.completed())
}

#[cfg(test)]
#[path = "objectives/tests.rs"]
mod tests;
