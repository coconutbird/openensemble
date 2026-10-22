//! Renderer adapter for decoded PFX effects.
//!
//! The `pipeline::database::hw1::pfx` parser decodes the authored PFX data. This module changes
//! the authored enums into renderer types and loads the referenced textures into a GPU-ready
//! material payload.

use pipeline::database::hw1::pfx::{
    BlendMode, LayerBlendMode, ParticleMaterialDefinition, ParticleTextureDefinition, ParticleType,
};
use pipeline::source::{AssetSource, StdFileProvider};

use super::{
    ParticleBlendMode, ParticleError, ParticleGeometry, ParticleLayerBlend, ParticleMaterial,
    ParticleTextureArray,
};

impl From<ParticleType> for ParticleGeometry {
    fn from(particle_type: ParticleType) -> Self {
        match particle_type {
            ParticleType::Billboard => Self::Billboard,
            ParticleType::UpFacing => Self::UpFacing,
            ParticleType::OrientedAxial => Self::OrientedAxial,
            ParticleType::VelocityAligned => Self::VelocityAligned,
            ParticleType::Beam => Self::Beam,
            ParticleType::BeamVertical => Self::BeamVertical,
            ParticleType::BeamHorizontal => Self::BeamHorizontal,
            ParticleType::Trail => Self::Trail,
            ParticleType::TrailCross => Self::TrailCross,
            ParticleType::TerrainPatch => Self::TerrainPatch,
        }
    }
}

impl From<BlendMode> for ParticleBlendMode {
    fn from(blend: BlendMode) -> Self {
        match blend {
            BlendMode::Alpha => Self::Alpha,
            BlendMode::Additive => Self::Additive,
            BlendMode::PremultipliedAlpha => Self::PremultipliedAlpha,
            BlendMode::Subtractive => Self::Subtractive,
            BlendMode::Distortion => Self::Distortion,
        }
    }
}

impl From<LayerBlendMode> for ParticleLayerBlend {
    fn from(blend: LayerBlendMode) -> Self {
        match blend {
            LayerBlendMode::Multiply => Self::Multiply,
            LayerBlendMode::Alpha => Self::Alpha,
        }
    }
}

/// Resolves the DDX files of a PFX material into a GPU-ready material payload.
///
/// Unavailable optional texture sets are disabled, matching retail draw routing.
///
/// # Errors
///
/// Returns an error when the primary diffuse set has no decodable image.
pub fn load_particle_material(
    definition: &ParticleMaterialDefinition,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<ParticleMaterial, ParticleError> {
    let mut unavailable_texture_sets = 0;
    let diffuse = [
        load_texture_set(&definition.diffuse[0], source)?,
        load_optional_texture_set(
            &definition.diffuse[1],
            source,
            &mut unavailable_texture_sets,
        ),
        load_optional_texture_set(
            &definition.diffuse[2],
            source,
            &mut unavailable_texture_sets,
        ),
    ];
    let intensity =
        load_optional_texture_set(&definition.intensity, source, &mut unavailable_texture_sets);
    Ok(ParticleMaterial {
        diffuse,
        intensity,
        unavailable_texture_sets,
        layer_1_to_2: definition.layer_1_to_2.into(),
        layer_2_to_3: definition.layer_2_to_3.into(),
        blend: definition.blend.into(),
        soft_particles: definition.soft_particles,
        light_volume: definition.light_volume,
        light_volume_intensity: definition.light_volume_intensity,
        corner_colors: definition.corner_colors,
    })
}

fn load_optional_texture_set(
    definition: &ParticleTextureDefinition,
    source: &mut AssetSource<StdFileProvider>,
    unavailable_texture_sets: &mut usize,
) -> Option<ParticleTextureArray> {
    if let Ok(texture) = load_texture_set(definition, source) {
        texture
    } else {
        *unavailable_texture_sets += 1;
        None
    }
}

fn load_texture_set(
    definition: &ParticleTextureDefinition,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<Option<ParticleTextureArray>, ParticleError> {
    if definition.stages.is_empty() {
        return Ok(None);
    }
    let paths = definition
        .stages
        .iter()
        .map(|stage| stage.path.clone())
        .collect::<Vec<_>>();
    ParticleTextureArray::load(source, &paths).map(Some)
}
