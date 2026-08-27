//! Data types for the terrain viewer.
//!
//! This module contains GPU-specific types that remain in the viewer,
//! and re-exports terrain types from the `render` crate.

use render::wgpu;

// Re-export terrain types from render crate
pub use render::terrain::{AlbedoData, FoliageQNChunk, FoliageSet, RawXtdData};

/// Borrowed terrain data used to initialize the CPU-rendered terrain pipeline.
pub struct CpuTerrainData<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub uvs: &'a [[f32; 2]],
    pub indices: &'a [u32],
    pub albedo: Option<&'a AlbedoData>,
    pub surface_size: [u32; 2],
}

/// GPU resources for terrain rendering.
pub struct GpuResources {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_bind_group: wgpu::BindGroup,
    pub texture_bind_group: wgpu::BindGroup,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub params_buffer: wgpu::Buffer,
    pub lighting_buffer: Option<wgpu::Buffer>,
    pub terrain_size: [f32; 2],
    pub tile_scale: f32,
    /// GPU tessellation mode - use instanced patch rendering.
    pub use_gpu_tessellation: bool,
    /// Number of patch instances to draw (64x64 = 4096).
    pub num_patch_instances: u32,
}
