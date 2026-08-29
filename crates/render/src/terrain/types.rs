//! Data types for terrain rendering.
//!
//! These types represent processed terrain data ready for rendering.
//! They bridge the gap between raw file data (XTD/XTT) and GPU resources.

use crate::wgpu;

/// Accepted-axis terrain position texture and the constants required to decode
/// its normalized Y channel back into world space.
///
/// The position texture stores logical `(x, z)` at texture coordinate `(z, x)`.
/// Keeping that convention in one shared type prevents model, decal, trail,
/// and terrain-effect renderers from independently reintroducing the old
/// diagonal mirror bug.
#[derive(Clone, Copy)]
pub struct TerrainHeightfield<'a> {
    /// Packed terrain position texture used by the canonical terrain renderer.
    pub view: &'a wgpu::TextureView,
    /// Number of packed samples per terrain axis.
    pub dimension: u32,
    /// World spacing between adjacent terrain samples.
    pub tile_scale: f32,
    /// World-space X/Z coordinate represented by texel `(0, 0)`.
    pub world_min_xz: [f32; 2],
    /// XTD Y decode range.
    pub y_range: f32,
    /// XTD Y decode midpoint.
    pub y_mid: f32,
    /// Normalized Y bias used by the PC terrain shader.
    pub normalized_y_bias: f32,
}

/// Albedo atlas data decoded from XTT file.
#[derive(Clone)]
pub struct AlbedoData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A single terrain texture loaded from ERA.
#[derive(Clone)]
pub struct TerrainTexture {
    /// Texture name (e.g., "`grass_01`").
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
#[derive(Clone)]
pub struct NormalMapTexture {
    /// Texture name (e.g., "`grass_01`").
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data (normal map encoded as RGB, A may be height/unused).
    pub pixels: Vec<u8>,
}

/// A specular map texture loaded from ERA (`_sp.ddx` files).
#[derive(Clone)]
pub struct SpecularMapTexture {
    /// Texture name (for example, `grass_01`).
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data. RGB stores the colored specular response.
    pub pixels: Vec<u8>,
}

/// Splat data for a single terrain chunk.
#[derive(Clone)]
pub struct ChunkSplatData {
    /// Grid X position (0-15 for 16x16 grid).
    pub grid_x: i32,
    /// Grid Z position (0-15 for 16x16 grid).
    pub grid_z: i32,
    /// Indices into `terrain_textures` for this chunk's layers.
    pub layer_texture_ids: Vec<i32>,
    /// Alpha maps for layers 1..n (layer 0 has no alpha, it's the base).
    /// Each is 64x64 = 4096 bytes.
    pub alpha_maps: Vec<Vec<u8>>,
}

/// A terrain decal texture loaded from ERA.
/// Decals have separate diffuse (_df) and opacity (_op) textures.
#[derive(Clone)]
pub struct DecalTexture {
    /// Decal name (e.g., "`road_01`").
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA diffuse texture pixels.
    pub diffuse_pixels: Vec<u8>,
    /// Opacity texture pixels (single channel, expanded to RGBA).
    pub opacity_pixels: Vec<u8>,
}

/// A decal instance to be rendered on the terrain.
/// This corresponds to XTT's `ActiveDecalInstance`.
#[derive(Clone, Debug)]
pub struct DecalInstance {
    /// Index into the `decal_textures` array.
    pub decal_index: i32,
    /// Rotation angle in radians.
    pub rotation: f32,
    /// Tile center X in terrain UV space (0-1).
    pub tile_center_x: f32,
    /// Tile center Y in terrain UV space (0-1).
    pub tile_center_y: f32,
    /// U scale factor.
    pub u_scale: f32,
    /// V scale factor.
    pub v_scale: f32,
}

/// Per-chunk decal data.
#[derive(Clone)]
pub struct ChunkDecalData {
    /// Grid X position (0-15 for 16x16 grid).
    pub grid_x: i32,
    /// Grid Z position (0-15 for 16x16 grid).
    pub grid_z: i32,
    /// Indices into `decal_instances` for this chunk's decals.
    pub decal_layer_ids: Vec<i32>,
    /// Alpha maps for decal layers (if any).
    /// Each is 64x64 = 4096 bytes.
    pub alpha_maps: Vec<Vec<u8>>,
}

// ============================================================================
// Foliage Types
// ============================================================================

/// A single foliage blade vertex (position + normal + UV).
/// Decoded from the foliage asset XML/XMB data.
#[derive(Clone, Debug)]
pub struct FoliageBladeVertex {
    /// Local position relative to blade base.
    pub position: [f32; 3],
    /// Vertex normal.
    pub normal: [f32; 3],
    /// UV coordinates.
    pub uv: [f32; 2],
}

/// A foliage set containing textures and blade geometry.
/// Corresponds to `BTerrainFoliageSet` in the original.
#[derive(Clone)]
pub struct FoliageSet {
    /// Set name (e.g., "foliage\\`bg_grass_02`").
    pub name: String,
    /// Backside shadow scalar for lighting.
    pub backside_shadow_scalar: f32,
    /// Number of blade types in this set.
    pub num_blade_types: u32,
    /// Number of vertices per blade (typically 10).
    pub num_verts_per_blade: u32,
    /// Blade vertex data stored as line texture format:
    /// positions.xyz + uv.x stored in positions texture
    /// normals.xyz + runtime (source-inverted) uv.y stored in normals texture
    pub blade_positions: Vec<[f32; 4]>,
    pub blade_normals: Vec<[f32; 4]>,
    /// Albedo/diffuse texture (RGBA).
    pub albedo_width: u32,
    pub albedo_height: u32,
    pub albedo_pixels: Vec<u8>,
    /// Normal map texture (RGBA).
    pub normal_width: u32,
    pub normal_height: u32,
    pub normal_pixels: Vec<u8>,
    /// Specular texture (RGBA).
    pub specular_width: u32,
    pub specular_height: u32,
    pub specular_pixels: Vec<u8>,
    /// Opacity texture (RGBA).
    pub opacity_width: u32,
    pub opacity_height: u32,
    pub opacity_pixels: Vec<u8>,
}

impl Default for FoliageSet {
    fn default() -> Self {
        Self {
            name: String::new(),
            backside_shadow_scalar: 1.0,
            num_blade_types: 1,
            num_verts_per_blade: 10,
            blade_positions: Vec::new(),
            blade_normals: Vec::new(),
            albedo_width: 0,
            albedo_height: 0,
            albedo_pixels: Vec::new(),
            normal_width: 0,
            normal_height: 0,
            normal_pixels: Vec::new(),
            specular_width: 0,
            specular_height: 0,
            specular_pixels: Vec::new(),
            opacity_width: 0,
            opacity_height: 0,
            opacity_pixels: Vec::new(),
        }
    }
}

/// Per quad-node foliage chunk data.
/// Corresponds to `BTerrainFoliageQNChunk` in the original.
#[derive(Clone)]
pub struct FoliageQNChunk {
    /// Parent quad-node index (into terrain grid).
    pub qn_parent_index: u32,
    /// World-grid X coordinate from the parent XTD visual quad node.
    pub grid_x: i32,
    /// World-grid Z coordinate from the parent XTD visual quad node.
    pub grid_z: i32,
    /// Number of foliage sets used in this chunk.
    pub num_sets: u32,
    /// Indices into the foliage sets array.
    pub set_indices: Vec<i32>,
    /// Polygon count for each set (for `DrawIndexedPrimitive`).
    pub set_poly_counts: Vec<i32>,
    /// Packed 32-bit vertex IDs for each set's triangle strip.
    pub index_buffers: Vec<Vec<u32>>,
}

// ============================================================================
// Road Types
// ============================================================================

/// Decoded road data ready for rendering.
#[derive(Clone, Debug)]
pub struct RoadChunkData {
    /// Road texture name (e.g., "roads\\`road_01`").
    pub texture_name: String,
    /// All road vertices (position + UV), flattened from all QN chunks.
    pub positions: Vec<[f32; 3]>,
    /// UV coordinates for each vertex.
    pub uvs: Vec<[f32; 2]>,
}

// ============================================================================
// Raw XTD Types (for GPU tessellation)
// ============================================================================

/// Raw XTD vertex data for GPU tessellation (before decoding to world positions).
pub struct RawXtdData {
    /// Packed position data in the PC texture's native X-major storage order.
    ///
    /// Logical `(x, z)` is stored at `x * num_verts_per_axis + z`; the shader
    /// addresses that word with texture coordinate `(z, x)`.
    pub packed_positions: Vec<u32>,
    /// Packed normal data in the same native X-major storage order.
    pub packed_normals: Vec<u32>,
    /// Number of vertices per axis (e.g., 1024).
    pub num_verts_per_axis: u32,
    /// Atlas mid point for decoding.
    pub mid: [f32; 3],
    /// Atlas range for decoding.
    pub range: [f32; 3],
    /// Tile scale for world position.
    pub tile_scale: f32,
    /// World-space minimum bounds from the XTD header.
    pub world_min: [f32; 3],
    /// World-space maximum bounds from the XTD header.
    pub world_max: [f32; 3],
    /// XTD patch tessellation levels in texture row-major order (X changes fastest).
    ///
    /// The PC hull shader converts levels 0, 1, 2, and 3 to subdivision
    /// factors 16, 8, 4, and 2, then raises shared edges to the finer of the
    /// two adjacent patches.
    pub tessellation: Option<TerrainTessellationData>,
    /// Ambient occlusion data (R8 values, half resolution).
    pub ao_data: Option<AoTextureData>,
    /// Alpha/transparency data (R8 values, half resolution).
    pub alpha_data: Option<AlphaTextureData>,
}

/// Per-patch terrain tessellation metadata decoded from the XTD tess chunk.
#[derive(Clone)]
pub struct TerrainTessellationData {
    pub patches_x: u32,
    pub patches_z: u32,
    pub levels: Vec<u8>,
}

/// Half-resolution AO texture data as decoded from the game.
/// Dimensions: full width × half height (e.g., 1024×512 for 1024×1024 terrain).
#[derive(Clone)]
pub struct AoTextureData {
    pub values: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Half-resolution Alpha texture data as decoded from the game.
/// Dimensions: full width × half height (same as AO).
/// Used for terrain transparency (holes, cliff edges).
#[derive(Clone)]
pub struct AlphaTextureData {
    pub values: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Lighting texture data (RGBA8 decoded from BC1) at full terrain resolution.
#[derive(Clone)]
pub struct LightingTextureData {
    /// Row-major RGBA8 pixels.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
