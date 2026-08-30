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

mod runtime_definition;

pub use runtime_definition::{
    ParticleColorDefinition, ParticleColorKey, ParticleColorKind, ParticleColorProgression,
    ParticleEmitterShape, ParticleEmitterShapeKind, ParticleEmitterTiming, ParticleForceDefinition,
    ParticleMagnetDefinition, ParticleMagnetKind, ParticlePaletteEntry, ParticleRuntimeDefinition,
    ParticleScalarKey, ParticleScalarProgression, ParticleScalarProperty, ParticleTrailEmission,
    ParticleTrailUv, ParticleVarying, ParticleVectorProperty,
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
    /// Particle Editor preview switch. The retail loader deliberately ignores
    /// this attribute and initializes runtime activity from emitter timing.
    pub editor_active: bool,
    /// Renderable geometry or a nested PFX reference.
    pub kind: ParticleEmitterKind,
    /// Maximum live-particle budget.
    pub max_particles: u32,
    /// Requests back-to-front sorting.
    pub sort_particles: bool,
    /// Static material/texture references.
    pub material: ParticleMaterialDefinition,
    /// Authored timing, shape, progression, force, and magnet state.
    pub runtime: ParticleRuntimeDefinition,
}

impl ParticleEmitter {
    fn from_node(node: &Node) -> Result<Self, ParticleEffectError> {
        let name = attribute(node, "Name").map_or_else(String::new, Cow::into_owned);
        let editor_active = attribute(node, "Active")
            .and_then(|value| parse_bool(&value))
            .unwrap_or(true);
        let emitter_data =
            child(node, "EmitterData").ok_or_else(|| ParticleEffectError::MissingSection {
                emitter: name.clone(),
                section: "EmitterData",
            })?;
        let particle_type =
            child_text(emitter_data, "ParticleType").unwrap_or(Cow::Borrowed("eBillBoard"));
        let kind = parse_emitter_kind(node, emitter_data, &particle_type)?;
        let runtime = ParticleRuntimeDefinition::from_node(node, emitter_data, &kind)?;
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
        let color_data = child(node, "ColorData");
        let corner_colors = [
            "ColorVertex1",
            "ColorVertex2",
            "ColorVertex3",
            "ColorVertex4",
        ]
        .map(|field| packed_color_child(color_data, field).unwrap_or([1.0; 4]));
        let light_volume = matches!(
            blend,
            ParticleBlendMode::Alpha | ParticleBlendMode::PremultipliedAlpha
        ) && child_text(emitter_data, "LightBuffer")
            .and_then(|value| parse_bool(&value))
            .unwrap_or(true);
        Ok(Self {
            name,
            editor_active,
            kind,
            max_particles: child_text(emitter_data, "MaxParticles")
                .and_then(|value| value.parse().ok())
                .unwrap_or(1000),
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
                light_volume,
                light_volume_intensity: child_text(emitter_data, "LightBufferIntensityScale")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1.0),
                soft_fade_scale: child_text(emitter_data, "SoftParticleFadeRange")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1.0_f32)
                    .clamp(0.1, 2.0),
                corner_colors,
            },
            runtime,
        })
    }

    /// Loads all optional texture arrays while preserving the authored
    /// renderer choices.
    ///
    /// # Errors
    ///
    /// Returns an error when the primary diffuse set has no decodable stage.
    /// Unavailable optional sets are disabled, matching retail draw routing.
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
    /// Default depth-delta multiplier copied to emitted instances.
    pub soft_fade_scale: f32,
    /// Quad-corner modulation colors decoded from A8R8G8B8.
    pub corner_colors: [[f32; 4]; 4],
}

impl ParticleMaterialDefinition {
    /// Resolves referenced DDX files into a GPU-ready material payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the primary diffuse set has no decodable image.
    pub fn load(
        &self,
        source: &mut AssetSource<StdFileProvider>,
    ) -> Result<ParticleMaterial, ParticleError> {
        let mut unavailable_texture_sets = 0;
        let diffuse = [
            self.diffuse[0].load(source)?,
            load_optional_texture_set(&self.diffuse[1], source, &mut unavailable_texture_sets),
            load_optional_texture_set(&self.diffuse[2], source, &mut unavailable_texture_sets),
        ];
        let intensity =
            load_optional_texture_set(&self.intensity, source, &mut unavailable_texture_sets);
        Ok(ParticleMaterial {
            diffuse,
            intensity,
            unavailable_texture_sets,
            layer_1_to_2: self.layer_1_to_2,
            layer_2_to_3: self.layer_2_to_3,
            blend: self.blend,
            soft_particles: self.soft_particles,
            light_volume: self.light_volume,
            light_volume_intensity: self.light_volume_intensity,
            corner_colors: self.corner_colors,
        })
    }
}

fn load_optional_texture_set(
    definition: &ParticleTextureDefinition,
    source: &mut AssetSource<StdFileProvider>,
    unavailable_texture_sets: &mut usize,
) -> Option<ParticleTextureArray> {
    if let Ok(texture) = definition.load(source) {
        texture
    } else {
        *unavailable_texture_sets += 1;
        None
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
    /// Authored frame width in texture pixels.
    pub frame_width: f32,
    /// Authored frame height in texture pixels.
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
    emitter_data: &Node,
    value: &str,
) -> Result<ParticleEmitterKind, ParticleEffectError> {
    let geometry = match value {
        "eBillBoard" => ParticleGeometry::Billboard,
        "eUpfacing" => ParticleGeometry::UpFacing,
        "eOrientedAxialBillboard" => ParticleGeometry::OrientedAxial,
        "eVelocityAligned" => ParticleGeometry::VelocityAligned,
        "eBeam" => parse_beam_geometry(emitter_data)?,
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

fn parse_beam_geometry(emitter_data: &Node) -> Result<ParticleGeometry, ParticleEffectError> {
    let alignment = child_text(emitter_data, "BeamAlignmentType")
        .unwrap_or(Cow::Borrowed("eBeamAlignToCamera"));
    match alignment.as_ref() {
        "eBeamAlignToCamera" => Ok(ParticleGeometry::Beam),
        "eBeamAlignVertical" => Ok(ParticleGeometry::BeamVertical),
        "eBeamAlignHorizontal" => Ok(ParticleGeometry::BeamHorizontal),
        value => unsupported("beam alignment", value),
    }
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
            frames_per_second: uv.map_or(0.0, |uv| {
                numeric_child::<f32>(Some(uv), "FramesPerSecond").max(1.0)
            }),
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

fn packed_color_child(node: Option<&Node>, name: &str) -> Option<[f32; 4]> {
    let value = node.and_then(|node| child_text(node, name))?;
    let packed = value
        .parse::<u32>()
        .ok()
        .or_else(|| value.parse::<i32>().ok().map(i32::cast_unsigned))?;
    let channel =
        |shift: u32| f32::from(u8::try_from((packed >> shift) & 0xff_u32).unwrap_or(0)) / 255.0;
    Some([channel(16), channel(8), channel(0), channel(24)])
}

pub(crate) fn canonical_effect_path(path: &str) -> String {
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
        assert_eq!(
            emitter.material.soft_fade_scale.to_bits(),
            2.0_f32.to_bits()
        );
        assert!(!emitter.material.light_volume);
        assert!(emitter.sort_particles);
        assert_eq!(emitter.max_particles, 12);
        assert_eq!(
            emitter.material.diffuse[0].stages[0].path,
            "effects/test.tga"
        );
    }

    #[test]
    fn beam_alignment_selects_the_retail_geometry_shader_family() {
        for (alignment, expected) in [
            ("eBeamAlignToCamera", ParticleGeometry::Beam),
            ("eBeamAlignVertical", ParticleGeometry::BeamVertical),
            ("eBeamAlignHorizontal", ParticleGeometry::BeamHorizontal),
        ] {
            let document = Document::from_xml(&format!(
                r"<ParticleEffect><ParticleEmitter><EmitterData>
                    <ParticleType>eBeam</ParticleType>
                    <BeamAlignmentType>{alignment}</BeamAlignmentType>
                </EmitterData></ParticleEmitter></ParticleEffect>",
            ))
            .unwrap();
            let effect = ParticleEffect::from_document(&document).unwrap();
            assert_eq!(
                effect.emitters[0].kind,
                ParticleEmitterKind::Render(expected)
            );
        }
    }

    #[test]
    fn corner_colors_and_implicit_light_buffer_match_the_retail_loader() {
        let document = Document::from_xml(
            r#"<ParticleEffect>
                <ParticleEmitter Name="alpha">
                    <EmitterData><BlendMode>eAlphaBlend</BlendMode></EmitterData>
                    <ColorData>
                        <ColorVertex1>-1</ColorVertex1>
                        <ColorVertex2>-65536</ColorVertex2>
                        <ColorVertex3>-16711936</ColorVertex3>
                        <ColorVertex4>-16776961</ColorVertex4>
                    </ColorData>
                </ParticleEmitter>
                <ParticleEmitter Name="additive">
                    <EmitterData><BlendMode>eAdditive</BlendMode></EmitterData>
                </ParticleEmitter>
            </ParticleEffect>"#,
        )
        .unwrap();
        let effect = ParticleEffect::from_document(&document).unwrap();
        assert_eq!(
            effect.emitters[0].material.corner_colors,
            [
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ]
        );
        assert!(effect.emitters[0].material.light_volume);
        assert!(!effect.emitters[1].material.light_volume);
    }
}
