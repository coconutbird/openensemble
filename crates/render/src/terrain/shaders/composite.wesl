// Terrain texture compositing shader.
//
// Renders terrain splat layers to an 8K×8K atlas (16×16 chunks, 512×512 each).
// Uses the EXACT same splatting logic as gpu_tess.wesl mode 9 (runtime splatting),
// which has been verified correct against the original game's XTT albedo output.
//
// Each chunk is rendered via a fullscreen triangle with viewport set to the
// chunk's region in the atlas. The fragment shader computes chunk_x/chunk_z
// from the chunk_index, then does identical UV, alpha, and blending math.

struct CompositeParams {
    chunk_index: u32,      // Which chunk we're compositing (0-255)
    num_layers: u32,       // Number of active layers for this chunk
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: CompositeParams;

@group(0) @binding(1)
var t_terrain_array: texture_2d_array<f32>;

@group(0) @binding(2)
var t_alpha_atlas: texture_2d_array<f32>;

@group(0) @binding(3)
var<storage, read> chunk_layers: array<u32>;

@group(0) @binding(4)
var<storage, read> texture_scales: array<vec2<f32>>;

@group(0) @binding(5)
var s_terrain: sampler;

@group(0) @binding(6)
var s_alpha: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    let raw_uv = (positions[vertex_index] + 1.0) * 0.5;
    // Flip Y: clip Y=+1 is top, texture V=0 is top
    out.uv = vec2<f32>(raw_uv.x, 1.0 - raw_uv.y);
    return out;
}

// Fragment shader — identical splatting logic to gpu_tess.wesl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let chunk_idx = params.chunk_index;

    // in.uv is 0-1 within this chunk (the viewport is set per-chunk)
    let in_chunk_uv = in.uv;

    // Derive grid position from chunk index
    // Same formula as gpu_tess: chunk_idx = grid_x * 16 + grid_z
    let grid_z = chunk_idx % 16u;
    let grid_x = chunk_idx / 16u;

    // Get layer texture indices for this chunk
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Sample alpha — same gutter inset as gpu_tess
    let gutter = 1.0 / 64.0;
    let guttered_uv = mix(vec2<f32>(gutter), vec2<f32>(1.0 - gutter), in_chunk_uv);
    let alphas = textureSampleLevel(t_alpha_atlas, s_alpha, guttered_uv, chunk_idx, 0.0);

    // Continuous base UV for seamless chunk boundaries — same as gpu_tess
    let base_uv = vec2<f32>(f32(grid_x), f32(grid_z)) + in_chunk_uv;

    // Tiled UVs with per-texture scaling
    let uv0 = base_uv * texture_scales[layer0];
    let uv1 = base_uv * texture_scales[layer1];
    let uv2 = base_uv * texture_scales[layer2];
    let uv3 = base_uv * texture_scales[layer3];

    // Layer 0: base layer (always 100%)
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    // Layer 1: blend using alphas.r
    if (layer1 > 0u && alphas.r > 0.0) {
        let c1 = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, c1, alphas.r);
    }

    // Layer 2: blend using alphas.g
    if (layer2 > 0u && alphas.g > 0.0) {
        let c2 = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, c2, alphas.g);
    }

    // Layer 3: blend using alphas.b
    if (layer3 > 0u && alphas.b > 0.0) {
        let c3 = textureSampleLevel(t_terrain_array, s_terrain, uv3, layer3, 0.0).rgb;
        color = mix(color, c3, alphas.b);
    }

    return vec4<f32>(color, 1.0);
}
