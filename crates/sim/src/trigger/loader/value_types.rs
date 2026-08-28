//! Parsers for authored trigger value types with compound wire formats.

use super::Vec3;

pub(super) fn parse_list_position(text: &str) -> i32 {
    match text.trim() {
        value if value.eq_ignore_ascii_case("Last") => 1,
        value if value.eq_ignore_ascii_case("Random") => 2,
        value => value
            .parse()
            .ok()
            .filter(|value| (0..=2).contains(value))
            .unwrap_or(0),
    }
}

pub(super) fn parse_vector_list(text: &str) -> Vec<Vec3> {
    text.split('|')
        .filter_map(|token| {
            let mut components = token.split(',').map(str::trim);
            Some(Vec3::new(
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
            ))
        })
        .collect()
}
