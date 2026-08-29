//! Scenario flags needed while constructing authoritative simulation state.

use pipeline::xmb::Document;

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

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
