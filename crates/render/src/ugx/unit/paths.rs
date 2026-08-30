use pipeline::database::hw1::Visual;
use pipeline::database::hw1::visual::Model as VisualModel;

pub(super) fn model_asset_path<'visual>(
    visual: &'visual Visual,
    model: &'visual VisualModel,
    variation_index: Option<usize>,
) -> Option<&'visual str> {
    let component = model.component.as_ref()?;
    let selected = component
        .logic
        .as_ref()
        .filter(|logic| logic.logic_type.eq_ignore_ascii_case("Variation"))
        .and_then(|logic| {
            let last = logic.entries.len().checked_sub(1)?;
            logic.entries.get(variation_index.unwrap_or(0).min(last))
        });
    if let Some(entry) = selected {
        if let Some(path) = entry.asset.as_ref().and_then(model_path) {
            return Some(path);
        }
        if let Some(reference) = entry.model_ref.as_deref() {
            let referenced = visual
                .models
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(reference))?;
            if let Some(path) = direct_model_asset_path(referenced) {
                return Some(path);
            }
        }
    }
    direct_model_asset_path(model)
}

fn direct_model_asset_path(model: &VisualModel) -> Option<&str> {
    model.component.as_ref()?.assets.iter().find_map(model_path)
}

fn model_path(asset: &pipeline::database::hw1::visual::Asset) -> Option<&str> {
    asset
        .asset_type
        .eq_ignore_ascii_case("Model")
        .then_some(asset.file.as_deref())
        .flatten()
}

pub(super) fn canonical_model_path(path: &str) -> String {
    canonical_art_path(path)
}

pub(super) fn canonical_art_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}
