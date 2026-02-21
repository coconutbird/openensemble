//! GPU uniform structures for terrain shaders.
//!
//! These structures must match the WGSL shader definitions exactly.
//! Be careful with alignment - WGSL has strict rules:
//! - vec2<f32> = 8 byte alignment
//! - vec3<f32> = 16 byte alignment  
//! - vec4<f32> = 16 byte alignment
//! - Struct total size must be multiple of largest member alignment

/// Terrain shader parameters.
///
/// Controls rendering options like debug mode and texture scaling.
/// Must match the TerrainParams struct in WGSL shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TerrainParams {
    /// Terrain size in world units [width, depth].
    pub terrain_size: [f32; 2],
    /// Number of chunks in each direction [x, z].
    pub chunk_count: [f32; 2],
    /// Texture tiling scale.
    pub texture_tile_scale: f32,
    /// Debug visualization mode (0=normal, 1=alpha, 2=UV, etc).
    pub debug_mode: f32,
    /// Normal map intensity (1.0 = default).
    pub bump_power: f32,
    /// Padding for 16-byte alignment.
    pub _padding: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            terrain_size: [1024.0, 1024.0],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: 1.0,
            debug_mode: 0.0,
            bump_power: 1.0,
            _padding: 0.0,
        }
    }
}

// SAFETY: TerrainParams is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for TerrainParams {}
unsafe impl bytemuck::Zeroable for TerrainParams {}

/// GPU tessellation shader parameters.
///
/// Contains data needed to decode packed positions/normals in the vertex shader.
/// Must match the TessParams struct in WGSL shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GpuTessParams {
    /// Position decoding mid point [x, y, z, pad].
    pub mid: [f32; 4],
    /// Position decoding range [x, y, z, pad].
    pub range: [f32; 4],
    /// Terrain info [num_verts_per_axis, tile_scale, num_patches_x, num_patches_z].
    pub terrain_info: [f32; 4],
    /// World bounds minimum [x, y, z, pad].
    pub world_min: [f32; 4],
    /// World bounds maximum [x, y, z, pad].
    pub world_max: [f32; 4],
}

impl Default for GpuTessParams {
    fn default() -> Self {
        Self {
            mid: [0.0; 4],
            range: [1.0, 1.0, 1.0, 0.0],
            terrain_info: [1025.0, 1.0, 64.0, 64.0],
            world_min: [0.0; 4],
            world_max: [1024.0, 100.0, 1024.0, 0.0],
        }
    }
}

// SAFETY: GpuTessParams is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for GpuTessParams {}
unsafe impl bytemuck::Zeroable for GpuTessParams {}

/// Camera uniform for shaders.
///
/// Contains the view-projection matrix.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CameraUniform {
    /// Combined view-projection matrix.
    pub view_proj: [[f32; 4]; 4],
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

// SAFETY: CameraUniform is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for CameraUniform {}
unsafe impl bytemuck::Zeroable for CameraUniform {}
