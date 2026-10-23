//! Tactic file paths and parsing for the gameplay catalog.

use pipeline::database::hw1::tactics::TacticData;
use pipeline::source::{AssetSource, StdFileProvider};

pub(super) fn canonical_tactics_path(tactics_ref: &str) -> String {
    let tactics_ref = tactics_ref.trim().replace('/', "\\");
    let tactics_ref = tactics_ref.trim_start_matches('\\');
    if tactics_ref
        .to_ascii_lowercase()
        .starts_with("data\\tactics\\")
    {
        tactics_ref.to_owned()
    } else {
        format!("data\\tactics\\{tactics_ref}")
    }
}

pub(super) fn load_tactics(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
) -> Result<TacticData, String> {
    let document = source
        .read_xmb(path)
        .ok_or_else(|| "asset was not found or was not a valid XMB document".to_owned())?;
    pipeline::database::hw1::tactics::parse(&document)
        .map_err(|error| format!("failed to parse tactic data: {error}"))
}
