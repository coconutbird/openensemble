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

/// GPU terrain rendering shader using instanced patches (compiled from WESL).
///
/// Port of the original gpuTerrainXbox.fx. Each instance is a terrain patch
/// (16x16 vertices), and vertices sample position/normal from packed textures.
///
/// Features:
/// - Packed position decoding (R10G10B10A2)
/// - Packed normal decoding
/// - AO texture sampling with proper untiling
/// - Alpha (transparency/holes) support
/// - Runtime texture splatting or GPU-composited atlas sampling
/// - Normal map splatting
/// - Multiple debug modes
/// - Shared lighting/fog/TBN via WESL imports
pub const GPU_TESS_SHADER: &str = include_wesl!("terrain_gpu");

/// GPU compositing shader for pre-baking terrain textures (compiled from WESL).
///
/// Port of the original gpuTerrainComposite.fx. Renders terrain splat layers
/// to an 8K×8K atlas (16×16 chunks, 512×512 each) using the same splatting
/// logic as the runtime shader.
///
/// Original game uses multi-pass with hardware alpha blending (one layer per
/// pass). We use single-pass with manual `mix()` which is mathematically
/// equivalent.
pub const COMPOSITE_SHADER: &str = include_wesl!("terrain_composite");

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

/// Shadow depth shader for rendering terrain from the light's perspective.
///
/// Features:
/// - Minimal vertex shader reusing `gpu_tess` vertex format
/// - Packed position decoding (R10G10B10A2)
/// - Outputs depth + depth² for VSM filtering
/// - No lighting, fog, or texturing (depth only)
pub const SHADOW_DEPTH_SHADER: &str = include_wesl!("shadow_depth");
