//! Renderer-facing adaptation of HW1 terrain-effect (`.tfx`) assets.
//!
//! TFX files do not introduce another GPU material. They route a contacted
//! terrain surface to existing particle, decal/trail, visual, light, and sound
//! systems. The simulation layer can therefore resolve one surface here and
//! submit each returned action to the corresponding foundation renderer.

use std::borrow::Cow;

use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xmb::{Document, Node, Reader, Variant};

mod impact;
mod runtime;

pub use impact::{ImpactEffectCatalog, ImpactEffectDefinition, ImpactEffectError};
pub use runtime::{
    ResolvedTerrainEffect, TerrainEffectRouteKind, TerrainImpactAssets, TerrainImpactRouter,
};

const TERRAIN_TILE_TYPES_PATH: &str = "data\\terrainTileTypes.xml";

/// A decoded terrain-effect routing table.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainEffect {
    /// Authored surface entries in file order.
    pub surfaces: Vec<TerrainSurfaceEffect>,
}

impl TerrainEffect {
    /// Resolves and decodes a TFX from the active local asset stack.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing file, malformed XMB/XML, an unexpected
    /// root, or an action not represented by the renderer foundation.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, TerrainEffectError> {
        let canonical = canonical_terrain_effect_path(path);
        let bytes = source
            .resolve_with_fallback(&canonical, &[".tfx"])
            .ok_or_else(|| TerrainEffectError::NotFound(canonical.clone()))?;
        let document = Reader::read(&bytes).map_err(|error| TerrainEffectError::Parse {
            path: canonical,
            reason: error.to_string(),
        })?;
        Self::from_document(&document)
    }

    /// Adapts an already-decoded generic XMB document.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing/wrong root or an unknown action node.
    pub fn from_document(document: &Document) -> Result<Self, TerrainEffectError> {
        let root = document.root().ok_or(TerrainEffectError::MissingRoot)?;
        if !root.name.eq_ignore_ascii_case("TerrainEffect") {
            return Err(TerrainEffectError::UnexpectedRoot(root.name.clone()));
        }
        let surfaces = root
            .children
            .iter()
            .map(TerrainSurfaceEffect::from_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { surfaces })
    }

    /// Looks up an exact authored surface name case-insensitively.
    #[must_use]
    pub fn surface(&self, name: &str) -> Option<&TerrainSurfaceEffect> {
        self.surfaces
            .iter()
            .find(|surface| surface.name.eq_ignore_ascii_case(name))
    }

    /// Resolves the first generic item for a surface and falls back to
    /// the authored `default` surface.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&TerrainSurfaceEffect> {
        self.first_exact(name, TerrainEffectSize::Generic)
            .or_else(|| self.first_exact("default", TerrainEffectSize::Generic))
    }

    /// Returns the first item for an exact surface, applying only retail's
    /// requested-size-to-generic fallback.
    ///
    /// Persistent terrain-effect attachments use this path and deliberately do
    /// not fall back to the `default` surface.
    #[must_use]
    pub fn first_exact(
        &self,
        name: &str,
        size: TerrainEffectSize,
    ) -> Option<&TerrainSurfaceEffect> {
        self.items_exact(name, size).into_iter().next()
    }

    /// Selects a weighted item, falling back to the authored `default` surface.
    ///
    /// `roll` is presentation-only entropy. Retail drew inclusively from
    /// `0..=weight_sum` and compared with `<=`, accidentally favoring the first
    /// item. This renderer uses an unbiased half-open roll because that defect
    /// has no synchronized simulation effect.
    #[must_use]
    pub fn select(
        &self,
        name: &str,
        size: TerrainEffectSize,
        roll: u32,
    ) -> Option<&TerrainSurfaceEffect> {
        select_weighted(self.items_exact(name, size).into_iter(), roll)
            .or_else(|| select_weighted(self.items_exact("default", size).into_iter(), roll))
    }

    fn items_exact<'effect>(
        &'effect self,
        name: &str,
        size: TerrainEffectSize,
    ) -> Vec<&'effect TerrainSurfaceEffect> {
        let has_requested_size = size != TerrainEffectSize::Generic
            && self
                .surfaces
                .iter()
                .any(|surface| surface.name.eq_ignore_ascii_case(name) && surface.size == size);
        let resolved_size = if has_requested_size {
            size
        } else {
            TerrainEffectSize::Generic
        };
        self.surfaces
            .iter()
            .filter(|surface| {
                surface.name.eq_ignore_ascii_case(name) && surface.size == resolved_size
            })
            .collect()
    }
}

/// Retail impact-size bucket used to group TFX variants.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerrainEffectSize {
    /// Small impact effect.
    Small,
    /// Medium impact effect.
    Medium,
    /// Large impact effect.
    Large,
    /// Non-size-specific entry and retail fallback bucket.
    #[default]
    Generic,
}

impl TerrainEffectSize {
    fn from_attribute(value: Option<Cow<'_, str>>) -> Self {
        let Some(value) = value else {
            return Self::Generic;
        };
        if value.eq_ignore_ascii_case("small") {
            Self::Small
        } else if value.eq_ignore_ascii_case("medium") {
            Self::Medium
        } else if value.eq_ignore_ascii_case("large") {
            Self::Large
        } else {
            Self::Generic
        }
    }
}

/// Actions selected for one authored terrain material/surface.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSurfaceEffect {
    /// Surface key such as `earth`, `snow`, `metal`, or `default`.
    pub name: String,
    /// Authored impact-size bucket.
    pub size: TerrainEffectSize,
    /// Authored relative selection weight. Zero is normalized to one.
    pub weight: u32,
    /// Routed actions in authored order.
    pub actions: Vec<TerrainEffectAction>,
}

impl TerrainSurfaceEffect {
    fn from_node(node: &Node) -> Result<Self, TerrainEffectError> {
        let actions = node
            .children
            .iter()
            .map(TerrainEffectAction::from_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: node.name.clone(),
            size: TerrainEffectSize::from_attribute(attribute(node, "size")),
            weight: parse_attribute(node, "weight").unwrap_or(1).max(1),
            actions,
        })
    }

    /// Returns the particle route that retail retains for this item.
    ///
    /// Repeated nodes overwrite the same retail field, so the last one wins.
    #[must_use]
    pub fn particle_path(&self) -> Option<&str> {
        self.actions.iter().rev().find_map(|action| {
            if let TerrainEffectAction::Particle(path) = action {
                Some(path.as_str())
            } else {
                None
            }
        })
    }
}

/// Orientation rule for a terrain-conforming impact decal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerrainDecalOrientation {
    /// Align the decal to the impact direction.
    #[default]
    Aligned,
    /// Pick a renderer-only random rotation.
    Random,
}

/// Authored terrain impact-decal route.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainImpactDecal {
    /// Base texture path; retail derives diffuse, normal, opacity, and specular maps.
    pub path: String,
    /// Decal extent along its local X axis.
    pub size_x: f32,
    /// Decal extent along its local Z axis.
    pub size_z: f32,
    /// Fully opaque lifetime in seconds.
    pub fully_opaque_seconds: f32,
    /// Fade-out lifetime in seconds.
    pub fade_out_seconds: f32,
    /// Direction-aligned or renderer-random rotation.
    pub orientation: TerrainDecalOrientation,
}

/// Authored terrain-ribbon route.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainTrail {
    /// Base texture path.
    pub path: String,
    /// Minimum world-space distance between ribbon nodes.
    pub minimum_node_distance: f32,
    /// Ribbon width in world units.
    pub width: f32,
    /// Frames held at full alpha.
    pub full_alpha_frames: i32,
    /// Frames spent fading out.
    pub fade_out_frames: i32,
    /// Retail fixed maximum number of retained nodes.
    pub max_nodes: u32,
}

/// Authored timed local-light route.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainLight {
    /// LGT asset path.
    pub path: String,
    /// Lifetime in seconds.
    pub lifespan_seconds: f32,
}

/// One TFX route into an existing runtime subsystem.
#[derive(Clone, Debug, PartialEq)]
pub enum TerrainEffectAction {
    /// Starts a PFX through [`crate::particle::ParticleEffect`].
    Particle(String),
    /// Places a short-lived terrain-conforming impact decal.
    ImpactDecal(TerrainImpactDecal),
    /// Extends a terrain-conforming trail decal.
    Trail(TerrainTrail),
    /// Spawns a visual/UGX graph through [`crate::ugx::Unit`].
    Visual(String),
    /// Adds an LGT-backed runtime light through [`crate::lighting`].
    Light(TerrainLight),
    /// Forwards an event to the audio system; it has no GPU representation.
    Sound(String),
}

impl TerrainEffectAction {
    fn from_node(node: &Node) -> Result<Self, TerrainEffectError> {
        let value = node_text(node).into_owned();
        match node.name.to_ascii_lowercase().as_str() {
            "particlefile" => Ok(Self::Particle(value)),
            "impactdecal" => Ok(Self::ImpactDecal(TerrainImpactDecal {
                path: value,
                size_x: parse_attribute(node, "sizeX").unwrap_or(0.0),
                size_z: parse_attribute(node, "sizeZ").unwrap_or(0.0),
                fully_opaque_seconds: parse_attribute(node, "opaque").unwrap_or(0.0),
                fade_out_seconds: parse_attribute(node, "fadeouttime").unwrap_or(0.0),
                orientation: if attribute(node, "orientation")
                    .is_some_and(|value| value.eq_ignore_ascii_case("random"))
                {
                    TerrainDecalOrientation::Random
                } else {
                    TerrainDecalOrientation::Aligned
                },
            })),
            "trail" => Ok(Self::Trail(TerrainTrail {
                path: value,
                minimum_node_distance: parse_attribute(node, "minDistBetweenNodes").unwrap_or(2.0),
                width: parse_attribute(node, "width").unwrap_or(2.5),
                full_alpha_frames: parse_attribute(node, "numFramesFullAlpha").unwrap_or(30),
                fade_out_frames: parse_attribute(node, "numFramesFadeOut").unwrap_or(30),
                max_nodes: 30,
            })),
            "vis" => Ok(Self::Visual(value)),
            "light" => Ok(Self::Light(TerrainLight {
                path: value,
                lifespan_seconds: parse_attribute(node, "lifespan").unwrap_or(0.0),
            })),
            "sound" => Ok(Self::Sound(value)),
            _ => Err(TerrainEffectError::UnsupportedAction(node.name.clone())),
        }
    }

    /// Returns the authored path or event name.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::Particle(value) | Self::Visual(value) | Self::Sound(value) => value,
            Self::ImpactDecal(decal) => &decal.path,
            Self::Trail(trail) => &trail.path,
            Self::Light(light) => &light.path,
        }
    }
}

/// Surface-type name table loaded in the exact retail numeric order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerrainSurfaceCatalog {
    names: Vec<String>,
}

impl TerrainSurfaceCatalog {
    /// Loads `data\\terrainTileTypes.xml` from the active asset stack.
    ///
    /// # Errors
    ///
    /// Returns an error when the table is missing, malformed, or lacks a name.
    pub fn load(source: &mut AssetSource<StdFileProvider>) -> Result<Self, TerrainEffectError> {
        let bytes = source
            .resolve_with_fallback(TERRAIN_TILE_TYPES_PATH, &[".xmb"])
            .ok_or_else(|| TerrainEffectError::NotFound(TERRAIN_TILE_TYPES_PATH.to_owned()))?;
        let document = Reader::read(&bytes).map_err(|error| TerrainEffectError::Parse {
            path: TERRAIN_TILE_TYPES_PATH.to_owned(),
            reason: error.to_string(),
        })?;
        Self::from_document(&document)
    }

    /// Adapts an already-decoded terrain tile-type document.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing/wrong root or an unnamed entry.
    pub fn from_document(document: &Document) -> Result<Self, TerrainEffectError> {
        let root = document.root().ok_or(TerrainEffectError::MissingRoot)?;
        if !root.name.eq_ignore_ascii_case("TerrainTileTypes") {
            return Err(TerrainEffectError::UnexpectedRoot(root.name.clone()));
        }
        let names = root
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("TerrainTileType"))
            .map(|node| {
                attribute(node, "name")
                    .map(Cow::into_owned)
                    .filter(|name| !name.is_empty())
                    .ok_or(TerrainEffectError::MissingSurfaceName)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { names })
    }

    /// Resolves a retail surface byte to its authored name.
    #[must_use]
    pub fn name(&self, surface_type: u8) -> Option<&str> {
        self.names
            .get(usize::from(surface_type))
            .map(String::as_str)
    }

    /// Returns the number of indexed surface names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns whether the catalog has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// TFX adaptation failure.
#[derive(Debug, thiserror::Error)]
pub enum TerrainEffectError {
    /// The requested terrain effect was absent from the active asset stack.
    #[error("terrain effect not found: {0}")]
    NotFound(String),
    /// The generic XMB/XML reader rejected the source bytes.
    #[error("failed to parse terrain effect '{path}': {reason}")]
    Parse {
        /// Canonical game path.
        path: String,
        /// Reader diagnostic.
        reason: String,
    },
    /// The decoded document contained no root node.
    #[error("terrain effect XMB has no root node")]
    MissingRoot,
    /// The document was not a terrain-effect definition.
    #[error("unexpected terrain effect root '{0}'")]
    UnexpectedRoot(String),
    /// A routed action is absent from the known shipped TFX schema.
    #[error("unsupported terrain effect action '{0}'")]
    UnsupportedAction(String),
    /// A terrain tile-type entry did not provide its required name.
    #[error("terrain tile type is missing its name")]
    MissingSurfaceName,
}

fn select_weighted<'effect>(
    items: impl Iterator<Item = &'effect TerrainSurfaceEffect>,
    roll: u32,
) -> Option<&'effect TerrainSurfaceEffect> {
    let items = items.collect::<Vec<_>>();
    let total = items.iter().map(|item| u64::from(item.weight)).sum::<u64>();
    if total == 0 {
        return None;
    }
    let target = u64::try_from((u128::from(roll) * u128::from(total)) >> 32).unwrap_or(u64::MAX);
    let mut cumulative = 0_u64;
    items.into_iter().find(|item| {
        cumulative = cumulative.saturating_add(u64::from(item.weight));
        target < cumulative
    })
}

fn node_text(node: &Node) -> Cow<'_, str> {
    match &node.text {
        Variant::String(value) => Cow::Borrowed(value.as_str()),
        Variant::Null => Cow::Borrowed(""),
        _ => Cow::Owned(node.text_string()),
    }
}

fn attribute<'a>(node: &'a Node, name: &str) -> Option<Cow<'a, str>> {
    let attribute = node
        .attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))?;
    match &attribute.value {
        Variant::String(value) => Some(Cow::Borrowed(value.as_str())),
        Variant::Null => None,
        _ => Some(Cow::Owned(attribute.value_string())),
    }
}

fn parse_attribute<T: std::str::FromStr>(node: &Node, name: &str) -> Option<T> {
    attribute(node, name)?.trim().parse().ok()
}

pub(crate) fn canonical_terrain_effect_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    let normalized = [".tfx", ".TFX"]
        .into_iter()
        .find_map(|suffix| normalized.strip_suffix(suffix))
        .unwrap_or(&normalized);
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized.to_owned()
    } else {
        format!("art\\{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use pipeline::xmb::Document;

    use super::{
        TerrainDecalOrientation, TerrainEffect, TerrainEffectAction, TerrainEffectSize,
        TerrainSurfaceCatalog, canonical_terrain_effect_path,
    };

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn tfx_path_is_canonicalized_without_double_extension() {
        assert_eq!(
            canonical_terrain_effect_path("effects/terrain/warthog.tfx"),
            "art\\effects\\terrain\\warthog"
        );
    }

    #[test]
    fn routes_surface_actions_and_default_fallback() {
        let document = Document::from_xml(
            r#"<TerrainEffect>
                <default><particlefile>effects/default</particlefile></default>
                <snow weight="2" size="Small">
                    <particlefile>effects/snow</particlefile>
                    <impactdecal sizeX="4" sizeZ="6" opaque="0.25"
                        fadeouttime="1.5" orientation="random">decals/track</impactdecal>
                    <trail>decals/trail</trail>
                    <vis>effects/visual</vis>
                    <light lifespan="0.5">effects/light</light>
                    <sound>play_snow</sound>
                </snow>
            </TerrainEffect>"#,
        )
        .unwrap();
        let effect = TerrainEffect::from_document(&document).unwrap();
        let snow = effect
            .first_exact("SNOW", TerrainEffectSize::Small)
            .unwrap();
        assert_eq!(snow.actions.len(), 6);
        assert_eq!(snow.weight, 2);
        let TerrainEffectAction::ImpactDecal(decal) = &snow.actions[1] else {
            panic!("expected impact decal");
        };
        assert_eq!(decal.path, "decals/track");
        assert_near(decal.size_x, 4.0);
        assert_near(decal.size_z, 6.0);
        assert_near(decal.fully_opaque_seconds, 0.25);
        assert_near(decal.fade_out_seconds, 1.5);
        assert_eq!(decal.orientation, TerrainDecalOrientation::Random);
        assert_eq!(effect.resolve("water").unwrap().name, "default");
    }

    #[test]
    fn size_fallback_and_weighted_selection_are_unbiased() {
        let document = Document::from_xml(
            r#"<TerrainEffect>
                <earth weight="1"><particlefile>first</particlefile></earth>
                <earth weight="3"><particlefile>second</particlefile></earth>
                <earth size="Large"><particlefile>large</particlefile></earth>
            </TerrainEffect>"#,
        )
        .unwrap();
        let effect = TerrainEffect::from_document(&document).unwrap();

        assert_eq!(
            effect
                .first_exact("earth", TerrainEffectSize::Large)
                .and_then(|item| item.particle_path()),
            Some("large")
        );
        assert_eq!(
            effect
                .first_exact("earth", TerrainEffectSize::Medium)
                .and_then(|item| item.particle_path()),
            Some("first")
        );
        assert_eq!(
            [0, 1_u32 << 30, 2_u32 << 30, 3_u32 << 30]
                .into_iter()
                .map(|roll| effect
                    .select("earth", TerrainEffectSize::Generic, roll)
                    .and_then(|item| item.particle_path())
                    .unwrap())
                .collect::<Vec<_>>(),
            ["first", "second", "second", "second"]
        );
    }

    #[test]
    fn trail_defaults_match_retail_ribbon_creation() {
        let document = Document::from_xml(
            "<TerrainEffect><earth><trail>tracks</trail></earth></TerrainEffect>",
        )
        .unwrap();
        let effect = TerrainEffect::from_document(&document).unwrap();
        let TerrainEffectAction::Trail(trail) = &effect.surfaces[0].actions[0] else {
            panic!("expected trail");
        };
        assert_near(trail.minimum_node_distance, 2.0);
        assert_near(trail.width, 2.5);
        assert_eq!(trail.full_alpha_frames, 30);
        assert_eq!(trail.fade_out_frames, 30);
        assert_eq!(trail.max_nodes, 30);
    }

    #[test]
    fn surface_catalog_retains_authored_numeric_order() {
        let document = Document::from_xml(
            r#"<TerrainTileTypes>
                <TerrainTileType name="UNDEFINED" />
                <TerrainTileType name="Sand" />
                <TerrainTileType name="Earth" />
            </TerrainTileTypes>"#,
        )
        .unwrap();
        let catalog = TerrainSurfaceCatalog::from_document(&document).unwrap();
        assert_eq!(catalog.name(0), Some("UNDEFINED"));
        assert_eq!(catalog.name(2), Some("Earth"));
        assert_eq!(catalog.name(3), None);
    }

    #[test]
    fn rejects_unknown_routes_instead_of_dropping_them() {
        let document = Document::from_xml(
            "<TerrainEffect><earth><unknown>x</unknown></earth></TerrainEffect>",
        )
        .unwrap();
        assert!(TerrainEffect::from_document(&document).is_err());
    }
}
