//! WGSL shader source code for terrain rendering.
//!
//! Contains shader code for both CPU-tessellated and GPU-tessellated terrain rendering,
//! as well as the compositing shader for pre-baking terrain textures.

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

/// GPU compositing shader for pre-baking terrain textures.
///
/// This shader renders terrain splat layers to a render target, creating
/// a unique composited texture per chunk. This is the approach used by
/// retail Halo Wars DE for high-quality terrain rendering.
///
/// Features:
/// - Fullscreen triangle rendering for chunk regions
/// - Texture array sampling with per-texture UV scaling
/// - Alpha-blended layer compositing
/// - Outputs to 8K atlas (16x16 chunks, 512x512 each)
pub const COMPOSITE_SHADER: &str = include_str!("shaders/composite.wgsl");

/// Foliage rendering shader based on terrainFoliage.fx.
///
/// Features:
/// - Blade geometry fetched from position/normal textures
/// - Deterministic random rotation and height scaling per blade
/// - Terrain height sampling for blade base positioning
/// - Distance-based alpha fade
/// - Two-sided lighting (normal flipped toward camera)
pub const FOLIAGE_SHADER: &str = include_str!("shaders/foliage.wgsl");

/// Heightfield/decal patch shader based on terrainHeightField.fx.
///
/// Features:
/// - Instanced quad patches with forward/right vector interpolation
/// - Terrain-conforming via heightfield depth texture
/// - Full lit pipeline: directional + SH ambient + specular
/// - Normal mapping (DXN/BC5)
/// - Fog (radial + planar)
/// - Opacity-based alpha blending
pub const HEIGHTFIELD_SHADER: &str = include_str!("shaders/terrain_heightfield.wgsl");

/// Road shader based on terrainRoads.fx.
///
/// Features:
/// - Pre-tessellated road geometry conforming to terrain
/// - Terrain position/normal texture sampling for height + TBN
/// - Full lit pipeline: directional + SH ambient + specular
/// - Normal mapping (DXN/BC5)
/// - Fog (radial + planar)
pub const ROADS_SHADER: &str = include_str!("shaders/terrain_roads.wgsl");
