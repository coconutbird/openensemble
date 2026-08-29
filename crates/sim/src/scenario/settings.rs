//! Scenario flags needed while constructing authoritative simulation state.

use pipeline::xmb::Document;
use std::collections::BTreeMap;

/// Read retail's root-level `AllowVeterancy` flag.
///
/// `BScenario` starts disabled. Finding the element changes the temporary
/// parse default to true before reading its optional boolean text.
pub(super) fn allows_veterancy(document: &Document) -> bool {
    let Some(flag) = document.root().and_then(|root| {
        root.children
            .iter()
            .find(|child| child.name == "AllowVeterancy")
    }) else {
        return false;
    };
    let text = flag.text_string();
    let value = text.trim();
    if value.is_empty() {
        return true;
    }
    !value.eq_ignore_ascii_case("false") && value != "0"
}

/// Read explicitly authored visual-variation indices by scenario object ID.
///
/// The pipeline representation defaults a missing attribute to zero, while
/// retail uses `-1` to request visual-logic selection. Reading the raw tree
/// keeps that distinction intact for installed scenario loading.
pub(super) fn visual_variation_indices(document: &Document) -> BTreeMap<i32, i32> {
    let Some(objects) = document
        .root()
        .and_then(|root| root.children.iter().find(|child| child.name == "Objects"))
    else {
        return BTreeMap::new();
    };
    objects
        .children
        .iter()
        .filter(|child| child.name == "Object")
        .filter_map(|object| {
            let id = integer_attribute(object, "ID")?;
            let variation = integer_attribute(object, "VisualVariationIndex")?;
            Some((id, variation.max(-1)))
        })
        .collect()
}

fn integer_attribute(node: &pipeline::xmb::Node, name: &str) -> Option<i32> {
    node.get_attribute(name)?.value_string().trim().parse().ok()
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
