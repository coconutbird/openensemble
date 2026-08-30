//! Cached TFX routers and their retail surface-name table.

use std::collections::HashMap;
use std::sync::Arc;

use pipeline::source::{AssetSource, StdFileProvider};

use crate::terrain_effect::{
    TerrainEffect, TerrainEffectAction, TerrainEffectSize, TerrainSurfaceCatalog,
    TerrainSurfaceEffect, canonical_terrain_effect_path,
};
use crate::ugx::UnitAttachmentKind;

#[derive(Clone, Debug, Default)]
pub(super) struct TerrainEffectAssets {
    effects: HashMap<String, Option<Arc<TerrainEffect>>>,
    catalog: Option<Arc<TerrainSurfaceCatalog>>,
    catalog_attempted: bool,
    issues: Vec<String>,
}

impl TerrainEffectAssets {
    pub(super) fn load_for_placements(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        placements: &[super::UnitPlacement],
    ) -> Vec<String> {
        let paths = placements
            .iter()
            .flat_map(|placement| {
                placement.unit.attachments_for_animations(
                    placement.animation_type(),
                    placement.movement_track_animation_type(),
                )
            })
            .filter(|attachment| attachment.kind == UnitAttachmentKind::TerrainEffect)
            .filter_map(|attachment| attachment.asset_path.clone())
            .chain(placements.iter().flat_map(|placement| {
                placement
                    .unit
                    .animation_tags()
                    .filter(|tag| tag.tag_type.eq_ignore_ascii_case("TerrainEffect"))
                    .filter_map(|tag| tag.name.clone())
            }));
        self.load_referenced(source, paths)
    }

    pub(super) fn load_referenced(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        paths: impl IntoIterator<Item = String>,
    ) -> Vec<String> {
        let paths = paths.into_iter().collect::<Vec<_>>();
        if paths.is_empty() {
            return Vec::new();
        }
        self.ensure_catalog(source);
        for path in &paths {
            let key = terrain_effect_key(path);
            if self.effects.contains_key(&key) {
                continue;
            }
            self.effects.insert(key.clone(), None);
            match TerrainEffect::load(source, path) {
                Ok(effect) => {
                    self.effects.insert(key, Some(Arc::new(effect)));
                }
                Err(error) => self.issues.push(error.to_string()),
            }
        }
        paths
            .iter()
            .filter_map(|path| self.get(path))
            .flat_map(|effect| effect.surfaces.iter())
            .flat_map(|surface| surface.actions.iter())
            .filter_map(|action| {
                if let TerrainEffectAction::Particle(path) = action {
                    (!path.trim().is_empty()).then(|| path.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(super) fn particle_path(&self, path: &str, surface_type: u8) -> Option<&str> {
        let surface_name = self.catalog.as_ref()?.name(surface_type)?;
        self.get(path)?
            .first_exact(surface_name, TerrainEffectSize::Generic)?
            .particle_path()
            .filter(|path| !path.trim().is_empty())
    }

    pub(super) fn select(
        &self,
        path: &str,
        surface_type: Option<u8>,
        roll: u32,
    ) -> Option<TerrainSurfaceEffect> {
        let surface_name = surface_type
            .and_then(|surface| self.catalog.as_ref()?.name(surface))
            .unwrap_or("UNDEFINED");
        self.get(path)?
            .select(surface_name, TerrainEffectSize::Generic, roll)
            .cloned()
    }

    pub(super) fn particle_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Particle(path) = action {
                Some(path.as_str())
            } else {
                None
            }
        })
    }

    pub(super) fn light_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Light(light) = action {
                Some(light.path.as_str())
            } else {
                None
            }
        })
    }

    pub(super) fn decal_paths(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::ImpactDecal(decal) = action {
                Some(decal.path.as_str())
            } else {
                None
            }
        })
    }

    pub(super) fn visual_names(&self) -> Vec<String> {
        self.action_paths(|action| {
            if let TerrainEffectAction::Visual(name) = action {
                Some(name.as_str())
            } else {
                None
            }
        })
    }

    pub(super) fn loaded_count(&self) -> usize {
        self.effects
            .values()
            .filter(|effect| effect.is_some())
            .count()
    }

    pub(super) fn issues(&self) -> &[String] {
        &self.issues
    }

    fn get(&self, path: &str) -> Option<&TerrainEffect> {
        self.effects.get(&terrain_effect_key(path))?.as_deref()
    }

    fn action_paths(
        &self,
        mut select: impl for<'action> FnMut(&'action TerrainEffectAction) -> Option<&'action str>,
    ) -> Vec<String> {
        let mut paths = HashMap::new();
        for action in self
            .effects
            .values()
            .filter_map(Option::as_deref)
            .flat_map(|effect| &effect.surfaces)
            .flat_map(|surface| &surface.actions)
        {
            if let Some(path) = select(action).filter(|path| !path.trim().is_empty()) {
                paths
                    .entry(path.to_ascii_lowercase())
                    .or_insert_with(|| path.to_owned());
            }
        }
        paths.into_values().collect()
    }

    fn ensure_catalog(&mut self, source: &mut AssetSource<StdFileProvider>) {
        if self.catalog_attempted {
            return;
        }
        self.catalog_attempted = true;
        match TerrainSurfaceCatalog::load(source) {
            Ok(catalog) => self.catalog = Some(Arc::new(catalog)),
            Err(error) => self.issues.push(error.to_string()),
        }
    }
}

impl super::UnitScene {
    /// Return the number of unique TFX routers decoded for the current roster.
    #[must_use]
    pub fn terrain_effect_count(&self) -> usize {
        self.terrain_effect_assets.loaded_count()
    }

    /// Return the number of missing or invalid TFX/catalog assets encountered.
    #[must_use]
    pub fn terrain_effect_issue_count(&self) -> usize {
        self.terrain_effect_assets.issues().len()
    }

    /// Return diagnostics for missing or invalid TFX/catalog assets.
    #[must_use]
    pub fn terrain_effect_issues(&self) -> &[String] {
        self.terrain_effect_assets.issues()
    }
}

fn terrain_effect_key(path: &str) -> String {
    canonical_terrain_effect_path(path).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use pipeline::xmb::Document;

    use super::{TerrainEffectAssets, terrain_effect_key};
    use crate::terrain_effect::{TerrainEffect, TerrainSurfaceCatalog};

    #[test]
    fn persistent_route_uses_first_exact_surface_without_default_fallback() {
        let catalog = TerrainSurfaceCatalog::from_document(
            &Document::from_xml(
                r#"<TerrainTileTypes>
                    <TerrainTileType name="UNDEFINED" />
                    <TerrainTileType name="Earth" />
                    <TerrainTileType name="Rock" />
                </TerrainTileTypes>"#,
            )
            .unwrap(),
        )
        .unwrap();
        let effect = TerrainEffect::from_document(
            &Document::from_xml(
                r"<TerrainEffect>
                    <default><particlefile>default</particlefile></default>
                    <earth><particlefile>first</particlefile></earth>
                    <earth><particlefile>second</particlefile></earth>
                </TerrainEffect>",
            )
            .unwrap(),
        )
        .unwrap();
        let path = "effects\\test";
        let assets = TerrainEffectAssets {
            effects: HashMap::from([(terrain_effect_key(path), Some(Arc::new(effect)))]),
            catalog: Some(Arc::new(catalog)),
            catalog_attempted: true,
            issues: Vec::new(),
        };

        assert_eq!(assets.particle_path(path, 1), Some("first"));
        assert_eq!(assets.particle_path(path, 2), None);
    }
}
