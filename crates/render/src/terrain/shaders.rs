//! WGSL shader source code for terrain rendering.
//!
//! Contains shader code for both CPU-tessellated and GPU-tessellated terrain rendering,
//! as well as the compositing shader for pre-baking terrain textures.
//!
//! Shaders using shared code (lighting, fog, TBN) are authored as `.wesl` files
//! with `import` statements and compiled to WGSL at build time via the `wesl` crate.
//! Plain WGSL shaders are loaded directly with `include_str!`.

use wesl::include_wesl;

/// Basic terrain shader for CPU-tessellated mesh (compiled from WESL).
///
/// Features:
/// - Texture array splatting with up to 8 textures
/// - Alpha blending between terrain layers
/// - Normal mapping with BC5/DXN support
/// - Multiple debug visualization modes (0-17)
/// - Per-texture UV scaling
pub const TERRAIN_SHADER: &str = include_wesl!("terrain");

/// GPU tessellation shader using instanced patches (compiled from WESL).
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
/// - Shared lighting/fog/TBN via WESL imports
pub const GPU_TESS_SHADER: &str = include_wesl!("gpu_tess");

/// GPU compositing shader for pre-baking terrain textures (compiled from WESL).
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
pub const COMPOSITE_SHADER: &str = include_wesl!("composite");

/// Foliage rendering shader based on terrainFoliage.fx (compiled from WESL).
///
/// Features:
/// - Blade geometry fetched from position/normal textures
/// - Deterministic random rotation and height scaling per blade
/// - Terrain height sampling for blade base positioning
/// - Distance-based alpha fade
/// - Two-sided lighting (normal flipped toward camera)
/// - SH ambient fill lighting
/// - Fog (radial + planar)
/// - Shared lighting/fog via WESL imports
pub const FOLIAGE_SHADER: &str = include_wesl!("foliage");

/// Heightfield/decal patch shader based on terrainHeightField.fx (compiled from WESL).
///
/// Features:
/// - Instanced quad patches with forward/right vector interpolation
/// - Terrain-conforming via heightfield depth texture
/// - Full lit pipeline: directional + SH ambient + specular
/// - Normal mapping (DXN/BC5)
/// - Fog (radial + planar)
/// - Opacity-based alpha blending
/// - Shared lighting/fog/TBN via WESL imports
pub const HEIGHTFIELD_SHADER: &str = include_wesl!("terrain_heightfield");

/// Road shader based on terrainRoads.fx (compiled from WESL).
///
/// Features:
/// - Pre-tessellated road geometry conforming to terrain
/// - Terrain position/normal texture sampling for height + TBN
/// - Full lit pipeline: directional + SH ambient + specular
/// - Normal mapping (DXN/BC5)
/// - Fog (radial + planar)
/// - Shared lighting/fog/TBN via WESL imports
pub const ROADS_SHADER: &str = include_wesl!("terrain_roads");
