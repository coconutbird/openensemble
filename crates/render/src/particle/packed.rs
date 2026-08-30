//! GPU uniform and instance packing for particle rendering.

use num_traits::ToPrimitive;

use super::{
    MATERIAL_ALPHA_TEST, MATERIAL_HAS_INTENSITY, MATERIAL_LIGHT_VOLUME,
    MATERIAL_PREMULTIPLY_COLOR_ALPHA, MATERIAL_SOFT_FADE_RGB, MATERIAL_SOFT_PARTICLES,
    ParticleBlendMode, ParticleMaterial, ParticleScene, ParticleTextureArray,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PackedParticleInstance {
    pub(super) position_rotation: [f32; 4],
    pub(super) axis_half_length: [f32; 4],
    pub(super) up_axis: [f32; 4],
    pub(super) half_size_softness: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) intensity: [f32; 4],
    pub(super) uv_rect0: [f32; 4],
    pub(super) uv_rect1: [f32; 4],
    pub(super) uv_rect2: [f32; 4],
    pub(super) uv_rect_intensity: [f32; 4],
    pub(super) texture_layers: [u32; 4],
    pub(super) geometry: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PackedParticleScene {
    view_projection: [[f32; 4]; 4],
    world_to_view: [[f32; 4]; 4],
    view_to_world: [[f32; 4]; 4],
    camera_position: [f32; 4],
    pub(super) viewport_depth: [f32; 4],
    pub(super) light_volume_row0: [f32; 4],
    light_volume_row1: [f32; 4],
    light_volume_row2: [f32; 4],
    pub(super) light_volume_params: [f32; 4],
}

impl PackedParticleScene {
    pub(super) fn from_scene(scene: &ParticleScene) -> Self {
        Self {
            view_projection: scene.view_projection.to_cols_array_2d(),
            world_to_view: scene.world_to_view.to_cols_array_2d(),
            view_to_world: scene.world_to_view.inverse().to_cols_array_2d(),
            camera_position: [
                scene.camera_position[0],
                scene.camera_position[1],
                scene.camera_position[2],
                0.0,
            ],
            viewport_depth: [
                scene.viewport_size[0].to_f32().unwrap_or(f32::MAX),
                scene.viewport_size[1].to_f32().unwrap_or(f32::MAX),
                scene.depth_unproject[0],
                scene.depth_unproject[1],
            ],
            light_volume_row0: scene.light_volume_rows[0],
            light_volume_row1: scene.light_volume_rows[1],
            light_volume_row2: scene.light_volume_rows[2],
            light_volume_params: [scene.light_volume_intensity_scale, 0.0, 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PackedParticleMaterial {
    pub(super) flags: [u32; 4],
    hdr_scales: [f32; 4],
    light_params: [f32; 4],
    corner_color_1: [f32; 4],
    corner_color_2: [f32; 4],
    corner_color_3: [f32; 4],
    corner_color_4: [f32; 4],
}

impl PackedParticleMaterial {
    pub(super) fn from_material(material: &ParticleMaterial) -> Self {
        let layer_count = material
            .diffuse
            .iter()
            .rposition(Option::is_some)
            .map_or(1, |index| index + 1);
        let additive_pixel_path = matches!(material.blend, ParticleBlendMode::Additive)
            || (material.blend == ParticleBlendMode::PremultipliedAlpha
                && (layer_count == 1 || (!material.soft_particles && layer_count == 2)));
        let mut flags = 0;
        for (enabled, flag) in [
            (material.intensity.is_some(), MATERIAL_HAS_INTENSITY),
            (
                material.light_volume
                    && matches!(
                        material.blend,
                        ParticleBlendMode::Alpha | ParticleBlendMode::PremultipliedAlpha
                    ),
                MATERIAL_LIGHT_VOLUME,
            ),
            (material.soft_particles, MATERIAL_SOFT_PARTICLES),
            (additive_pixel_path, MATERIAL_SOFT_FADE_RGB),
            (
                matches!(
                    material.blend,
                    ParticleBlendMode::Additive | ParticleBlendMode::PremultipliedAlpha
                ),
                MATERIAL_PREMULTIPLY_COLOR_ALPHA,
            ),
            (
                material.blend != ParticleBlendMode::PremultipliedAlpha,
                MATERIAL_ALPHA_TEST,
            ),
        ] {
            if enabled {
                flags |= flag;
            }
        }
        Self {
            flags: [
                flags,
                u32::try_from(layer_count).unwrap_or(1),
                material.layer_1_to_2 as u32,
                material.layer_2_to_3 as u32,
            ],
            hdr_scales: [
                material.diffuse[0]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material.diffuse[1]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material.diffuse[2]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material
                    .intensity
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
            ],
            light_params: [material.light_volume_intensity.max(0.0), 0.0, 0.0, 0.0],
            corner_color_1: material.corner_colors[0],
            corner_color_2: material.corner_colors[1],
            corner_color_3: material.corner_colors[2],
            corner_color_4: material.corner_colors[3],
        }
    }
}
