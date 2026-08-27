//! High-level PFX adapter built on the existing generic XMB reader.
//!
//! This is intentionally not a replacement low-level reader. It translates
//! the already-decoded XMB tree into renderer material and geometry choices.

use std::borrow::Cow;

use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xmb::{Document, Node, Reader};

use super::{
    ParticleBlendMode, ParticleError, ParticleGeometry, ParticleLayerBlend, ParticleMaterial,
    ParticleTextureArray,
};

/// A decoded PFX effect containing renderer-facing emitter definitions.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleEffect {
    /// Authored effect name/path.
    pub name: String,
    /// Emitters in authored order.
    pub emitters: Vec<ParticleEmitter>,
}

impl ParticleEffect {
    /// Loads a PFX/XMB asset through the active local asset stack.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset is missing, invalid XMB, or contains an
    /// unsupported authored renderer enum.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, ParticleEffectError> {
        let canonical = canonical_effect_path(path);
        let bytes = source
            .resolve_with_fallback(&canonical, &[".pfx.xmb", ".xmb"])
            .ok_or_else(|| ParticleEffectError::NotFound(canonical.clone()))?;
        let document = Reader::read(&bytes).map_err(|error| ParticleEffectError::Parse {
            path: canonical,
            reason: error.to_string(),
        })?;
        Self::from_document(&document)
    }

    /// Adapts an already-decoded XMB document without re-reading it.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong root element or unsupported renderer
    /// enum values.
    pub fn from_document(document: &Document) -> Result<Self, ParticleEffectError> {
        let root = document.root().ok_or(ParticleEffectError::MissingRoot)?;
        if !root.name.eq_ignore_ascii_case("ParticleEffect") {
            return Err(ParticleEffectError::UnexpectedRoot(root.name.clone()));
        }
        let name = attribute(root, "Name").map_or_else(String::new, Cow::into_owned);
        let emitters = root
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("ParticleEmitter"))
            .map(ParticleEmitter::from_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { name, emitters })
    }
}

/// One PFX emitter's renderer-relevant authored state.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleEmitter {
    /// Authored emitter name.
    pub name: String,
    /// Initial active switch from the emitter attribute.
    pub active: bool,
    /// Renderable geometry or a nested PFX reference.
    pub kind: ParticleEmitterKind,
    /// Maximum live-particle budget.
    pub max_particles: u32,
    /// Requests back-to-front sorting.
    pub sort_particles: bool,
    /// Static material/texture references.
    pub material: ParticleMaterialDefinition,
}

impl ParticleEmitter {
    fn from_node(node: &Node) -> Result<Self, ParticleEffectError> {
        let name = attribute(node, "Name").map_or_else(String::new, Cow::into_owned);
        let active = attribute(node, "Active")
            .and_then(|value| parse_bool(&value))
            .unwrap_or(true);
        let emitter_data =
            child(node, "EmitterData").ok_or_else(|| ParticleEffectError::MissingSection {
                emitter: name.clone(),
                section: "EmitterData",
            })?;
        let particle_type =
            child_text(emitter_data, "ParticleType").unwrap_or(Cow::Borrowed("eBillBoard"));
        let kind = parse_emitter_kind(node, &particle_type)?;
        let blend_value =
            child_text(emitter_data, "BlendMode").unwrap_or(Cow::Borrowed("eAlphaBlend"));
        let blend = parse_blend(&blend_value)?;
        let texture_data = child(node, "TextureData");
        let diffuse = ["Diffuse", "Diffuse2", "Diffuse3"].map(|name| {
            texture_data
                .and_then(|textures| child(textures, name))
                .map_or_else(ParticleTextureDefinition::default, parse_texture_definition)
        });
        let intensity = texture_data
            .and_then(|textures| child(textures, "Intensity"))
            .map_or_else(ParticleTextureDefinition::default, parse_texture_definition);
        let layer_1_to_2_value = texture_data
            .and_then(|textures| child_text(textures, "DiffuseLayer1To2BlendMode"))
            .unwrap_or(Cow::Borrowed("eBlendMultiply"));
        let layer_1_to_2 = parse_layer_blend(&layer_1_to_2_value)?;
        let layer_2_to_3_value = texture_data
            .and_then(|textures| child_text(textures, "DiffuseLayer2To3BlendMode"))
            .unwrap_or(Cow::Borrowed("eBlendMultiply"));
        let layer_2_to_3 = parse_layer_blend(&layer_2_to_3_value)?;
        Ok(Self {
            name,
            active,
            kind,
            max_particles: child_text(emitter_data, "MaxParticles")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            sort_particles: child_text(emitter_data, "SortParticles")
                .and_then(|value| parse_bool(&value))
                .unwrap_or(false),
            material: ParticleMaterialDefinition {
                diffuse,
                intensity,
                layer_1_to_2,
                layer_2_to_3,
                blend,
                soft_particles: child_text(emitter_data, "SoftParticles")
                    .and_then(|value| parse_bool(&value))
                    .unwrap_or(false),
                light_volume: child_text(emitter_data, "LightBuffer")
                    .and_then(|value| parse_bool(&value))
                    .unwrap_or(false),
                light_volume_intensity: child_text(emitter_data, "LightBufferIntensityScale")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1.0),
                soft_fade_range: child_text(emitter_data, "SoftParticleFadeRange")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.5),
            },
        })
    }

    /// Loads all optional texture arrays while preserving the authored
    /// renderer choices.
    ///
    /// # Errors
    ///
    /// Returns an error when any authored texture stage is missing, invalid,
    /// or incompatible with the other stages in its array.
    pub fn load_material(
        &self,
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<ParticleMaterial, ParticleError> {
        self.material.load(source)
    }
}

/// Whether an emitter draws geometry or recursively starts another PFX.
#[derive(Clone, Debug, PartialEq)]
pub enum ParticleEmitterKind {
    /// GPU-expandable particle geometry.
    Render(ParticleGeometry),
    /// Nested effect path authored by an `ePFX` emitter.
    NestedEffect(String),
}

/// Renderer material description retaining texture paths until GPU upload.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleMaterialDefinition {
    /// Diffuse map definitions in layer order.
    pub diffuse: [ParticleTextureDefinition; 3],
    /// Optional intensity map definition.
    pub intensity: ParticleTextureDefinition,
    /// Diffuse layer-one/two blend operation.
    pub layer_1_to_2: ParticleLayerBlend,
    /// Diffuse layer-two/three blend operation.
    pub layer_2_to_3: ParticleLayerBlend,
    /// Framebuffer blend family.
    pub blend: ParticleBlendMode,
    /// Whether scene-depth fading is requested.
    pub soft_particles: bool,
    /// Whether the light volume modulates particle RGB.
    pub light_volume: bool,
    /// Authored light-volume multiplier.
    pub light_volume_intensity: f32,
    /// Default soft-depth fade range copied to emitted instances.
    pub soft_fade_range: f32,
}

impl ParticleMaterialDefinition {
    /// Resolves referenced DDX files into a GPU-ready material payload.
    ///
    /// # Errors
    ///
    /// Returns an error when a referenced image is absent or invalid.
    pub fn load(
        &self,
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<ParticleMaterial, ParticleError> {
        let diffuse = self
            .diffuse
            .each_ref()
            .map(|definition| definition.load(source))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| ParticleError::TooManyTextureLayers { actual: 3 })?;
        Ok(ParticleMaterial {
            diffuse,
            intensity: self.intensity.load(source)?,
            layer_1_to_2: self.layer_1_to_2,
            layer_2_to_3: self.layer_2_to_3,
            blend: self.blend,
            soft_particles: self.soft_particles,
            light_volume: self.light_volume,
            light_volume_intensity: self.light_volume_intensity,
        })
    }
}

/// Weighted texture stages and UV animation for one material layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleTextureDefinition {
    /// Texture stages in authored weighted-selection order.
    pub stages: Vec<ParticleTextureStage>,
    /// Authored UV/frame animation controls.
    pub uv_animation: ParticleUvAnimation,
}

impl ParticleTextureDefinition {
    fn load(
        &self,
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<Option<ParticleTextureArray>, ParticleError> {
        if self.stages.is_empty() {
            return Ok(None);
        }
        let paths = self
            .stages
            .iter()
            .map(|stage| stage.path.clone())
            .collect::<Vec<_>>();
        ParticleTextureArray::load(source, &paths).map(Some)
    }
}

/// One weighted texture-array entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleTextureStage {
    /// PFX texture reference.
    pub path: String,
    /// Relative random-selection weight.
    pub weight: f32,
}

/// PFX UV animation controls needed to populate instance UV rectangles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleUvAnimation {
    /// Enables sprite-sheet frame selection.
    pub enabled: bool,
    /// Randomizes initial horizontal scroll.
    pub random_scroll_u: bool,
    /// Randomizes initial vertical scroll.
    pub random_scroll_v: bool,
    /// Number of sprite-sheet frames.
    pub frame_count: u32,
    /// Normalized frame width.
    pub frame_width: f32,
    /// Normalized frame height.
    pub frame_height: f32,
    /// Playback rate.
    pub frames_per_second: f32,
    /// Horizontal UV scroll per second.
    pub scroll_u: f32,
    /// Vertical UV scroll per second.
    pub scroll_v: f32,
}

/// PFX adaptation error.
#[derive(Debug, thiserror::Error)]
pub enum ParticleEffectError {
    /// Effect file was absent from the active asset stack.
    #[error("particle effect not found: {0}")]
    NotFound(String),
    /// XMB decoding failed.
    #[error("failed to parse particle effect '{path}': {reason}")]
    Parse {
        /// Canonical game path.
        path: String,
        /// XMB diagnostic.
        reason: String,
    },
    /// Document had no root node.
    #[error("particle effect XMB has no root node")]
    MissingRoot,
    /// Document root was not `ParticleEffect`.
    #[error("unexpected particle effect root '{0}'")]
    UnexpectedRoot(String),
    /// Required emitter section was absent.
    #[error("particle emitter '{emitter}' is missing {section}")]
    MissingSection {
        /// Emitter name.
        emitter: String,
        /// Required section name.
        section: &'static str,
    },
    /// Renderer enum is not represented by the typed foundation.
    #[error("unsupported particle {field} value '{value}'")]
    UnsupportedValue {
        /// Field name.
        field: &'static str,
        /// Authored enum string.
        value: String,
    },
}

fn parse_emitter_kind(
    emitter: &Node,
    value: &str,
) -> Result<ParticleEmitterKind, ParticleEffectError> {
    let geometry = match value {
        "eBillBoard" => ParticleGeometry::Billboard,
        "eUpfacing" => ParticleGeometry::UpFacing,
        "eOrientedAxialBillboard" => ParticleGeometry::OrientedAxial,
        "eVelocityAligned" => ParticleGeometry::VelocityAligned,
        "eBeam" => ParticleGeometry::Beam,
        "eTrail" => ParticleGeometry::Trail,
        "eTrailCross" => ParticleGeometry::TrailCross,
        "eTerrainPatch" => ParticleGeometry::TerrainPatch,
        "ePFX" => {
            let path =
                descendant_text(emitter, "PFXFilePath").map_or_else(String::new, Cow::into_owned);
            return Ok(ParticleEmitterKind::NestedEffect(path));
        }
        _ => return unsupported("type", value),
    };
    Ok(ParticleEmitterKind::Render(geometry))
}

fn parse_blend(value: &str) -> Result<ParticleBlendMode, ParticleEffectError> {
    match value {
        "eAlphaBlend" => Ok(ParticleBlendMode::Alpha),
        "eAdditive" => Ok(ParticleBlendMode::Additive),
        "ePremultipliedAlpha" => Ok(ParticleBlendMode::PremultipliedAlpha),
        "eSubtractive" => Ok(ParticleBlendMode::Subtractive),
        "eDistortion" => Ok(ParticleBlendMode::Distortion),
        _ => unsupported("blend mode", value),
    }
}

fn parse_layer_blend(value: &str) -> Result<ParticleLayerBlend, ParticleEffectError> {
    match value {
        "eBlendMultiply" => Ok(ParticleLayerBlend::Multiply),
        "eBlendAlpha" => Ok(ParticleLayerBlend::Alpha),
        _ => unsupported("layer blend", value),
    }
}

fn unsupported<T>(field: &'static str, value: &str) -> Result<T, ParticleEffectError> {
    Err(ParticleEffectError::UnsupportedValue {
        field,
        value: value.to_owned(),
    })
}

fn parse_texture_definition(node: &Node) -> ParticleTextureDefinition {
    let stages = child(node, "Textures")
        .map(|textures| {
            textures
                .children
                .iter()
                .filter(|stage| stage.name.eq_ignore_ascii_case("Stage"))
                .filter_map(|stage| {
                    let path = child_text(stage, "file")?.into_owned();
                    let weight = child_text(stage, "weight")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(1.0);
                    Some(ParticleTextureStage { path, weight })
                })
                .collect()
        })
        .unwrap_or_default();
    let uv = child(node, "UVAnimation");
    ParticleTextureDefinition {
        stages,
        uv_animation: ParticleUvAnimation {
            enabled: uv
                .and_then(|uv| child_text(uv, "UVAnimationEnabled"))
                .and_then(|value| parse_bool(&value))
                .unwrap_or(false),
            random_scroll_u: uv
                .and_then(|uv| child_text(uv, "UseRandomScrollOffsetU"))
                .and_then(|value| parse_bool(&value))
                .unwrap_or(false),
            random_scroll_v: uv
                .and_then(|uv| child_text(uv, "UseRandomScrollOffsetV"))
                .and_then(|value| parse_bool(&value))
                .unwrap_or(false),
            frame_count: numeric_child(uv, "NumFrames"),
            frame_width: numeric_child(uv, "FrameWidth"),
            frame_height: numeric_child(uv, "FrameHeight"),
            frames_per_second: numeric_child(uv, "FramesPerSecond"),
            scroll_u: numeric_child(uv, "ScrollU"),
            scroll_v: numeric_child(uv, "ScrollV"),
        },
    }
}

fn numeric_child<T>(node: Option<&Node>, name: &str) -> T
where
    T: std::str::FromStr + Default,
{
    node.and_then(|node| child_text(node, name))
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn child<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn child_text<'a>(node: &'a Node, name: &str) -> Option<Cow<'a, str>> {
    child(node, name).and_then(node_text)
}

fn descendant_text<'a>(node: &'a Node, name: &str) -> Option<Cow<'a, str>> {
    if node.name.eq_ignore_ascii_case(name) {
        return node_text(node);
    }
    node.children
        .iter()
        .find_map(|child| descendant_text(child, name))
}

fn node_text(node: &Node) -> Option<Cow<'_, str>> {
    match &node.text {
        pipeline::xmb::Variant::Null => None,
        pipeline::xmb::Variant::String(value) => Some(Cow::Borrowed(value.as_str())),
        _ => Some(Cow::Owned(node.text_string())),
    }
}

fn attribute<'a>(node: &'a Node, name: &str) -> Option<Cow<'a, str>> {
    let attribute = node
        .attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))?;
    match &attribute.value {
        pipeline::xmb::Variant::Null => None,
        pipeline::xmb::Variant::String(value) => Some(Cow::Borrowed(value.as_str())),
        _ => Some(Cow::Owned(attribute.value_string())),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") || value == "1" {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") || value == "0" {
        Some(false)
    } else {
        None
    }
}

fn canonical_effect_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    let normalized = [".pfx.xmb", ".PFX.XMB", ".pfx", ".PFX", ".xmb", ".XMB"]
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

    use super::{ParticleEffect, ParticleEmitterKind, canonical_effect_path};
    use crate::particle::{ParticleBlendMode, ParticleGeometry, ParticleLayerBlend};

    #[test]
    fn pfx_path_is_canonicalized_without_double_extensions() {
        assert_eq!(
            canonical_effect_path("effects/fire/test.pfx.xmb"),
            "art\\effects\\fire\\test"
        );
    }

    #[test]
    fn decoded_xmb_maps_every_renderer_choice_directly() {
        let document = Document::from_xml(
            r#"<ParticleEffect Name="effects/test.pfx">
                <ParticleEmitter Name="beam" Active="true">
                    <EmitterData>
                        <MaxParticles>12</MaxParticles>
                        <ParticleType>eTrailCross</ParticleType>
                        <BlendMode>eSubtractive</BlendMode>
                        <SoftParticles>true</SoftParticles>
                        <SortParticles>true</SortParticles>
                        <LightBuffer>true</LightBuffer>
                        <LightBufferIntensityScale>2</LightBufferIntensityScale>
                        <SoftParticleFadeRange>3</SoftParticleFadeRange>
                    </EmitterData>
                    <TextureData>
                        <DiffuseLayer1To2BlendMode>eBlendAlpha</DiffuseLayer1To2BlendMode>
                        <DiffuseLayer2To3BlendMode>eBlendMultiply</DiffuseLayer2To3BlendMode>
                        <Diffuse><Textures><Stage><file>effects/test.tga</file><weight>2</weight></Stage></Textures></Diffuse>
                    </TextureData>
                </ParticleEmitter>
            </ParticleEffect>"#,
        )
        .unwrap();
        let effect = ParticleEffect::from_document(&document).unwrap();
        let emitter = &effect.emitters[0];
        assert_eq!(
            emitter.kind,
            ParticleEmitterKind::Render(ParticleGeometry::TrailCross)
        );
        assert_eq!(emitter.material.blend, ParticleBlendMode::Subtractive);
        assert_eq!(emitter.material.layer_1_to_2, ParticleLayerBlend::Alpha);
        assert!(emitter.material.soft_particles);
        assert!(emitter.material.light_volume);
        assert!(emitter.sort_particles);
        assert_eq!(emitter.max_particles, 12);
        assert_eq!(
            emitter.material.diffuse[0].stages[0].path,
            "effects/test.tga"
        );
    }
}
