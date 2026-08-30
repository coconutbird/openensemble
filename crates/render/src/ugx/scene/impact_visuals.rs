//! Decoded proto-visual graphs referenced by TFX temporary visual actions.

use std::collections::HashMap;
use std::sync::Arc;

use pipeline::database::hw1::Visual;
use pipeline::source::{AssetSource, StdFileProvider};

use crate::ugx::Unit;

#[derive(Clone, Debug)]
pub(super) struct ImpactVisualAsset {
    pub(super) unit: Option<Arc<Unit>>,
    pub(super) particle_path: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ImpactVisualAssets {
    assets: HashMap<String, Option<Arc<ImpactVisualAsset>>>,
    issues: Vec<String>,
}

impl ImpactVisualAssets {
    pub(super) fn load_referenced(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        names: impl IntoIterator<Item = String>,
    ) {
        for name in names {
            let key = visual_key(&name);
            if self.assets.contains_key(&key) {
                continue;
            }
            let path = canonical_visual_path(&name);
            let loaded = source
                .read_xmb(&path)
                .ok_or_else(|| format!("impact visual not found: {path}"))
                .and_then(|document| {
                    pipeline::database::hw1::visual::parse(&document)
                        .map_err(|error| format!("failed to parse impact visual '{path}': {error}"))
                })
                .and_then(|visual| load_visual_asset(source, &path, &visual));
            match loaded {
                Ok(asset) => {
                    self.assets.insert(key, Some(Arc::new(asset)));
                }
                Err(error) => {
                    self.issues.push(error);
                    self.assets.insert(key, None);
                }
            }
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<&Arc<ImpactVisualAsset>> {
        self.assets.get(&visual_key(name))?.as_ref()
    }

    pub(super) fn loaded(&self) -> impl Iterator<Item = &Arc<Unit>> {
        self.assets
            .values()
            .filter_map(Option::as_ref)
            .filter_map(|asset| asset.unit.as_ref())
    }

    pub(super) fn particle_paths(&self) -> impl Iterator<Item = String> + '_ {
        self.assets
            .values()
            .filter_map(Option::as_ref)
            .filter_map(|asset| asset.particle_path.clone())
    }

    fn loaded_count(&self) -> usize {
        self.assets.values().filter(|asset| asset.is_some()).count()
    }

    fn issues(&self) -> &[String] {
        &self.issues
    }
}

fn load_visual_asset(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
    visual: &Visual,
) -> Result<ImpactVisualAsset, String> {
    let particle_path = default_asset_path(visual, "Particle").map(str::to_owned);
    let unit = if default_asset_path(visual, "Model").is_some() {
        Some(Arc::new(Unit::load(source, visual).map_err(|error| {
            format!("failed to load impact visual '{path}': {error}")
        })?))
    } else {
        None
    };
    if unit.is_none() && particle_path.is_none() {
        return Err(format!(
            "impact visual '{path}' has no supported Model or Particle asset"
        ));
    }
    Ok(ImpactVisualAsset {
        unit,
        particle_path,
    })
}

fn default_asset_path<'visual>(visual: &'visual Visual, asset_type: &str) -> Option<&'visual str> {
    let default_model = visual.default_model.as_deref()?;
    visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(default_model))?
        .component
        .as_ref()?
        .assets
        .iter()
        .find(|asset| asset.asset_type.eq_ignore_ascii_case(asset_type))?
        .file
        .as_deref()
}

impl super::UnitScene {
    /// Return the number of TFX temporary visual graphs decoded.
    #[must_use]
    pub fn impact_visual_count(&self) -> usize {
        self.impact_visual_assets.loaded_count()
    }

    /// Return missing, malformed, or undecodable impact visual diagnostics.
    #[must_use]
    pub fn impact_visual_issues(&self) -> &[String] {
        self.impact_visual_assets.issues()
    }

    /// Return the number of impact visual diagnostics.
    #[must_use]
    pub fn impact_visual_issue_count(&self) -> usize {
        self.impact_visual_assets.issues().len()
    }
}

fn visual_key(name: &str) -> String {
    canonical_visual_path(name).to_ascii_lowercase()
}

fn canonical_visual_path(name: &str) -> String {
    let mut normalized = name
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized.to_ascii_lowercase().ends_with(".xmb") {
        normalized.truncate(normalized.len() - 4);
    }
    if !normalized.to_ascii_lowercase().ends_with(".vis") {
        normalized.push_str(".vis");
    }
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::Visual;
    use pipeline::database::hw1::visual::{Asset, Component, Model};

    use super::{canonical_visual_path, default_asset_path};

    #[test]
    fn visual_paths_receive_one_art_prefix_and_vis_extension() {
        assert_eq!(
            canonical_visual_path("effects/impact"),
            "art\\effects\\impact.vis"
        );
        assert_eq!(
            canonical_visual_path("art\\effects\\impact.vis.xmb"),
            "art\\effects\\impact.vis"
        );
    }

    #[test]
    fn direct_particle_visuals_are_not_misclassified_as_missing_models() {
        let visual = Visual {
            default_model: Some("Default".to_owned()),
            models: vec![Model {
                name: "Default".to_owned(),
                component: Some(Component {
                    assets: vec![Asset {
                        asset_type: "Particle".to_owned(),
                        file: Some("effects\\impact".to_owned()),
                        ..Asset::default()
                    }],
                    ..Component::default()
                }),
                ..Model::default()
            }],
            ..Visual::default()
        };

        assert_eq!(
            default_asset_path(&visual, "particle"),
            Some("effects\\impact")
        );
        assert_eq!(default_asset_path(&visual, "Model"), None);
    }
}
