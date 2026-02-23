//! Pure data types for terrain loading.
//!
//! These types represent loaded terrain data without any GPU dependencies.
//! They can be used by both the viewer and the engine.

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
#[derive(Clone)]
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

/// A terrain decal texture loaded from ERA.
/// Decals have separate diffuse (_df) and opacity (_op) textures.
#[derive(Clone)]
pub struct DecalTexture {
    /// Decal name (e.g., "road_01").
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
/// This corresponds to XTT's ActiveDecalInstance.
#[derive(Clone, Debug)]
pub struct DecalInstance {
    /// Index into the decal_textures array.
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
    /// Indices into decal_instances for this chunk's decals.
    pub decal_layer_ids: Vec<i32>,
    /// Alpha maps for decal layers (if any).
    /// Each is 64x64 = 4096 bytes.
    pub alpha_maps: Vec<Vec<u8>>,
}

// ============================================================================
// Foliage Types
// ============================================================================

/// A single foliage blade vertex (position + normal + UV).
/// From the original XML format in TerrainFoliage.cpp.
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
/// Corresponds to BTerrainFoliageSet in the original.
#[derive(Clone)]
pub struct FoliageSet {
    /// Set name (e.g., "foliage\\bg_grass_02").
    pub name: String,
    /// Backside shadow scalar for lighting.
    pub backside_shadow_scalar: f32,
    /// Number of blade types in this set.
    pub num_blade_types: u32,
    /// Number of vertices per blade (typically 10).
    pub num_verts_per_blade: u32,
    /// Blade vertex data stored as line texture format:
    /// positions.xyz + uv.x stored in positions texture
    /// normals.xyz + uv.y stored in normals texture
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
/// Corresponds to BTerrainFoliageQNChunk in the original.
#[derive(Clone)]
pub struct FoliageQNChunk {
    /// Parent quad-node index (into terrain grid).
    pub qn_parent_index: u32,
    /// Number of foliage sets used in this chunk.
    pub num_sets: u32,
    /// Indices into the foliage sets array.
    pub set_indices: Vec<i32>,
    /// Polygon count for each set (for DrawIndexedPrimitive).
    pub set_poly_counts: Vec<i32>,
    /// Raw index buffer data for each set.
    /// These are 32-bit indices used with triangle strips.
    pub index_buffers: Vec<Vec<u8>>,
}
