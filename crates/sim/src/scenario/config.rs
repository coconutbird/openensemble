//! Retail startup configuration projected into synchronized simulation state.

use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::BTreeSet;

const STARTUP_CONFIG_PATHS: [&str; 4] = [
    "startup\\game.cfg",
    "startup\\locale.cfg",
    "startup\\final.cfg",
    "startup\\user.cfg",
];

/// Read startup config files in retail initialization order.
///
/// This runs before the scenario ERA is mounted: scenario archives can replace
/// database tables, but they cannot retroactively alter startup configuration.
pub(super) fn load_startup_config(source: &mut AssetSource<StdFileProvider>) -> BTreeSet<String> {
    let mut definitions = BTreeSet::new();
    for path in STARTUP_CONFIG_PATHS {
        if let Some(bytes) = source.resolve_with_fallback(path, &[]) {
            apply_config(&String::from_utf8_lossy(&bytes), &mut definitions);
        }
    }
    definitions
}

fn apply_config(text: &str, definitions: &mut BTreeSet<String>) {
    let mut block_comment_depth = 0_u32;
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}');
        let comment_probe = line.trim_start();
        if comment_probe.starts_with("/*") {
            block_comment_depth = block_comment_depth.saturating_add(1);
            continue;
        }
        if comment_probe.starts_with("*/") {
            block_comment_depth = block_comment_depth.saturating_sub(1);
            continue;
        }
        if block_comment_depth > 0 {
            continue;
        }
        let Some((defined, name)) = config_operation(line) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        if defined {
            definitions.insert(name);
        } else {
            definitions.remove(&name);
        }
    }
}

fn config_operation(line: &str) -> Option<(bool, &str)> {
    let line = line.trim_end_matches('\r');
    if line.is_empty() || line.starts_with("//") {
        return None;
    }
    let (defined, command) = match line.as_bytes().first() {
        Some(b'-') => (false, &line[1..]),
        Some(b'+') => (true, &line[1..]),
        _ => (true, line),
    };
    let name = command
        .split([' ', '\t', '\n', '\r', '='])
        .find(|token| !token.is_empty())?;
    (!name.starts_with("//")).then_some((defined, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_definitions_follow_retail_define_undefine_and_comment_rules() {
        let mut definitions = BTreeSet::new();
        apply_config(
            "\u{feff}Veterancy\nCameraFOV=44.9\n+CampaignDebug\n-vEtErAnCy\n\
             // ignored\n/*\nHidden\n*/\nVeterancy 1\n",
            &mut definitions,
        );

        assert_eq!(
            definitions.into_iter().collect::<Vec<_>>(),
            vec![
                "camerafov".to_owned(),
                "campaigndebug".to_owned(),
                "veterancy".to_owned(),
            ]
        );
    }
}
