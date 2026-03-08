// Terrain texture compositing shader — port of gpuTerrainComposite.fx
//
// Original game approach:
//   - Multi-pass: one draw call per splat layer, hardware alpha blending
//   - Vertex shader: computes layer index from vertex_id, scales UVs per layer
//   - Pixel shader: samples 3D texture array + 3D alpha texture, outputs (color, alpha)
//   - Hardware blending: SrcAlpha / InvSrcAlpha composites layers together
//
// Our approach (mathematically equivalent):
//   - Single-pass: fullscreen triangle per chunk, all layers blended in fragment shader
//   - Same UV math, same alpha sampling, same blend order
//   - Viewport + scissor rect isolates each chunk's 512×512 region in the 8K atlas
//
// The main terrain shader (terrain_gpu.wesl) samples this atlas at:
//   sample_uv = world_pos.xz / terrain_extent
// which maps to atlas position:
//   atlas_x = sample_uv.x * atlas_width  = (grid_x + fract) * chunk_size
//   atlas_y = sample_uv.y * atlas_height = (grid_z + fract) * chunk_size
//
// So we place chunk at viewport (grid_x * chunk_size, grid_z * chunk_size).

struct CompositeParams {
    chunk_index: u32,      // Which chunk (0-255 for 16×16 grid)
    num_layers: u32,       // Number of active splat layers
    debug_mode: u32,       // 0=normal, 1=UV, 2=chunk ID, 3=alpha, 4=layer0
    _padding: f32,
};

@group(0) @binding(0) var<uniform> params: CompositeParams;
@group(0) @binding(1) var t_terrain_array: texture_2d_array<f32>;
@group(0) @binding(2) var t_alpha_atlas: texture_2d_array<f32>;
@group(0) @binding(3) var<storage, read> chunk_layers: array<u32>;
@group(0) @binding(4) var<storage, read> texture_scales: array<vec2<f32>>;
@group(0) @binding(5) var s_terrain: sampler;
@group(0) @binding(6) var s_alpha: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle — covers the viewport set by compositing.rs
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    // Standard fullscreen triangle vertices
    let x = f32(i32(vid & 1u) * 4 - 1);
    let y = f32(i32(vid >> 1u) * 4 - 1);

    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: (0,0) at top-left of chunk, (1,1) at bottom-right
    // In wgpu clip space: x=-1 is left, y=-1 is bottom, y=+1 is top
    // Texture V=0 is top, so flip Y
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let chunk_idx = params.chunk_index;

    // in.uv is 0→1 within this chunk's viewport region
    let uv = in.uv;

    // Decompose chunk index → grid position
    // Matches terrain_gpu.wesl: chunk_idx = grid_x * 16 + grid_z
    let grid_x = chunk_idx / 16u;
    let grid_z = chunk_idx % 16u;

    // --- Layer indices from storage buffer ---
    let base = chunk_idx * 8u;
    let layer0 = chunk_layers[base + 0u];
    let layer1 = chunk_layers[base + 1u];
    let layer2 = chunk_layers[base + 2u];
    let layer3 = chunk_layers[base + 3u];

    // --- Alpha sampling ---
    // Original: tex3D(alphasSampler, float3(uv, slice))[channel]
    // Gutter inset: alpha textures are 64×64, inset by half-pixel to avoid edge artifacts
    let gutter = 1.0 / 64.0;
    let alpha_uv = mix(vec2<f32>(gutter), vec2<f32>(1.0 - gutter), uv);
    let alphas = textureSampleLevel(t_alpha_atlas, s_alpha, alpha_uv, chunk_idx, 0.0);

    // --- Tiled texture UVs ---
    // Original: targetUVs = TexCoord0 * g_LayerData[indx].yz
    // TexCoord0 is the in-chunk UV (0→1), but we use continuous UVs across chunks
    // for seamless boundaries: base_uv = (grid_x + uv.x, grid_z + uv.y)
    // This matches terrain_gpu.wesl: base_uv = sample_uv * chunk_count
    let base_uv = vec2<f32>(f32(grid_x), f32(grid_z)) + uv;

    let uv0 = base_uv * texture_scales[layer0];
    let uv1 = base_uv * texture_scales[layer1];
    let uv2 = base_uv * texture_scales[layer2];
    let uv3 = base_uv * texture_scales[layer3];

    // --- Layer compositing ---
    // Original: multi-pass with SrcAlpha/InvSrcAlpha hardware blending
    // Equivalent to: result = mix(prev, layer_color, layer_alpha)
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    if (layer1 > 0u && alphas.r > 0.0) {
        let c1 = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, c1, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let c2 = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, c2, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let c3 = textureSampleLevel(t_terrain_array, s_terrain, uv3, layer3, 0.0).rgb;
        color = mix(color, c3, alphas.b);
    }

    // --- Debug modes ---
    if (params.debug_mode == 1u) {
        return vec4<f32>(uv.x, uv.y, 0.0, 1.0);           // UV gradient
    }
    if (params.debug_mode == 2u) {
        return vec4<f32>(f32(grid_x) / 16.0, f32(grid_z) / 16.0, 0.0, 1.0); // Chunk ID
    }
    if (params.debug_mode == 3u) {
        return vec4<f32>(alphas.r, alphas.g, alphas.b, 1.0); // Alpha channels
    }
    if (params.debug_mode == 4u) {
        return vec4<f32>(textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb, 1.0);
    }
    if (params.debug_mode == 5u) {
        // base_uv / 16 should match Mode 11 (sample_uv) exactly
        return vec4<f32>(base_uv.x / 16.0, base_uv.y / 16.0, 0.0, 1.0);
    }

    return vec4<f32>(color, 1.0);
}
