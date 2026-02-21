//! WGSL shader source code for terrain rendering.
//!
//! Contains shader code for both CPU-tessellated and GPU-tessellated terrain rendering.

/// Basic terrain shader for CPU-tessellated mesh.
///
/// Features:
/// - Texture array splatting with up to 8 textures
/// - Alpha blending between terrain layers
/// - Normal mapping with BC5/DXN support
/// - Multiple debug visualization modes (0-17)
/// - Per-texture UV scaling
pub const TERRAIN_SHADER: &str = include_str!("shaders/terrain.wgsl");

/// GPU tessellation shader using instanced patches.
///
/// Each instance is a terrain patch (16x16 vertices), and vertices sample
/// position/normal from packed textures. This allows high-resolution terrain
/// rendering without sending all vertices to the GPU.
///
/// Features:
/// - Packed position decoding (R10G10B10A2)
/// - Packed normal decoding
/// - AO texture sampling with proper untiling
/// - Alpha (transparency/holes) support
/// - Normal map splatting
/// - Multiple debug modes
pub const GPU_TESS_SHADER: &str = include_str!("shaders/gpu_tess.wgsl");
