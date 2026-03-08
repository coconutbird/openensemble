// Terrain shader for CPU-tessellated mesh
// Supports texture splatting, alpha blending, and multiple debug modes

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Terrain texture array (up to 8 textures)
@group(1) @binding(0)
var t_terrain_array: texture_2d_array<f32>;
@group(1) @binding(1)
var s_terrain: sampler;

// Alpha atlas for all chunks (16x16 chunks, 64x64 per chunk = 1024x1024)
// Stores up to 4 alpha channels per texture (RGBA)
@group(1) @binding(2)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data (256 chunks * 8 layer IDs = 2048 u32s)
@group(1) @binding(3)
var<storage, read> chunk_layers: array<u32>;

// Terrain dimensions (32 bytes total to match Rust struct)
struct TerrainParams {
    terrain_size: vec2<f32>,      // offset 0: World size of terrain (width, depth)
    chunk_count: vec2<f32>,       // offset 8: Number of chunks (16, 16)
    texture_tile_scale: f32,      // offset 16: How many times textures tile
    debug_mode: f32,              // offset 20: Debug visualization mode
    bump_power: f32,              // offset 24: Normal map XY scale (gBumpPower)
    _pad3: f32,                   // offset 28: padding
};
@group(1) @binding(4)
var<uniform> params: TerrainParams;

// Separate sampler for alpha atlas (Nearest filtering to avoid chunk boundary bleeding)
@group(1) @binding(5)
var s_alpha: sampler;

// XTT albedo (original pre-composited from game export)
@group(1) @binding(6)
var t_xtt_albedo: texture_2d<f32>;

// Per-texture UV scales (8 textures * vec2<f32> = 16 floats)
// These control how many times each texture tiles across the terrain
@group(1) @binding(7)
var<storage, read> texture_scales: array<vec2<f32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.world_pos = in.position;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let normal = normalize(in.normal);
    let diff = max(dot(normal, light_dir), 0.0);
    let ambient = 0.4;
    let lighting = ambient + diff * 0.6;

    // Determine which chunk this pixel belongs to (0-15 in each axis)
    // UV convention: U = world Z, V = world X (matching game's gPos.zx)
    let chunk_uv = in.uv * params.chunk_count;
    let grid_z = u32(clamp(floor(chunk_uv.x), 0.0, params.chunk_count.x - 1.0));  // UV.x = Z
    let grid_x = u32(clamp(floor(chunk_uv.y), 0.0, params.chunk_count.y - 1.0));  // UV.y = X
    // X-major index (game convention): gridX * numZChunks + gridZ
    let chunk_idx = grid_x * u32(params.chunk_count.x) + grid_z;

    // UV within the chunk (0-1) for alpha sampling
    let in_chunk_uv = fract(chunk_uv);

    // Calculate alpha atlas UV - each chunk is 64x64 in a 1024x1024 atlas
    // chunk_uv is (Z, X). in_chunk_uv = (frac_z, frac_x) maps directly to alpha
    // texture (col, row) — no transposition needed. Atlas position = (grid_z, grid_x).
    let alpha_atlas_uv = (vec2<f32>(f32(grid_z), f32(grid_x)) + in_chunk_uv) / params.chunk_count;

    // Sample alpha values for this chunk (RGBA = 4 alpha channels for layers 1-4)
    // Use s_alpha (Nearest filtering) to avoid bleeding at chunk boundaries
    let alphas = textureSample(t_alpha_atlas, s_alpha, alpha_atlas_uv);

    // Get layer indices for this chunk (8 layers max per chunk, stored as u32s)
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Calculate per-layer tiled UVs using each texture's scale factors
    // in.uv is (Z, X) from decoder — same convention as the compositor.
    // u_scale applies to Z (UV.x), v_scale applies to X (UV.y).
    let tiling_uv = in.uv * params.chunk_count;
    let uv0 = tiling_uv * texture_scales[layer0];
    let uv1 = tiling_uv * texture_scales[layer1];
    let uv2 = tiling_uv * texture_scales[layer2];
    let uv3 = tiling_uv * texture_scales[layer3];

    // DEBUG MODE: 0=splatting, 1=alpha values, 2=in-chunk UVs, 3=raw atlas, 4=terrain UVs, 5=composited
    // Press 0-5 keys to switch modes
    let debug_mode = i32(params.debug_mode);

    if debug_mode == 1 {
        // Visualize alpha values as color (RED shows layer 1 alpha)
        return vec4<f32>(alphas.r, alphas.g, alphas.b, 1.0);
    } else if debug_mode == 2 {
        // Visualize in-chunk UVs (should show smooth gradient within each chunk)
        return vec4<f32>(in_chunk_uv.x, in_chunk_uv.y, 0.0, 1.0);
    } else if debug_mode == 3 {
        // Sample raw atlas using terrain UV directly (shows atlas as-is)
        let raw_alpha = textureSample(t_alpha_atlas, s_alpha, in.uv);
        return vec4<f32>(raw_alpha.r, raw_alpha.g, raw_alpha.b, 1.0);
    } else if debug_mode == 4 {
        // Show raw terrain mesh UVs (should be smooth 0-1 gradient across entire terrain)
        return vec4<f32>(in.uv.x, in.uv.y, 0.0, 1.0);
    }
    // More debug modes continued in fragment shader body...

    // Mode 0: Runtime texture splatting
    var color = textureSample(t_terrain_array, s_terrain, uv0, layer0).rgb;

    // Blend layers 1-3 using alpha values
    if layer1 > 0u && alphas.r > 0.0 {
        let layer1_color = textureSample(t_terrain_array, s_terrain, uv1, layer1).rgb;
        color = mix(color, layer1_color, alphas.r);
    }
    if layer2 > 0u && alphas.g > 0.0 {
        let layer2_color = textureSample(t_terrain_array, s_terrain, uv2, layer2).rgb;
        color = mix(color, layer2_color, alphas.g);
    }
    if layer3 > 0u && alphas.b > 0.0 {
        let layer3_color = textureSample(t_terrain_array, s_terrain, uv3, layer3).rgb;
        color = mix(color, layer3_color, alphas.b);
    }

    return vec4<f32>(color * lighting, 1.0);
}
