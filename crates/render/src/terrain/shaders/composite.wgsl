// Terrain texture compositing shader.
//
// Renders splat layers to a render target for one chunk.
// Uses a fullscreen triangle to fill the chunk's region in the atlas.
//
// Input: chunk index, layer IDs, alpha maps, tiled terrain textures
// Output: composited RGBA to render target

struct CompositeParams {
    chunk_index: u32,      // Which chunk we're compositing (0-255)
    num_layers: u32,       // Number of active layers (1-8)
    chunk_offset: vec2<f32>, // UV offset in output atlas
    chunk_size: vec2<f32>,   // UV size in output atlas
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: CompositeParams;

// Terrain texture array (same as main shader)
@group(0) @binding(1)
var t_terrain_array: texture_2d_array<f32>;

// Alpha atlas (packed alpha values for blending)
@group(0) @binding(2)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data (8 layer indices per chunk)
@group(0) @binding(3)
var<storage, read> chunk_layers: array<u32>;

// Texture UV scales (one vec2 per texture)
@group(0) @binding(4)
var<storage, read> texture_scales: array<vec2<f32>>;

@group(0) @binding(5)
var s_terrain: sampler;

// Vertex shader output
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,  // 0-1 within chunk
};

// Fullscreen triangle vertex shader
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle vertices (covers -1 to 1 clip space)
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    // Map from clip space to 0-1 UV
    out.uv = (positions[vertex_index] + 1.0) * 0.5;
    return out;
}

// Fragment shader - composites terrain layers
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let chunk_idx = params.chunk_index;
    let in_chunk_uv = in.uv;
    
    // Get layer indices (8 layers per chunk)
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];
    
    // Calculate chunk position in grid
    let chunk_x = chunk_idx % 16u;
    let chunk_z = chunk_idx / 16u;
    
    // Sample alpha values from atlas
    // Each chunk has a 64x64 region in the 1024x1024 alpha atlas
    let alpha_uv = (vec2<f32>(f32(chunk_x), f32(chunk_z)) + in_chunk_uv) / 16.0;
    let alphas = textureSample(t_alpha_atlas, s_terrain, alpha_uv);
    
    // Calculate world UV for tiled texture sampling
    // This maps the chunk UV to terrain-space coordinates
    let base_uv = vec2<f32>(f32(chunk_x), f32(chunk_z)) + in_chunk_uv;
    
    // Apply per-texture UV scaling
    let scale0 = texture_scales[layer0];
    let scale1 = texture_scales[layer1];
    let scale2 = texture_scales[layer2];
    let scale3 = texture_scales[layer3];
    
    let uv0 = base_uv * scale0;
    let uv1 = base_uv * scale1;
    let uv2 = base_uv * scale2;
    let uv3 = base_uv * scale3;
    
    // Sample and blend layers
    // Layer 0 is always the base (100% coverage)
    var color = textureSample(t_terrain_array, s_terrain, uv0, layer0).rgb;
    
    // Blend layers 1-3 using alpha values
    // Alpha atlas stores: R = layer1 alpha, G = layer2 alpha, B = layer3 alpha
    if (layer1 > 0u && alphas.r > 0.0) {
        let c1 = textureSample(t_terrain_array, s_terrain, uv1, layer1).rgb;
        color = mix(color, c1, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let c2 = textureSample(t_terrain_array, s_terrain, uv2, layer2).rgb;
        color = mix(color, c2, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let c3 = textureSample(t_terrain_array, s_terrain, uv3, layer3).rgb;
        color = mix(color, c3, alphas.b);
    }
    
    return vec4<f32>(color, 1.0);
}

