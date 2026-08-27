use render::wgpu;

use crate::foliage::FoliageConfig;

/// A single foliage draw call (one per QN chunk × set pair).
pub struct FoliageDrawCall {
    /// Dynamic offset into `chunk_info_buffer` for this draw.
    pub dynamic_offset: u32,
    /// Which foliage set to render (index into `set_resources`).
    pub set_index: usize,
    /// Number of vertices per blade for this set.
    pub num_verts_per_blade: u32,
    /// Number of active blades in this draw call (from index buffer parsing).
    pub num_active_blades: u32,
}

/// GPU resources for foliage rendering.
pub struct FoliageResources {
    /// Render pipeline for foliage.
    pub pipeline: wgpu::RenderPipeline,
    /// Oracle-compatible alpha-tested VSM caster pipeline.
    pub shadow_pipeline: wgpu::RenderPipeline,
    /// Main scene camera binding used by the world phase.
    pub camera_bind_group: wgpu::BindGroup,
    /// Light cameras indexed by directional shadow cascade.
    pub shadow_camera_bind_groups: Vec<wgpu::BindGroup>,
    /// Uniform buffer for foliage parameters (global — lighting, fog, etc).
    pub params_buffer: wgpu::Buffer,
    /// Dynamic uniform buffer for per-chunk data (chunk offsets).
    pub chunk_info_buffer: Option<wgpu::Buffer>,
    /// Bind group layout for foliage params + terrain textures.
    pub params_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group layout for foliage material textures + blade geometry.
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    /// Per-set resources (textures, bind groups).
    pub set_resources: Vec<FoliageSetResources>,
    /// Params bind group (terrain textures + dynamic chunk info).
    pub params_bind_group: Option<wgpu::BindGroup>,
    /// Caster params bind group, with a dummy shadow texture to avoid sampling
    /// the cascaded shadow map while it is attached as a render target.
    pub shadow_params_bind_group: Option<wgpu::BindGroup>,
    /// Pre-built draw calls from QN chunk data.
    pub draw_calls: Vec<FoliageDrawCall>,
    /// Minimum uniform buffer offset alignment (for dynamic uniform).
    pub min_offset_alignment: u32,
    /// Current configuration.
    pub config: FoliageConfig,
    /// Blade map texture: compact list of (`grid_position`, `blade_type`) per active blade.
    /// Each texel is `Rg32Uint`: r = `grid_position` (0..4095), g = `blade_type`.
    /// Draw calls index into this via `blade_data_offset` in `ChunkInfo`.
    pub blade_map_texture: Option<wgpu::Texture>,
    pub blade_map_view: Option<wgpu::TextureView>,
    pub(super) params: Option<FoliageParamsUniform>,
}

/// Per-foliage-set GPU resources.
pub struct FoliageSetResources {
    /// Albedo texture.
    pub _albedo_texture: wgpu::Texture,
    pub _albedo_view: wgpu::TextureView,
    /// Opacity texture.
    pub _opacity_texture: wgpu::Texture,
    pub _opacity_view: wgpu::TextureView,
    /// Blade positions texture (xyz = pos, w = u).
    pub _blade_positions_texture: wgpu::Texture,
    pub _blade_positions_view: wgpu::TextureView,
    /// Blade normals texture (xyz = normal, w = v).
    pub _blade_normals_texture: wgpu::Texture,
    pub _blade_normals_view: wgpu::TextureView,
    /// Material bind group.
    pub material_bind_group: wgpu::BindGroup,
    pub _material_params_buffer: wgpu::Buffer,
    /// Number of blade types in this set.
    pub num_blade_types: u32,
    /// Number of vertices per blade.
    pub num_verts_per_blade: u32,
}

/// Foliage uniform parameters (must match shader struct).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FoliageParamsUniform {
    /// `terrain_info`: `num_verts_per_axis`, `tile_scale`, `chunk_offset_x`, `chunk_offset_z`
    pub terrain_info: [f32; 4],
    /// Position compression midpoint (`g_posCompMin`): x, y, z, padding.
    pub position_mid: [f32; 4],
    /// Position compression range (`g_posCompRange`): x, y, z, padding.
    pub position_range: [f32; 4],
    /// `foliage_info`: `num_verts_per_blade`, `rcp_num_blades`, `fade_start`, `fade_end`
    pub foliage_info: [f32; 4],
    /// `camera_pos`: x, y, z, time
    pub camera_pos_time: [f32; 4],
    /// `dir_light_vec`: x, y, z, `backside_shadow_scalar`
    pub dir_light_vec: [f32; 4],
    /// `dir_light_color`: r, g, b, padding
    pub dir_light_color: [f32; 4],
    /// `fog_params`: density^2, start^2, unused, unused
    pub fog_params: [f32; 4],
    /// `fog_color`: r, g, b, a
    pub fog_color: [f32; 4],
    /// `planar_fog_params`: enabled, `start_y`, density^2, unused
    pub planar_fog_params: [f32; 4],
    /// `planar_fog_color`: r, g, b, a
    pub planar_fog_color: [f32; 4],
    /// SH fill lighting coefficients (7 vec4s)
    pub sh_fill_ar: [f32; 4],
    pub sh_fill_ag: [f32; 4],
    pub sh_fill_ab: [f32; 4],
    pub sh_fill_br: [f32; 4],
    pub sh_fill_bg: [f32; 4],
    pub sh_fill_bb: [f32; 4],
    pub sh_fill_c: [f32; 4],
    // Shadow params (8 vec4s)
    pub shadow_vp_col0: [f32; 4],
    pub shadow_vp_col1: [f32; 4],
    pub shadow_vp_col2: [f32; 4],
    pub shadow_vp_col3: [f32; 4],
    pub shadow_params: [f32; 4], // x = csm_scale, y = num_passes, z = enabled
    // Blackmap params (3 vec4s)
    pub blackmap_params0: [f32; 4], // rgb = bg_color, w = fog_scalar
    pub blackmap_params1: [f32; 4], // x = unexplored_scalar, yz = bounds_lo_xz, w = enabled
    pub blackmap_params2: [f32; 4], // x = pad, yz = bounds_hi_xz, w = bounds_falloff
    /// Packed local-light controls: count, specular power, shadows, enabled.
    pub local_light_params: [f32; 4],
    /// Visibility and unexplored-map world-coordinate scales.
    pub blackmap_uv_scales: [f32; 4],
}

pub struct FoliageWorldBindings<'a> {
    pub shadow: &'a wgpu::TextureView,
    pub blackmap: &'a wgpu::TextureView,
    pub unexplored: &'a wgpu::TextureView,
    pub local_lights: &'a wgpu::Buffer,
}

pub(super) struct FallbackFoliageWorldResources {
    pub(super) shadow: wgpu::TextureView,
    pub(super) blackmap: wgpu::TextureView,
    pub(super) unexplored: wgpu::TextureView,
    pub(super) local_lights: wgpu::Buffer,
}

impl FallbackFoliageWorldResources {
    pub(super) fn bindings(&self) -> FoliageWorldBindings<'_> {
        FoliageWorldBindings {
            shadow: &self.shadow,
            blackmap: &self.blackmap,
            unexplored: &self.unexplored,
            local_lights: &self.local_lights,
        }
    }
}

/// Per-chunk uniform data (must match shader `ChunkInfo` struct).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkInfoUniform {
    /// Chunk offset: mMinXVert, mMinZVert from quad node
    pub chunk_offset: [f32; 2],
    /// Number of vertices per blade (varies per set)
    pub num_verts_per_blade: f32,
    /// Offset into blade map texture for this draw call's blade data.
    pub blade_data_offset: f32,
}

pub(super) struct FoliageDrawInfo {
    pub(super) chunk_x: u32,
    pub(super) chunk_z: u32,
    pub(super) set_index: usize,
    pub(super) num_verts_per_blade: u32,
    pub(super) blades: Vec<[u32; 2]>,
}
