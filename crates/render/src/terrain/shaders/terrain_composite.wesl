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
//   sample_uv = (world_pos.x, world_pos.z) / terrain_extent   (X→U, Z→V)
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
@group(0) @binding(7) var t_alpha_atlas_hi: texture_2d_array<f32>;

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
    // chunk_idx = Z*16+X (XTT layout: grid_x=Z, grid_z=X)
    // grid_x here = Z direction, grid_z here = X direction
    let grid_x = chunk_idx / 16u;  // = Z
    let grid_z = chunk_idx % 16u;  // = X

    // --- Layer indices from storage buffer (up to 8 layers) ---
    let base = chunk_idx * 8u;
    let layer0 = chunk_layers[base + 0u];
    let layer1 = chunk_layers[base + 1u];
    let layer2 = chunk_layers[base + 2u];
    let layer3 = chunk_layers[base + 3u];
    let layer4 = chunk_layers[base + 4u];
    let layer5 = chunk_layers[base + 5u];
    let layer6 = chunk_layers[base + 6u];
    let layer7 = chunk_layers[base + 7u];

    // --- Alpha sampling ---
    // Original: tex3D(alphasSampler, float3(TexCoord0.xy, slice))[channel]
    // Viewport uv = (0→1) within this chunk. uv.x = X direction, uv.y = Z direction.
    // Alpha textures are uploaded transposed (x↔y swap) so texture columns=X, rows=Z.
    // uv maps directly to alpha texture. Apply half-pixel gutter inset (0.5/64 to 63.5/64).
    let half_pixel = 0.5 / 64.0;
    let alpha_uv = mix(vec2<f32>(half_pixel), vec2<f32>(1.0 - half_pixel), uv);
    let alphas = textureSampleLevel(t_alpha_atlas, s_alpha, alpha_uv, chunk_idx, 0.0);
    let alphas_hi = textureSampleLevel(t_alpha_atlas_hi, s_alpha, alpha_uv, chunk_idx, 0.0);

    // --- Tiled texture UVs ---
    // Original HWDE: gPos = (Z, X), TexCoord0 = gPos/64, targetUVs = TexCoord0 * layerScale
    // The original shader samples terrain textures with U=Z, V=X.
    // grid_x = Z, grid_z = X (from chunk_idx decomposition of Z*16+X layout).
    // tiling_uv = (Z + frac_Z, X + frac_X) to match original (Z, X) convention.
    let tiling_uv = vec2<f32>(f32(grid_x) + uv.y, f32(grid_z) + uv.x);

    let uv0 = tiling_uv * texture_scales[layer0];
    let uv1 = tiling_uv * texture_scales[layer1];
    let uv2 = tiling_uv * texture_scales[layer2];
    let uv3 = tiling_uv * texture_scales[layer3];

    // --- Layer compositing (all 8 layers) ---
    // Original: multi-pass with SrcAlpha/InvSrcAlpha hardware blending
    // Equivalent to: result = mix(prev, layer_color, layer_alpha)
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    // Layers 1-3: blend using alphas RGBA (lo atlas)
    // Only check alpha > 0 — texture ID 0 is valid for overlays.
    // Unused layer slots have alpha = 0 in the texture, so they're naturally skipped.
    if alphas.r > 0.0 {
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, c, alphas.r);
    }
    if alphas.g > 0.0 {
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, c, alphas.g);
    }
    if alphas.b > 0.0 {
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv3, layer3, 0.0).rgb;
        color = mix(color, c, alphas.b);
    }
    // Layer 4: blend using alphas.a (lo atlas A channel)
    if alphas.a > 0.0 {
        let uv4 = tiling_uv * texture_scales[layer4];
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv4, layer4, 0.0).rgb;
        color = mix(color, c, alphas.a);
    }
    // Layers 5-7: blend using alphas_hi RGB (hi atlas)
    if alphas_hi.r > 0.0 {
        let uv5 = tiling_uv * texture_scales[layer5];
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv5, layer5, 0.0).rgb;
        color = mix(color, c, alphas_hi.r);
    }
    if alphas_hi.g > 0.0 {
        let uv6 = tiling_uv * texture_scales[layer6];
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv6, layer6, 0.0).rgb;
        color = mix(color, c, alphas_hi.g);
    }
    if alphas_hi.b > 0.0 {
        let uv7 = tiling_uv * texture_scales[layer7];
        let c = textureSampleLevel(t_terrain_array, s_terrain, uv7, layer7, 0.0).rgb;
        color = mix(color, c, alphas_hi.b);
    }

    // --- Debug modes ---
    if params.debug_mode == 1u {
        return vec4<f32>(uv.x, uv.y, 0.0, 1.0);           // UV gradient
    }
    if params.debug_mode == 2u {
        return vec4<f32>(f32(grid_x) / 16.0, f32(grid_z) / 16.0, 0.0, 1.0); // Chunk ID
    }
    if params.debug_mode == 3u {
        return vec4<f32>(alphas.r, alphas.g, alphas.b, 1.0); // Alpha channels
    }
    if params.debug_mode == 4u {
        return vec4<f32>(textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb, 1.0);
    }
    if params.debug_mode == 5u {
        // tiling_uv / 16 shows the tiling UV gradient
        return vec4<f32>(tiling_uv.x / 16.0, tiling_uv.y / 16.0, 0.0, 1.0);
    }

    return vec4<f32>(color, 1.0);
}
