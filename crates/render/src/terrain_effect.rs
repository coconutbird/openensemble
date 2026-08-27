//! Renderer-facing adaptation of HW1 terrain-effect (`.tfx`) assets.
//!
//! TFX files do not introduce another GPU material. They route a contacted
//! terrain surface to existing particle, decal/trail, visual, light, and sound
//! systems. The simulation layer can therefore resolve one surface here and
//! submit each returned action to the corresponding foundation renderer.

use std::borrow::Cow;

use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xmb::{Document, Node, Reader, Variant};

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

    /// Resolves a surface and falls back to the authored `default` entry.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&TerrainSurfaceEffect> {
        self.surface(name).or_else(|| self.surface("default"))
    }
}

/// Actions selected for one authored terrain material/surface.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSurfaceEffect {
    /// Surface key such as `earth`, `snow`, `metal`, or `default`.
    pub name: String,
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
            actions,
        })
    }
}

/// One TFX route into an existing runtime subsystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerrainEffectAction {
    /// Starts a PFX through [`crate::particle::ParticleEffect`].
    Particle(String),
    /// Places a short-lived terrain-conforming impact decal.
    ImpactDecal(String),
    /// Extends a terrain-conforming trail decal.
    Trail(String),
    /// Spawns a visual/UGX graph through [`crate::ugx::Unit`].
    Visual(String),
    /// Adds an LGT-backed runtime light through [`crate::lighting`].
    Light(String),
    /// Forwards an event to the audio system; it has no GPU representation.
    Sound(String),
}

impl TerrainEffectAction {
    fn from_node(node: &Node) -> Result<Self, TerrainEffectError> {
        let value = node_text(node).into_owned();
        match node.name.to_ascii_lowercase().as_str() {
            "particlefile" => Ok(Self::Particle(value)),
            "impactdecal" => Ok(Self::ImpactDecal(value)),
            "trail" => Ok(Self::Trail(value)),
            "vis" => Ok(Self::Visual(value)),
            "light" => Ok(Self::Light(value)),
            "sound" => Ok(Self::Sound(value)),
            _ => Err(TerrainEffectError::UnsupportedAction(node.name.clone())),
        }
    }

    /// Returns the authored path or event name.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::Particle(value)
            | Self::ImpactDecal(value)
            | Self::Trail(value)
            | Self::Visual(value)
            | Self::Light(value)
            | Self::Sound(value) => value,
        }
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
}

fn node_text(node: &Node) -> Cow<'_, str> {
    match &node.text {
        Variant::String(value) => Cow::Borrowed(value.as_str()),
        Variant::Null => Cow::Borrowed(""),
        _ => Cow::Owned(node.text_string()),
    }
}

fn canonical_terrain_effect_path(path: &str) -> String {
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

    use super::{TerrainEffect, TerrainEffectAction, canonical_terrain_effect_path};

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
            r"<TerrainEffect>
                <default><particlefile>effects/default</particlefile></default>
                <snow>
                    <particlefile>effects/snow</particlefile>
                    <impactdecal>decals/track</impactdecal>
                    <trail>decals/trail</trail>
                    <vis>effects/visual</vis>
                    <light>effects/light</light>
                    <sound>play_snow</sound>
                </snow>
            </TerrainEffect>",
        )
        .unwrap();
        let effect = TerrainEffect::from_document(&document).unwrap();
        let snow = effect.resolve("SNOW").unwrap();
        assert_eq!(snow.actions.len(), 6);
        assert_eq!(
            snow.actions[1],
            TerrainEffectAction::ImpactDecal("decals/track".to_owned())
        );
        assert_eq!(effect.resolve("water").unwrap().name, "default");
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
