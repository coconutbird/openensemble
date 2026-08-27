//! GPU uniform structures for terrain shaders.
//!
//! These structures must match the WGSL shader definitions exactly.
//! Be careful with alignment - WGSL has strict rules:
//! - `vec2<f32>` = 8 byte alignment
//! - `vec3<f32>` = 16 byte alignment
//! - `vec4<f32>` = 16 byte alignment
//! - Struct total size must be multiple of largest member alignment

/// Terrain shader parameters.
///
/// Controls rendering options like debug mode and texture scaling.
/// Must match the `TerrainParams` struct in WGSL shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TerrainParams {
    /// Terrain size in world units [width, depth].
    pub terrain_size: [f32; 2],
    /// Number of chunks in each direction [x, z] (X→U, Z→V convention).
    pub chunk_count: [f32; 2],
    /// Texture tiling scale.
    pub texture_tile_scale: f32,
    /// Debug visualization mode (0=normal, 1=alpha, 2=UV, etc).
    pub debug_mode: f32,
    /// Normal map intensity (1.0 = default).
    pub bump_power: f32,
    /// Padding for 16-byte alignment.
    pub padding: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            terrain_size: [1024.0, 1024.0],
            chunk_count: [16.0, 16.0],
            texture_tile_scale: 1.0,
            debug_mode: 0.0,
            bump_power: 1.0,
            padding: 0.0,
        }
    }
}

// SAFETY: TerrainParams is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for TerrainParams {}
unsafe impl bytemuck::Zeroable for TerrainParams {}

/// GPU tessellation shader parameters.
///
/// Contains data needed to decode packed positions/normals in the vertex shader.
/// Must match the `TessParams` struct in WGSL shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GpuTessParams {
    /// Position decoding mid point [x, y, z, pad].
    pub mid: [f32; 4],
    /// Position decoding range [x, y, z, pad].
    pub range: [f32; 4],
    /// Terrain info [`num_verts_per_axis`, `tile_scale`, `num_patches_x`, `num_patches_z`].
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

/// Lighting parameters for terrain shaders.
///
/// Contains directional light, SH fill lighting, fog, AO, shadow,
/// blackmap, and local light parameters.
/// Matches the original Halo Wars cbShared lighting fields.
/// Must match the `LightingParams` struct in WGSL shaders.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct LightingParams {
    /// Direction TO light in world space [x, y, z, enabled].
    pub dir_light_vec: [f32; 4],
    /// Directional light color [r, g, b, `shadow_darkness`].
    pub dir_light_color: [f32; 4],
    /// Camera world position [x, y, z, pad].
    pub world_camera_pos: [f32; 4],
    /// SH fill lighting coefficients (linear R).
    pub sh_fill_ar: [f32; 4],
    /// SH fill lighting coefficients (linear G).
    pub sh_fill_ag: [f32; 4],
    /// SH fill lighting coefficients (linear B).
    pub sh_fill_ab: [f32; 4],
    /// SH fill lighting coefficients (quadratic R).
    pub sh_fill_br: [f32; 4],
    /// SH fill lighting coefficients (quadratic G).
    pub sh_fill_bg: [f32; 4],
    /// SH fill lighting coefficients (quadratic B).
    pub sh_fill_bb: [f32; 4],
    /// SH fill lighting coefficients (final quadratic).
    pub sh_fill_c: [f32; 4],
    /// Fog color [r, g, b, pad].
    pub fog_color: [f32; 4],
    /// Fog params [density2, start2, pad, pad].
    pub fog_params: [f32; 4],
    /// Planar fog color [r, g, b, pad].
    pub planar_fog_color: [f32; 4],
    /// Planar fog params [enabled, start, density2, pad].
    pub planar_fog_params: [f32; 4],
    /// AO params [`ao_diffuse_intensity`, pad, pad, pad].
    pub ao_params: [f32; 4],

    // --- Shadow params ---
    /// Shadow view-projection matrix column 0.
    pub shadow_vp_col0: [f32; 4],
    /// Shadow view-projection matrix column 1.
    pub shadow_vp_col1: [f32; 4],
    /// Shadow view-projection matrix column 2.
    pub shadow_vp_col2: [f32; 4],
    /// Shadow view-projection matrix column 3.
    pub shadow_vp_col3: [f32; 4],
    /// Shadow params [`csm_scale`, `num_passes`, enabled, pad].
    pub shadow_params: [f32; 4],

    // --- Blackmap params ---
    /// Blackmap params0 [`bg_r`, `bg_g`, `bg_b`, `fog_scalar`].
    pub blackmap_params0: [f32; 4],
    /// Blackmap params1 [`unexplored_scalar`, `bounds_lo_x`, `bounds_lo_z`, enabled].
    pub blackmap_params1: [f32; 4],
    /// Blackmap params2 [pad, `bounds_hi_x`, `bounds_hi_z`, `bounds_falloff`].
    pub blackmap_params2: [f32; 4],

    // --- Local light params ---
    /// Local light params [`num_lights`, `spec_power`, pad, pad].
    pub local_light_params: [f32; 4],

    // --- Bump fadeout params (HWDE cb4[35]) ---
    /// Fadeout params [`fadeout_min`, `fadeout_max`, `fadeout_bias`, pad].
    pub fadeout_params: [f32; 4],

    // --- Blackmap UV scales (HWDE cb4[33-34]) ---
    /// Blackmap UV scales [`scale_x`, `scale_z`, pad, pad].
    pub blackmap_uv_scales: [f32; 4],
}

impl Default for LightingParams {
    fn default() -> Self {
        // Default: warm directional light from above-right, gentle SH ambient
        let light_dir = [0.4, 0.8, 0.3, 1.0]; // normalized later in shader
        Self {
            dir_light_vec: light_dir,
            dir_light_color: [1.0, 0.95, 0.85, 0.3], // warm white, shadow_darkness=0.3
            world_camera_pos: [512.0, 200.0, 512.0, 0.0],
            // Default SH: simple hemisphere (sky blue above, ground brown below)
            sh_fill_ar: [0.15, 0.0, 0.0, 0.3],
            sh_fill_ag: [0.15, 0.0, 0.0, 0.35],
            sh_fill_ab: [0.15, 0.0, 0.0, 0.45],
            sh_fill_br: [0.0; 4],
            sh_fill_bg: [0.0; 4],
            sh_fill_bb: [0.0; 4],
            sh_fill_c: [0.0; 4],
            fog_color: [0.7, 0.8, 0.9, 0.0],
            fog_params: [0.0, 10000.0, 0.0, 0.0], // very distant fog (effectively disabled)
            planar_fog_color: [0.7, 0.8, 0.9, 0.0],
            planar_fog_params: [0.0, 0.0, 0.0, 0.0], // disabled
            ao_params: [0.8, 0.0, 0.0, 0.0],         // ao_diffuse_intensity = 0.8

            // Shadow: identity VP matrix, disabled by default
            shadow_vp_col0: [1.0, 0.0, 0.0, 0.0],
            shadow_vp_col1: [0.0, 1.0, 0.0, 0.0],
            shadow_vp_col2: [0.0, 0.0, 1.0, 0.0],
            shadow_vp_col3: [0.0, 0.0, 0.0, 1.0],
            shadow_params: [1.0, 1.0, 0.0, 0.0], // csm_scale=1, num_passes=1, enabled=0

            // Blackmap: disabled by default
            blackmap_params0: [0.0, 0.0, 0.0, 0.5], // bg=black, fog_scalar=0.5
            blackmap_params1: [0.2, 0.0, 0.0, 0.0], // unexplored=0.2, bounds_lo=(0,0), disabled
            blackmap_params2: [0.0, 1024.0, 1024.0, 0.01], // bounds_hi=(1024,1024), falloff=0.01

            // Local lights: none by default
            local_light_params: [0.0, 16.0, 0.0, 0.0], // 0 lights, spec_power=16

            // Bump fadeout: reasonable defaults (fade from 200 to 500 units)
            fadeout_params: [200.0, 500.0, 50.0, 0.0],

            // Blackmap UV scales: default 1/terrain_extent
            blackmap_uv_scales: [1.0 / 1024.0, 1.0 / 1024.0, 0.0, 0.0],
        }
    }
}

// SAFETY: LightingParams is repr(C) with all f32 fields
unsafe impl bytemuck::Pod for LightingParams {}
unsafe impl bytemuck::Zeroable for LightingParams {}
