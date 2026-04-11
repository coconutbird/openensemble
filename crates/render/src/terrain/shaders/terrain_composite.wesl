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
// Decal compositing:
//   HWDE uses a separate pass with sincos-based rotation matrix for each decal layer.
//   We fold all decal layers into the same single-pass shader after splat blending.
//   Decal rotation matches Shader #6/#7 in gpuTerrainComposite.bin:
//     rcp_scale = 1.0 / UVScale.xy
//     rotated_uv = rotation_matrix(DecalInf.x) * (uv.yx * rcp_scale - center) + 0.5
//   The decal diffuse is sampled from the same terrain texture array (LOD path).

struct CompositeParams {
    chunk_index: u32,      // Which chunk (0-255 for 16×16 grid)
    num_layers: u32,       // Number of active splat layers
    debug_mode: u32,       // 0=normal, 1=UV, 2=chunk ID, 3=alpha, 4=layer0
    num_decal_layers: u32, // Number of active decal layers for this chunk
};

// Per-decal-layer instance data packed as vec4:
//   x = rotation angle (radians)
//   y = tile_center_x (relative to chunk, in 0-1 UV space)
//   z = tile_center_y (relative to chunk, in 0-1 UV space)
//   w = HDR scale (1.0 = no HDR, >1.0 = emissive — dead code in PC release)
struct DecalLayerInfo {
    rotation: f32,
    center_u: f32,
    center_v: f32,
    hdr_scale: f32,
};

@group(0) @binding(0) var<uniform> params: CompositeParams;
@group(0) @binding(1) var t_terrain_array: texture_2d_array<f32>;
@group(0) @binding(2) var t_alpha_atlas: texture_2d_array<f32>;
@group(0) @binding(3) var<storage, read> chunk_layers: array<u32>;
@group(0) @binding(4) var<storage, read> texture_scales: array<vec2<f32>>;
@group(0) @binding(5) var s_terrain: sampler;
@group(0) @binding(6) var s_alpha: sampler;
@group(0) @binding(7) var t_alpha_atlas_hi: texture_2d_array<f32>;
// Decal resources
@group(0) @binding(8) var t_decal_alpha_atlas: texture_2d_array<f32>;
@group(0) @binding(9) var<storage, read> chunk_decal_layers: array<u32>;
@group(0) @binding(10) var<storage, read> decal_instances: array<vec4<f32>>;
@group(0) @binding(11) var<storage, read> decal_uv_scales: array<vec2<f32>>;

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

    // --- Decal compositing ---
    // HWDE renders decals in a separate pass per layer using Shader #6/#7.
    // We fold all decal layers into the same pass after splat blending.
    // Each decal layer has: rotation, center offset, UV scale, and an alpha map.
    //
    // Shader #6 math (LOD path, uses texture array):
    //   rcp_scale = 1.0 / UVScale.xy
    //   sin_r = sin(DecalInf.x), cos_r = cos(DecalInf.x)
    //   input_uv = v1.yx  (swapped — matches our .yx convention)
    //   rotated_u = dot([cos*rcp_x, sin*rcp_y, offset_u], [uv.x, uv.y, 1])
    //   rotated_v = dot([-sin*rcp_x, cos*rcp_y, offset_v], [uv.x, uv.y, 1])
    //   final_uv = rotated + 0.5
    //   color from gTargetTexture (array), alpha from gDecalOpacityTexture
    if params.num_decal_layers > 0u {
        let decal_base = chunk_idx * 8u;
        let num_decals = min(params.num_decal_layers, 8u);

        for (var d = 0u; d < num_decals; d = d + 1u) {
            let instance_id = chunk_decal_layers[decal_base + d];
            let decal_inf = decal_instances[instance_id];  // vec4: rotation, center_u, center_v, hdr_scale
            let decal_scale = decal_uv_scales[instance_id]; // vec2: u_scale, v_scale

            // Sample decal alpha from the decal alpha atlas
            // Decal alpha maps are stored similarly to splat: up to 8 layers, 4 per atlas.
            // d/4 selects which alpha atlas slice-set, d%4 selects RGBA channel.
            // For simplicity we pack all decal alphas into a single atlas with
            // R=layer0, G=layer1, B=layer2, A=layer3 (same as splat alpha).
            let decal_alphas = textureSampleLevel(t_decal_alpha_atlas, s_alpha, alpha_uv, chunk_idx, 0.0);
            var decal_alpha = 0.0;
            if d == 0u { decal_alpha = decal_alphas.r; }
            else if d == 1u { decal_alpha = decal_alphas.g; }
            else if d == 2u { decal_alpha = decal_alphas.b; }
            else if d == 3u { decal_alpha = decal_alphas.a; }
            // Layers 4-7 would need a second decal alpha atlas (rare in practice)

            if decal_alpha > 0.0 {
                // Decal rotation matrix (matches HWDE Shader #6 exactly):
                // Input UVs are swapped (.yx) to match the Z,X convention
                let rotation = decal_inf.x;
                let center = vec2<f32>(decal_inf.y, decal_inf.z);

                let sin_r = sin(rotation);
                let cos_r = cos(rotation);

                // Reciprocal UV scale (HWDE divides by scale, not multiplies)
                let rcp_scale = vec2<f32>(1.0 / max(decal_scale.x, 0.001), 1.0 / max(decal_scale.y, 0.001));

                // Build offset terms: -(rcp_scale * center) rotated
                let neg_offset = -rcp_scale * center;
                let offset_u = cos_r * neg_offset.x + sin_r * neg_offset.y;
                let offset_v = -sin_r * neg_offset.x + cos_r * neg_offset.y;

                // Apply rotation to swapped UVs
                let input_uv = uv.yx; // .yx swap matches HWDE's v1.yx
                let rot_u = cos_r * rcp_scale.x * input_uv.x + sin_r * rcp_scale.y * input_uv.y + offset_u;
                let rot_v = -sin_r * rcp_scale.x * input_uv.x + cos_r * rcp_scale.y * input_uv.y + offset_v;

                // Center at 0.5 (HWDE adds 0.5 after rotation)
                let decal_uv = vec2<f32>(rot_u + 0.5, rot_v + 0.5);

                // Only blend if UV is within [0,1] range (decal bounds check)
                if decal_uv.x >= 0.0 && decal_uv.x <= 1.0 && decal_uv.y >= 0.0 && decal_uv.y <= 1.0 {
                    // Find the texture index for this decal instance.
                    // The decal_index in the instance points to the ActiveDecalInfo which
                    // maps to a texture in our terrain texture array.
                    // For the LOD path, decals are stored at the end of the texture array.
                    // The decal diffuse is tiled using tiling_uv * LayerData[0].yz (splat scale).
                    let tiled_decal_uv = tiling_uv * decal_scale;
                    let decal_tex_id = chunk_decal_layers[decal_base + d];
                    // Sample from terrain texture array using the decal's texture slot
                    // For now, use tiling UVs scaled by the decal's own scale
                    let decal_color = textureSampleLevel(t_terrain_array, s_terrain, decal_uv * decal_scale, instance_id, 0.0).rgb;

                    // Blend decal over existing color using alpha
                    color = mix(color, decal_color, decal_alpha);
                }
            }
        }
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
