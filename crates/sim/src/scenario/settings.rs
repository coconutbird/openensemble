//! Scenario settings needed while constructing authoritative simulation state.

use pipeline::hw1::scenario::ScenarioObject;
use std::collections::BTreeMap;

/// Index the explicit visual-variation indices by scenario object ID.
///
/// Objects without an authored index are not in the map. Retail then uses
/// `-1` to request visual-logic selection. A later object with the same ID
/// replaces the value of an earlier object. Values below `-1` become `-1`.
pub(super) fn visual_variation_indices(objects: &[ScenarioObject]) -> BTreeMap<i32, i32> {
    objects
        .iter()
        .filter_map(|object| Some((object.id, object.visual_variation_index?.max(-1))))
        .collect()
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
