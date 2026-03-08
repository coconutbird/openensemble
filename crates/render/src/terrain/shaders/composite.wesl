// Terrain texture compositing shader - aligned with original Halo Wars gpuTerrainComposite.fx
//
// Original approach:
// - Draws each layer separately with hardware alpha blending (SrcAlpha/OneMinusSrcAlpha)
// - Uses tex3D for texture array, tex3D for alpha array
// - UV scaling: Output.targetUVs = TexCoord0 * g_LayerData[indx].yz
// - Alpha lookup: tex3D(alphasSampler, float3(uv0, layer_group_z))[channel]
//
// Our approach (equivalent math, single pass):
// - Single fullscreen triangle per chunk
// - Manual mix() blending (mathematically identical to alpha blend)
// - 2D texture array instead of 3D
// - 2D alpha atlas with RGBA channels

struct CompositeParams {
    chunk_index: u32,      // Which chunk we're compositing (0-255)
    num_layers: u32,       // Number of active layers (1-8)
    chunk_offset: vec2<f32>, // UV offset in output atlas (unused, kept for compatibility)
    chunk_size: vec2<f32>,   // UV size in output atlas (unused, kept for compatibility)
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: CompositeParams;

// Terrain texture array (equivalent to original's tex3D targetSampler)
@group(0) @binding(1)
var t_terrain_array: texture_2d_array<f32>;

// Alpha atlas (equivalent to original's tex3D alphasSampler, but 2D with RGBA channels)
@group(0) @binding(2)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data (8 layer indices per chunk)
// Equivalent to g_LayerData[].x (layer index)
@group(0) @binding(3)
var<storage, read> chunk_layers: array<u32>;

// Texture UV scales (one vec2 per texture)
// Equivalent to g_LayerData[].yz (u_scale, v_scale)
@group(0) @binding(4)
var<storage, read> texture_scales: array<vec2<f32>>;

// Terrain sampler (LINEAR filtering, WRAP addressing - matches original)
@group(0) @binding(5)
var s_terrain: sampler;

// Alpha sampler (LINEAR filtering like original alphasSampler)
@group(0) @binding(6)
var s_alpha: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,  // 0-1 within chunk (equivalent to original's TexCoord0)
};

// Fullscreen triangle vertex shader
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);

    // Convert clip space to UV [0,1]
    let raw_uv = (positions[vertex_index] + 1.0) * 0.5;
    // Flip Y to match texture coordinates (clip Y=+1 is top, texture V=0 is top)
    out.uv = vec2<f32>(raw_uv.x, 1.0 - raw_uv.y);
    return out;
}

// Fragment shader - composites terrain layers
// Equivalent to original's CompsPixel_albedo_ARGB8 but does all layers in one pass
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let chunk_idx = params.chunk_index;

    // in.uv is equivalent to original's TexCoord0 (0-1 within chunk)
    let in_chunk_uv = in.uv;

    // Derive chunk grid position from index
    // chunk_idx = chunk_x * 16 + chunk_y (X-major, matching original Halo Wars engine)
    let chunk_y = chunk_idx % 16u;
    let chunk_x = chunk_idx / 16u;

    // Get layer texture indices for this chunk
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Sample alpha values from atlas
    // Original: tex3D(alphasSampler, float3(uv0, layer_group_z))[channel]
    // Ours: 2D atlas where each chunk's region stores RGBA alpha values
    let alpha_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv) / 16.0;
    let alphas = textureSample(t_alpha_atlas, s_alpha, alpha_uv);

    // Calculate texture UVs with per-texture scaling
    // Original: Output.targetUVs = TexCoord0 * g_LayerData[indx].yz
    // We use continuous UVs across terrain for seamless chunk boundaries
    let base_uv = vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv;

    // Get UV scales and compute tiled UVs
    let scale0 = texture_scales[layer0];
    let scale1 = texture_scales[layer1];
    let scale2 = texture_scales[layer2];
    let scale3 = texture_scales[layer3];

    let uv0 = base_uv * scale0;
    let uv1 = base_uv * scale1;
    let uv2 = base_uv * scale2;
    let uv3 = base_uv * scale3;

    // Sample and blend layers
    // Original uses hardware alpha blending with multiple draw calls
    // We use mix() which is mathematically equivalent:
    //   mix(a, b, t) = a * (1-t) + b * t
    //   alpha_blend: dst = src * src.a + dst * (1-src.a)
    // These produce identical results when applied sequentially

    // Layer 0: base layer (always 100% coverage, no alpha)
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    // Layer 1: blend using alphas.r (original indexes.y=0 -> channel R)
    if (layer1 > 0u && alphas.r > 0.0) {
        let c1 = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, c1, alphas.r);
    }

    // Layer 2: blend using alphas.g (original indexes.y=1 -> channel G)
    if (layer2 > 0u && alphas.g > 0.0) {
        let c2 = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, c2, alphas.g);
    }

    // Layer 3: blend using alphas.b (original indexes.y=2 -> channel B)
    if (layer3 > 0u && alphas.b > 0.0) {
        let c3 = textureSampleLevel(t_terrain_array, s_terrain, uv3, layer3, 0.0).rgb;
        color = mix(color, c3, alphas.b);
    }

    return vec4<f32>(color, 1.0);
}
