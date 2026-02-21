// GPU tessellation shader using instanced patches with vertex displacement
// Each instance is a terrain patch, vertices sample position/normal from textures

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Tessellation parameters for decoding packed positions
struct TessParams {
    mid: vec4<f32>,           // x, y, z, pad
    range: vec4<f32>,         // x, y, z, pad
    terrain_info: vec4<f32>,  // num_verts, tile_scale, num_patches_x, num_patches_z
    world_min: vec4<f32>,     // x, y, z, pad
    world_max: vec4<f32>,     // x, y, z, pad
};
@group(1) @binding(0)
var<uniform> tess_params: TessParams;

// Position texture (R32Uint - packed R10G10B10A2)
@group(1) @binding(1)
var t_positions: texture_2d<u32>;

// Normal texture (R32Uint - packed)
@group(1) @binding(2)
var t_normals: texture_2d<u32>;

// XTT albedo texture for coloring
@group(1) @binding(3)
var t_xtt_albedo: texture_2d<f32>;
@group(1) @binding(4)
var s_terrain: sampler;

// Terrain params (reuse from main shader for debug mode)
struct TerrainParams {
    terrain_size: vec2<f32>,
    chunk_count: vec2<f32>,
    texture_tile_scale: f32,
    debug_mode: f32,
    bump_power: f32,
    _pad3: f32,
};
@group(1) @binding(5)
var<uniform> params: TerrainParams;

// AO texture (R8Unorm - 0=occluded, 1=fully lit)
@group(1) @binding(6)
var t_ao: texture_2d<f32>;

// Alpha texture (R8Unorm - 255=opaque, 0=transparent/hole)
@group(1) @binding(7)
var t_alpha: texture_2d<f32>;

// Normal map texture array (tangent-space normals)
@group(1) @binding(8)
var t_normal_array: texture_2d_array<f32>;

// Terrain texture array (diffuse textures for splatting)
@group(1) @binding(9)
var t_terrain_array: texture_2d_array<f32>;

// Alpha atlas for all chunks (16x16 chunks, 64x64 per chunk = 1024x1024)
@group(1) @binding(10)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data (256 chunks * 8 layer IDs = 2048 u32s)
@group(1) @binding(11)
var<storage, read> chunk_layers: array<u32>;

// Alpha sampler (nearest filtering to avoid chunk boundary bleeding)
@group(1) @binding(12)
var s_alpha: sampler;

// Pre-composited albedo atlas (all layers blended on CPU)
@group(1) @binding(13)
var t_composited: texture_2d<f32>;

// Per-texture UV scales (8 textures * vec2<f32> = 16 floats)
@group(1) @binding(14)
var<storage, read> texture_scales: array<vec2<f32>>;

// GPU-composited albedo atlas (from compositor, 8K×8K for 256 chunks)
@group(1) @binding(15)
var t_gpu_composited: texture_2d<f32>;

struct VertexInput {
    // Per-vertex: local UV within patch [0, 1]
    @location(0) local_uv: vec2<f32>,
    // Per-instance: patch index (x + z * num_patches_x)
    @location(1) patch_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Decode packed R10G10B10A2 position
fn unpack_position(packed: u32, mid: vec3<f32>, range: vec3<f32>) -> vec3<f32> {
    let x_bits = packed & 0x3FFu;
    let y_bits = (packed >> 10u) & 0x3FFu;
    let z_bits = (packed >> 20u) & 0x3FFu;

    let norm = vec3<f32>(
        f32(x_bits) / 1023.0,
        f32(y_bits) / 1023.0,
        f32(z_bits) / 1023.0
    );

    return norm * range - mid;
}

// Decode packed normal (x=bits 22-31, y=bits 11-20, z=bits 0-9)
fn unpack_normal(packed: u32) -> vec3<f32> {
    let x_bits = (packed >> 22u) & 0x3FFu;
    let y_bits = (packed >> 11u) & 0x3FFu;
    let z_bits = packed & 0x3FFu;

    let norm = vec3<f32>(
        (f32(x_bits) / 1023.0) * 2.0 - 1.0,
        (f32(y_bits) / 1023.0) * 2.0 - 1.0,
        (f32(z_bits) / 1023.0) * 2.0 - 1.0
    );

    return normalize(norm);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts = u32(tess_params.terrain_info.x);
    let tile_scale = tess_params.terrain_info.y;
    let num_patches_x = u32(tess_params.terrain_info.z);
    let num_patches_z = u32(tess_params.terrain_info.w);

    let patch_x = in.patch_index % num_patches_x;
    let patch_z = in.patch_index / num_patches_x;

    let verts_per_patch_f = f32(num_verts - 1u) / f32(num_patches_x);

    // Global UV across entire terrain [0, 1]
    let raw_u = (f32(patch_x) + in.local_uv.x) / f32(num_patches_x);
    let raw_v = (f32(patch_z) + in.local_uv.y) / f32(num_patches_z);
    let uv_steps = f32(num_verts - 1u);
    let global_u = round(raw_u * uv_steps) / uv_steps;
    let global_v = round(raw_v * uv_steps) / uv_steps;

    let tex_x = u32(global_u * f32(num_verts - 1u));
    let tex_z = u32(global_v * f32(num_verts - 1u));
    let clamped_x = min(tex_x, num_verts - 1u);
    let clamped_z = min(tex_z, num_verts - 1u);

    let packed_pos = textureLoad(t_positions, vec2<i32>(i32(clamped_x), i32(clamped_z)), 0).r;
    let packed_norm = textureLoad(t_normals, vec2<i32>(i32(clamped_x), i32(clamped_z)), 0).r;

    let local_pos = unpack_position(packed_pos, tess_params.mid.xyz, tess_params.range.xyz);
    let grid_x = global_u * f32(num_verts - 1u);
    let grid_z = global_v * f32(num_verts - 1u);

    let world_pos = vec3<f32>(
        grid_x * tile_scale + local_pos.x,
        local_pos.y,
        grid_z * tile_scale + local_pos.z
    );

    let normal = unpack_normal(packed_norm);

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.world_pos = world_pos;
    out.uv = vec2<f32>(global_u, global_v);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Compute UV from world position to ensure continuity across patch boundaries
    let num_verts = u32(tess_params.terrain_info.x);
    let tile_scale = tess_params.terrain_info.y;
    let terrain_extent = f32(num_verts - 1u) * tile_scale;
    let uv_from_world = vec2<f32>(
        in.world_pos.x / terrain_extent,
        in.world_pos.z / terrain_extent
    );
    let sample_uv = uv_from_world;

    // Sample alpha texture for terrain holes/transparency
    let alpha = textureSample(t_alpha, s_terrain, sample_uv).r;
    if (alpha < 0.5) {
        discard;
    }

    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let vertex_normal = normalize(vec3<f32>(0.0, 1.0, 0.0));

    // Determine which chunk this pixel belongs to (0-15 in each axis)
    let chunk_count = vec2<f32>(params.chunk_count.x, params.chunk_count.y);
    let chunk_uv = sample_uv * chunk_count;
    let chunk_x = u32(clamp(floor(chunk_uv.x), 0.0, chunk_count.x - 1.0));
    let chunk_y = u32(clamp(floor(chunk_uv.y), 0.0, chunk_count.y - 1.0));
    let chunk_idx = chunk_y * u32(chunk_count.x) + chunk_x;

    let in_chunk_uv = fract(chunk_uv);
    let alpha_atlas_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv) / chunk_count;
    let alphas = textureSample(t_alpha_atlas, s_alpha, alpha_atlas_uv);

    // Get layer indices for this chunk (8 layers max per chunk)
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Calculate per-layer tiled UVs
    let base_uv = sample_uv * chunk_count;
    let uv0 = base_uv * texture_scales[layer0];
    let uv1 = base_uv * texture_scales[layer1];
    let uv2 = base_uv * texture_scales[layer2];
    let uv3 = base_uv * texture_scales[layer3];

    // Runtime texture splatting
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    if (layer1 > 0u && alphas.r > 0.0) {
        let layer1_color = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, layer1_color, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let layer2_color = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, layer2_color, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let layer3_color = textureSampleLevel(t_terrain_array, s_terrain, uv3, layer3, 0.0).rgb;
        color = mix(color, layer3_color, alphas.b);
    }

    // Blend normal maps from individual textures
    let nm0 = textureSampleLevel(t_normal_array, s_terrain, uv0, layer0, 0.0).rg * 2.0 - 1.0;
    var tangent_normal = vec3<f32>(nm0.x, nm0.y, sqrt(max(0.0, 1.0 - nm0.x * nm0.x - nm0.y * nm0.y)));

    if (layer1 > 0u && alphas.r > 0.0) {
        let nm1 = textureSampleLevel(t_normal_array, s_terrain, uv1, layer1, 0.0).rg * 2.0 - 1.0;
        let layer1_normal = vec3<f32>(nm1.x, nm1.y, sqrt(max(0.0, 1.0 - nm1.x * nm1.x - nm1.y * nm1.y)));
        tangent_normal = mix(tangent_normal, layer1_normal, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let nm2 = textureSampleLevel(t_normal_array, s_terrain, uv2, layer2, 0.0).rg * 2.0 - 1.0;
        let layer2_normal = vec3<f32>(nm2.x, nm2.y, sqrt(max(0.0, 1.0 - nm2.x * nm2.x - nm2.y * nm2.y)));
        tangent_normal = mix(tangent_normal, layer2_normal, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let nm3 = textureSampleLevel(t_normal_array, s_terrain, uv3, layer3, 0.0).rg * 2.0 - 1.0;
        let layer3_normal = vec3<f32>(nm3.x, nm3.y, sqrt(max(0.0, 1.0 - nm3.x * nm3.x - nm3.y * nm3.y)));
        tangent_normal = mix(tangent_normal, layer3_normal, alphas.b);
    }

    // Apply bump power scaling
    tangent_normal.x = tangent_normal.x * params.bump_power;
    tangent_normal.y = tangent_normal.y * params.bump_power;
    tangent_normal.z = sqrt(max(0.0, 1.0 - tangent_normal.x * tangent_normal.x - tangent_normal.y * tangent_normal.y));
    tangent_normal = normalize(tangent_normal);

    // Build TBN matrix
    let up = vec3<f32>(0.0, 1.0, 0.0);
    var tangent = normalize(cross(up, vertex_normal));
    if (length(tangent) < 0.001) {
        tangent = vec3<f32>(1.0, 0.0, 0.0);
    }
    let bitangent = normalize(cross(vertex_normal, tangent));
    let tbn = mat3x3<f32>(tangent, bitangent, vertex_normal);
    let perturbed_normal = normalize(tbn * tangent_normal);

    let diff = max(dot(perturbed_normal, light_dir), 0.0);

    let ao = textureSample(t_ao, s_terrain, sample_uv).r;
    let ao_intensity = 0.8;
    let ao_factor = mix(1.0, ao, ao_intensity);

    let ambient = 0.4 * ao_factor;
    let diffuse = diff * 0.6 * mix(1.0, ao_factor, 0.3);
    let lighting = ambient + diffuse;

    // Debug modes
    if (params.debug_mode > 11.5 && params.debug_mode < 12.5) {
        // Debug mode 12: GPU-composited atlas output
        // Sample from the 8K×8K GPU-composited atlas
        let gpu_composited_color = textureSample(t_gpu_composited, s_terrain, sample_uv);
        return vec4<f32>(gpu_composited_color.rgb, 1.0);
    }
    if (params.debug_mode > 10.5 && params.debug_mode < 11.5) {
        // Debug mode 11: terrain UV visualization
        return vec4<f32>(sample_uv.x, sample_uv.y, 0.0, 1.0);
    }
    if (params.debug_mode > 9.5 && params.debug_mode < 10.5) {
        return vec4<f32>(0.5, 0.7, 0.3, 1.0);  // Solid green
    }
    if (params.debug_mode > 7.5 && params.debug_mode < 8.5) {
        return vec4<f32>(ao, ao, ao, 1.0);
    }
    if (params.debug_mode > 8.5 && params.debug_mode < 9.5) {
        let xtt_color = textureSample(t_xtt_albedo, s_terrain, sample_uv);
        return vec4<f32>(xtt_color.rgb * 0.7, 1.0);
    }
    if (params.debug_mode > 6.5 && params.debug_mode < 7.5) {
        let xtt_color = textureSample(t_xtt_albedo, s_terrain, sample_uv);
        return vec4<f32>(xtt_color.rgb, 1.0);
    }
    if (params.debug_mode > 0.5 && params.debug_mode < 1.5) {
        let edge_threshold = 0.02;
        let is_edge = in_chunk_uv.x < edge_threshold || in_chunk_uv.x > (1.0 - edge_threshold) ||
                      in_chunk_uv.y < edge_threshold || in_chunk_uv.y > (1.0 - edge_threshold);
        if (is_edge) {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    }

    return vec4<f32>(color * lighting, 1.0);
}

