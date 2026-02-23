//! Data types for the terrain viewer.
//!
//! This module contains GPU-specific types that remain in the viewer,
//! and re-exports data types from the `data` crate for backwards compatibility.

use data::xtd::{TerrainVertices, TessellatedMesh};
use glam::Vec3;
use render::wgpu;

// Re-export types from data crate for backwards compatibility
pub use data::terrain::{
    AlbedoData, ChunkDecalData, ChunkSplatData, DecalInstance, DecalTexture, FoliageQNChunk,
    FoliageSet, NormalMapTexture, TerrainTexture,
};

/// Terrain mesh data.
pub struct TerrainMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub world_min: [f32; 3],
    pub world_max: [f32; 3],
    pub tile_scale: f32,
}

impl TerrainMesh {
    pub fn from_xtd(
        vertices: &TerrainVertices,
        world_min: [f32; 3],
        world_max: [f32; 3],
        tile_scale: f32,
    ) -> Self {
        let indices = vertices.generate_indices();
        Self {
            positions: vertices.positions.clone(),
            normals: vertices.normals.clone(),
            uvs: vertices.uvs.clone(),
            indices,
            world_min,
            world_max,
            tile_scale,
        }
    }

    pub fn from_tessellated(
        tessellated: TessellatedMesh,
        world_min: [f32; 3],
        world_max: [f32; 3],
        tile_scale: f32,
    ) -> Self {
        Self {
            positions: tessellated.positions,
            normals: tessellated.normals,
            uvs: tessellated.uvs,
            indices: tessellated.indices,
            world_min,
            world_max,
            tile_scale,
        }
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.world_min[0] + self.world_max[0]) / 2.0,
            (self.world_min[1] + self.world_max[1]) / 2.0,
            (self.world_min[2] + self.world_max[2]) / 2.0,
        )
    }

    pub fn size(&self) -> Vec3 {
        Vec3::new(
            self.world_max[0] - self.world_min[0],
            self.world_max[1] - self.world_min[1],
            self.world_max[2] - self.world_min[2],
        )
    }
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
    pub terrain_size: [f32; 2],
    pub tile_scale: f32,
    /// GPU tessellation mode - use instanced patch rendering.
    pub use_gpu_tessellation: bool,
    /// Number of patch instances to draw (64x64 = 4096).
    pub num_patch_instances: u32,
}

/// Raw XTD vertex data for GPU tessellation (before decoding to world positions).
pub struct RawXtdData {
    /// Packed position data (R10G10B10A2 format).
    pub packed_positions: Vec<u32>,
    /// Packed normal data.
    pub packed_normals: Vec<u32>,
    /// Number of vertices per axis (e.g., 1025).
    pub num_verts_per_axis: u32,
    /// Atlas mid point for decoding.
    pub mid: [f32; 3],
    /// Atlas range for decoding.
    pub range: [f32; 3],
    /// Tile scale for world position.
    pub tile_scale: f32,
    /// Ambient occlusion data (R8 values, half resolution).
    /// Based on IDA RE: stored at 1024×512 for a 1024×1024 terrain (full width, half height).
    pub ao_data: Option<AoTextureData>,
    /// Alpha/transparency data (R8 values, half resolution).
    /// Same compression as AO. Used for terrain holes (water edges, cliffs).
    /// Sampled via gVertSampler_alpha_Texture in the game's vertex shader.
    pub alpha_data: Option<AlphaTextureData>,
}

/// Half-resolution AO texture data as decoded from the game.
/// Dimensions: full width × half height (e.g., 1024×512 for 1024×1024 terrain).
/// The game samples this with bilinear filtering via gVertSampler_ao_Texture.
#[derive(Clone)]
pub struct AoTextureData {
    pub values: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Half-resolution Alpha texture data as decoded from the game.
/// Dimensions: full width × half height (same as AO).
/// Used for terrain transparency (holes, cliff edges).
/// 255 = fully opaque, 0 = fully transparent/hole.
#[derive(Clone)]
pub struct AlphaTextureData {
    pub values: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
