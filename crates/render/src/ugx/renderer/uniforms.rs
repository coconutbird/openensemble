use glam::Mat4;
use num_traits::ToPrimitive;
use pipeline::ugx::MapType;

use super::TerrainHeightfieldInfo;
use crate::terrain::LightingParams;
use crate::ugx::model::{BlendMode, Material, MaterialFeature};

pub(super) const MATERIAL_FLAG_DIFFUSE: u32 = 1 << 0;
pub(super) const MATERIAL_FLAG_NORMAL: u32 = 1 << 1;
pub(super) const MATERIAL_FLAG_GLOSS: u32 = 1 << 2;
pub(super) const MATERIAL_FLAG_OPACITY: u32 = 1 << 3;
pub(super) const MATERIAL_FLAG_XFORM: u32 = 1 << 4;
pub(super) const MATERIAL_FLAG_EMISSIVE: u32 = 1 << 5;
pub(super) const MATERIAL_FLAG_AO: u32 = 1 << 6;
pub(super) const MATERIAL_FLAG_COLOR_GLOSS: u32 = 1 << 7;
pub(super) const MATERIAL_FLAG_TWO_SIDED: u32 = 1 << 8;
pub(super) const MATERIAL_FLAG_ENVIRONMENT: u32 = 1 << 9;
pub(super) const MATERIAL_FLAG_ENVIRONMENT_MASK: u32 = 1 << 10;
pub(super) const MATERIAL_FLAG_EMISSIVE_XFORM: u32 = 1 << 11;
pub(super) const MATERIAL_FLAG_HIGHLIGHT: u32 = 1 << 12;
pub(super) const MATERIAL_FLAG_MODULATE: u32 = 1 << 13;
pub(super) const MATERIAL_FLAG_DISTORTION: u32 = 1 << 14;
pub(super) const MATERIAL_FLAG_RECEIVES_SHADOWS: u32 = 1 << 15;
pub(super) const MATERIAL_FLAG_TERRAIN_CONFORM: u32 = 1 << 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ugx) struct SelectionOverlay {
    color: [f32; 4],
    uv_scale: f32,
    uv_offset: f32,
    intensity: f32,
}

impl SelectionOverlay {
    pub(in crate::ugx) const fn new(
        color: [f32; 4],
        uv_scale: f32,
        uv_offset: f32,
        intensity: f32,
    ) -> Self {
        Self {
            color,
            uv_scale,
            uv_offset,
            intensity,
        }
    }

    pub(in crate::ugx) const fn color(self) -> [f32; 4] {
        self.color
    }

    pub(in crate::ugx) const fn params(self) -> [f32; 4] {
        [self.uv_scale, self.uv_offset, self.intensity, 1.0]
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct SceneUniform {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    camera_position: [f32; 4],
    dir_light_vector: [f32; 4],
    dir_light_color: [f32; 4],
    sh_fill_ar: [f32; 4],
    sh_fill_ag: [f32; 4],
    sh_fill_ab: [f32; 4],
    sh_fill_br: [f32; 4],
    sh_fill_bg: [f32; 4],
    sh_fill_bb: [f32; 4],
    sh_fill_c: [f32; 4],
    fog_color: [f32; 4],
    fog_params: [f32; 4],
    planar_fog_color: [f32; 4],
    planar_fog_params: [f32; 4],
    ao_params: [f32; 4],
    frame_params: [f32; 4],
    shadow_vp_col0: [f32; 4],
    shadow_vp_col1: [f32; 4],
    shadow_vp_col2: [f32; 4],
    shadow_vp_col3: [f32; 4],
    shadow_params: [f32; 4],
    terrain_info: [f32; 4],
    terrain_decode: [f32; 4],
    local_light_params: [f32; 4],
    light_volume_params: [f32; 4],
    light_volume_row0: [f32; 4],
    light_volume_row1: [f32; 4],
    light_volume_row2: [f32; 4],
    selection_color: [f32; 4],
    selection_params: [f32; 4],
}

impl SceneUniform {
    pub(super) fn new(model: Mat4, terrain: TerrainHeightfieldInfo) -> Self {
        Self::from_frame(
            Mat4::IDENTITY,
            model,
            &LightingParams::default(),
            0.0,
            terrain,
            SelectionOverlay::default(),
        )
    }

    pub(super) fn from_frame(
        view_projection: Mat4,
        model: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
        terrain: TerrainHeightfieldInfo,
        selection: SelectionOverlay,
    ) -> Self {
        Self {
            view_projection: view_projection.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            camera_position: lighting.world_camera_pos,
            dir_light_vector: lighting.dir_light_vec,
            dir_light_color: lighting.dir_light_color,
            sh_fill_ar: lighting.sh_fill_ar,
            sh_fill_ag: lighting.sh_fill_ag,
            sh_fill_ab: lighting.sh_fill_ab,
            sh_fill_br: lighting.sh_fill_br,
            sh_fill_bg: lighting.sh_fill_bg,
            sh_fill_bb: lighting.sh_fill_bb,
            sh_fill_c: lighting.sh_fill_c,
            fog_color: lighting.fog_color,
            fog_params: lighting.fog_params,
            planar_fog_color: lighting.planar_fog_color,
            planar_fog_params: lighting.planar_fog_params,
            ao_params: lighting.ao_params,
            frame_params: [time_seconds, 0.0, 0.0, 0.0],
            shadow_vp_col0: lighting.shadow_vp_col0,
            shadow_vp_col1: lighting.shadow_vp_col1,
            shadow_vp_col2: lighting.shadow_vp_col2,
            shadow_vp_col3: lighting.shadow_vp_col3,
            shadow_params: lighting.shadow_params,
            terrain_info: [
                terrain.dimension.to_f32().unwrap_or(f32::MAX),
                terrain.tile_scale.recip(),
                terrain.y_range,
                terrain.y_mid,
            ],
            terrain_decode: [
                terrain.normalized_y_bias,
                terrain.world_min_xz[0],
                terrain.world_min_xz[1],
                0.0,
            ],
            local_light_params: lighting.local_light_params,
            light_volume_params: lighting.light_volume_params,
            light_volume_row0: lighting.light_volume_row0,
            light_volume_row1: lighting.light_volume_row1,
            light_volume_row2: lighting.light_volume_row2,
            selection_color: selection.color(),
            selection_params: selection.params(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct MaterialUniform {
    pub(super) tint: [f32; 4],
    pub(super) specular: [f32; 4],
    pub(super) params: [f32; 4],
    pub(super) flags: [u32; 4],
    pub(super) channels0: [u32; 4],
    pub(super) channels1: [u32; 4],
    pub(super) channels2: [u32; 4],
    pub(super) environment: [f32; 4],
    pub(super) hdr_scales: [f32; 4],
    pub(super) uv_velocity0: [f32; 4],
    pub(super) uv_velocity1: [f32; 4],
    pub(super) uv_velocity2: [f32; 4],
    pub(super) uv_velocity3: [f32; 4],
    pub(super) uv_velocity4: [f32; 4],
    pub(super) uv_velocity5: [f32; 4],
}

impl MaterialUniform {
    pub(super) fn from_material(
        material: &Material,
        environment_available: bool,
        environment_hdr_scale: f32,
    ) -> Self {
        let flags = material_flags(material, environment_available);
        let alpha_reference = if material.blend == BlendMode::AlphaTest {
            0.5
        } else {
            0.0
        };
        let emissive_hdr_scale = material
            .emissive
            .as_ref()
            .map_or(1.0, |image| image.hdr_scale.max(1.0));
        Self {
            tint: [1.0; 4],
            specular: [
                material.specular_color[0],
                material.specular_color[1],
                material.specular_color[2],
                material.specular_power,
            ],
            params: [material.opacity, alpha_reference, emissive_hdr_scale, 1.0],
            flags: [flags, 0, 0, 0],
            channels0: [
                material.channels.diffuse,
                material.channels.normal,
                material.channels.gloss,
                material.channels.opacity,
            ],
            channels1: [
                material.channels.xform,
                material.channels.emissive,
                material.channels.ao,
                material.channels.environment_mask,
            ],
            channels2: [
                material.channels.emissive_xform,
                material.channels.distortion,
                material.channels.highlight,
                material.channels.modulate,
            ],
            environment: [
                material.environment_fresnel_power,
                material.environment_sharpness,
                material.environment_fresnel,
                material.environment_reflectivity,
            ],
            hdr_scales: [
                emissive_hdr_scale,
                environment_hdr_scale,
                material
                    .highlight
                    .as_ref()
                    .map_or(1.0, |image| image.hdr_scale.max(1.0)),
                1.0,
            ],
            uv_velocity0: velocity_pair(material, MapType::Diffuse, MapType::Normal),
            uv_velocity1: velocity_pair(material, MapType::Gloss, MapType::Opacity),
            uv_velocity2: velocity_pair(material, MapType::Emissive, MapType::EnvMask),
            uv_velocity3: velocity_pair(material, MapType::AO, MapType::XForm),
            uv_velocity4: velocity_pair(material, MapType::EmXForm, MapType::Distortion),
            uv_velocity5: velocity_pair(material, MapType::Highlight, MapType::Modulate),
        }
    }
}

fn material_flags(material: &Material, environment_available: bool) -> u32 {
    [
        (material.diffuse.is_some(), MATERIAL_FLAG_DIFFUSE),
        (material.normal.is_some(), MATERIAL_FLAG_NORMAL),
        (material.gloss.is_some(), MATERIAL_FLAG_GLOSS),
        (material.opacity_map.is_some(), MATERIAL_FLAG_OPACITY),
        (material.xform.is_some(), MATERIAL_FLAG_XFORM),
        (material.emissive.is_some(), MATERIAL_FLAG_EMISSIVE),
        (material.ao.is_some(), MATERIAL_FLAG_AO),
        (
            material.has_feature(MaterialFeature::COLOR_GLOSS),
            MATERIAL_FLAG_COLOR_GLOSS,
        ),
        (
            material.has_feature(MaterialFeature::TWO_SIDED),
            MATERIAL_FLAG_TWO_SIDED,
        ),
        (environment_available, MATERIAL_FLAG_ENVIRONMENT),
        (
            material.environment_mask.is_some(),
            MATERIAL_FLAG_ENVIRONMENT_MASK,
        ),
        (
            material.emissive_xform.is_some(),
            MATERIAL_FLAG_EMISSIVE_XFORM,
        ),
        (material.highlight.is_some(), MATERIAL_FLAG_HIGHLIGHT),
        (material.modulate.is_some(), MATERIAL_FLAG_MODULATE),
        (material.distortion.is_some(), MATERIAL_FLAG_DISTORTION),
        (
            material.has_feature(MaterialFeature::RECEIVES_SHADOWS),
            MATERIAL_FLAG_RECEIVES_SHADOWS,
        ),
        (
            material.has_feature(MaterialFeature::TERRAIN_CONFORM),
            MATERIAL_FLAG_TERRAIN_CONFORM,
        ),
    ]
    .into_iter()
    .filter_map(|(enabled, flag)| enabled.then_some(flag))
    .fold(0, |flags, flag| flags | flag)
}

fn velocity_pair(material: &Material, first: MapType, second: MapType) -> [f32; 4] {
    let first = material.map_velocity(first);
    let second = material.map_velocity(second);
    [first[0], first[1], second[0], second[1]]
}
