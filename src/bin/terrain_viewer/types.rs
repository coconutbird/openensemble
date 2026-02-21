//! Data types for the terrain viewer.

use data::xtd::{TerrainVertices, TessellatedMesh};
use glam::Vec3;
use render::wgpu;

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

/// Albedo atlas data from XTT file.
pub struct AlbedoData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A single terrain texture loaded from ERA.
#[allow(dead_code)]
pub struct TerrainTexture {
    /// Texture name (e.g., "grass_01").
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data.
    pub pixels: Vec<u8>,
    /// U scale from XTT.
    pub u_scale: i32,
    /// V scale from XTT.
    pub v_scale: i32,
}

/// A normal map texture loaded from ERA (_nm.ddx files).
#[allow(dead_code)]
pub struct NormalMapTexture {
    /// Texture name (e.g., "grass_01").
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data (normal map encoded as RGB, A may be height/unused).
    pub pixels: Vec<u8>,
}

/// Splat data for a single terrain chunk.
#[derive(Clone)]
#[allow(dead_code)]
pub struct ChunkSplatData {
    /// Grid X position (0-15 for 16x16 grid).
    pub grid_x: i32,
    /// Grid Z position (0-15 for 16x16 grid).
    pub grid_z: i32,
    /// Indices into terrain_textures for this chunk's layers.
    pub layer_texture_ids: Vec<i32>,
    /// Alpha maps for layers 1..n (layer 0 has no alpha, it's the base).
    /// Each is 64x64 = 4096 bytes.
    pub alpha_maps: Vec<Vec<u8>>,
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
