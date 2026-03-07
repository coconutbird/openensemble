import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::fog::compute_radial_fog_density;
import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::normal::compute_tbn;
import package::shared::specular::compute_blinn_phong_specular;
import package::shared::shadows::vsm_shadow;
import package::shared::blackmap::apply_blackmap;
import package::shared::local_lights::evaluate_omni_specular;

// GPU tessellation shader - aligned with original Halo Wars gpuTerrainXbox.fx
//
// This shader implements the terrain rendering pipeline:
// - Instanced patch rendering with vertex displacement from textures
// - Runtime texture splatting (mode 9) or GPU-composited atlas (mode 12)
// - Normal map blending across splat layers
// - Ambient occlusion and terrain holes (alpha)
//
// Original Halo Wars approach:
// - Uses pre-composited "unique" textures from gpuTerrainComposite.fx
// - Samples UniqueAlbedoSampler for composited color
// - Applies lighting, AO, and fog
//
// Our implementation supports both:
// - Mode 9: Runtime splatting (blend textures per-pixel like compositor)
// - Mode 12: GPU-composited atlas (sample pre-blended texture like original)

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

// Lighting parameters - matches original Halo Wars cbShared lighting fields
struct LightingParams {
    dir_light_vec: vec4<f32>,       // xyz = direction to light, w = enabled
    dir_light_color: vec4<f32>,     // rgb = color, a = shadow_darkness
    world_camera_pos: vec4<f32>,    // xyz = camera pos
    sh_fill_ar: vec4<f32>,          // SH linear R
    sh_fill_ag: vec4<f32>,          // SH linear G
    sh_fill_ab: vec4<f32>,          // SH linear B
    sh_fill_br: vec4<f32>,          // SH quadratic R
    sh_fill_bg: vec4<f32>,          // SH quadratic G
    sh_fill_bb: vec4<f32>,          // SH quadratic B
    sh_fill_c: vec4<f32>,           // SH final quadratic
    fog_color: vec4<f32>,           // rgb = fog color
    fog_params: vec4<f32>,          // x = fog_density2, y = fog_start2
    planar_fog_color: vec4<f32>,    // rgb = planar fog color
    planar_fog_params: vec4<f32>,   // x = enabled, y = start, z = density2
    ao_params: vec4<f32>,           // x = ao_diffuse_intensity
    // Shadow params
    shadow_vp_col0: vec4<f32>,      // Shadow view-proj matrix column 0
    shadow_vp_col1: vec4<f32>,      // column 1
    shadow_vp_col2: vec4<f32>,      // column 2
    shadow_vp_col3: vec4<f32>,      // column 3
    shadow_params: vec4<f32>,       // x = csm_scale, y = num_passes, z = enabled
    // Blackmap params
    blackmap_params0: vec4<f32>,    // rgb = bg_color, w = fog_scalar
    blackmap_params1: vec4<f32>,    // x = unexplored_scalar, yz = bounds_lo_xz, w = enabled
    blackmap_params2: vec4<f32>,    // x = pad, yz = bounds_hi_xz, w = bounds_falloff
    // Local light params
    local_light_params: vec4<f32>,  // x = num_lights, y = spec_power
};
@group(1) @binding(16)
var<uniform> lighting: LightingParams;

// Shadow map texture (depth values from light's perspective)
@group(1) @binding(17)
var t_shadow_map: texture_2d<f32>;

// Blackmap visibility texture (alpha = visibility factor)
@group(1) @binding(18)
var t_blackmap: texture_2d<f32>;

// Blackmap unexplored mask texture
@group(1) @binding(19)
var t_unexplored: texture_2d<f32>;

// Local lights storage buffer (8 vec4s per light, up to 20 lights)
@group(1) @binding(20)
var<storage, read> local_lights: array<vec4<f32>>;

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
    @location(3) tangent: vec3<f32>,
    @location(4) binormal: vec3<f32>,
    @location(5) fog_densities: vec2<f32>,  // x = radial, y = planar
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

    // Compute TBN from vertex normal (matches original GiveTBNFromNormal)
    let tbn = compute_tbn(normal);

    // Compute per-vertex fog densities (matches original fogHelpers.inc)
    let z_fog_density = compute_radial_fog_density(world_pos, lighting.world_camera_pos.xyz, lighting.fog_params.x, lighting.fog_params.y);
    var planar_fog_density = 1.0;
    if (lighting.planar_fog_params.x > 0.5) {
        planar_fog_density = compute_planar_fog_density(world_pos.y, lighting.planar_fog_params.y, lighting.planar_fog_params.z);
    }

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.world_pos = world_pos;
    out.uv = vec2<f32>(global_u, global_v);
    out.tangent = tbn[0];
    out.binormal = tbn[1];
    out.fog_densities = vec2<f32>(z_fog_density, planar_fog_density);

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

    let vertex_normal = normalize(in.normal);

    // ==========================================================================
    // TEXTURE SPLATTING - Runtime compositing aligned with gpuTerrainComposite.fx
    // ==========================================================================
    // Determine which chunk this pixel belongs to (0-15 in each axis)
    let chunk_count = vec2<f32>(params.chunk_count.x, params.chunk_count.y);
    let chunk_uv = sample_uv * chunk_count;
    let chunk_x = u32(clamp(floor(chunk_uv.x), 0.0, chunk_count.x - 1.0));
    let chunk_y = u32(clamp(floor(chunk_uv.y), 0.0, chunk_count.y - 1.0));

    // Chunk index uses Y-major ordering: chunk_idx = chunk_y * 16 + chunk_x
    // This matches the compositor's chunk_index and layer buffer layout
    let chunk_idx = chunk_y * u32(chunk_count.x) + chunk_x;

    // UV within the chunk (0-1), equivalent to original's TexCoord0
    let in_chunk_uv = fract(chunk_uv);

    // Sample alpha atlas
    // Original: tex3D(alphasSampler, float3(uv0, layer_group_z))[channel]
    // Ours: 2D atlas where (chunk_x, chunk_y) region stores RGBA alpha values
    let alpha_atlas_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv) / chunk_count;
    let alphas = textureSample(t_alpha_atlas, s_alpha, alpha_atlas_uv);

    // Get layer texture indices for this chunk
    // Equivalent to original's g_LayerData[].x (layer index)
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Calculate tiled UVs with per-texture scaling
    // Original: Output.targetUVs = TexCoord0 * g_LayerData[indx].yz
    // We use continuous UVs (base_uv) to ensure seamless chunk boundaries
    let base_uv = sample_uv * chunk_count;
    let uv0 = base_uv * texture_scales[layer0];
    let uv1 = base_uv * texture_scales[layer1];
    let uv2 = base_uv * texture_scales[layer2];
    let uv3 = base_uv * texture_scales[layer3];

    // Runtime texture splatting - same blending as compositor
    // Original uses hardware alpha blending (SrcAlpha/OneMinusSrcAlpha)
    // We use mix() which is mathematically equivalent:
    //   mix(a, b, t) = a * (1-t) + b * t

    // Layer 0: base layer (always 100% coverage)
    var color = textureSampleLevel(t_terrain_array, s_terrain, uv0, layer0, 0.0).rgb;

    // Layer 1: blend using alphas.r
    if (layer1 > 0u && alphas.r > 0.0) {
        let layer1_color = textureSampleLevel(t_terrain_array, s_terrain, uv1, layer1, 0.0).rgb;
        color = mix(color, layer1_color, alphas.r);
    }
    // Layer 2: blend using alphas.g
    if (layer2 > 0u && alphas.g > 0.0) {
        let layer2_color = textureSampleLevel(t_terrain_array, s_terrain, uv2, layer2, 0.0).rgb;
        color = mix(color, layer2_color, alphas.g);
    }
    // Layer 3: blend using alphas.b
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

    // Build TBN matrix from interpolated vertex tangent/binormal/normal
    // (matches original GiveTBNFromNormal computed per-vertex)
    let tangent = normalize(in.tangent);
    let binormal = normalize(in.binormal);
    let tbn = mat3x3<f32>(tangent, binormal, vertex_normal);
    let world_normal = normalize(tbn * tangent_normal);

    // Sample AO texture
    let ao = textureSample(t_ao, s_terrain, sample_uv).r;
    let ao_intensity = lighting.ao_params.x;
    let ao_factor = mix(1.0, ao, ao_intensity);

    // === Lighting pipeline (matches original shFillLighting.inc + dirLighting.inc) ===

    // SH fill lighting (ambient)
    let sh_ambient = compute_sh_fill_lighting(world_normal, lighting.sh_fill_ar, lighting.sh_fill_ag, lighting.sh_fill_ab, lighting.sh_fill_br, lighting.sh_fill_bg, lighting.sh_fill_bb, lighting.sh_fill_c);

    // Directional light
    let light_dir = normalize(lighting.dir_light_vec.xyz);
    let n_dot_l = max(dot(world_normal, light_dir), 0.0);
    var dir_diffuse = lighting.dir_light_color.rgb * n_dot_l;

    // === Directional shadow (VSM) ===
    var shadow_factor = 1.0;
    if (lighting.shadow_params.z > 0.5) {
        // Transform world position to shadow space
        let shadow_vp = mat4x4<f32>(
            lighting.shadow_vp_col0,
            lighting.shadow_vp_col1,
            lighting.shadow_vp_col2,
            lighting.shadow_vp_col3,
        );
        let shadow_pos = shadow_vp * vec4<f32>(in.world_pos, 1.0);
        let shadow_ndc = shadow_pos.xyz / shadow_pos.w;
        let shadow_uv = vec2<f32>(shadow_ndc.x * 0.5 + 0.5, shadow_ndc.y * -0.5 + 0.5);
        let shadow_z = clamp(shadow_ndc.z, 0.0, 1.0);

        shadow_factor = vsm_shadow(shadow_uv, shadow_z, lighting.dir_light_color.a, t_shadow_map, s_terrain);
    }
    dir_diffuse *= shadow_factor;

    // Specular (Blinn-Phong, matching heightfield/roads shaders)
    let view_dir = normalize(lighting.world_camera_pos.xyz - in.world_pos);
    let spec_power = 16.0; // Default terrain specular power
    let spec_intensity = compute_blinn_phong_specular(world_normal, light_dir, view_dir, spec_power);
    let dir_spec = lighting.dir_light_color.rgb * spec_intensity * shadow_factor;

    // === Local/point lights ===
    var local_diffuse = vec3<f32>(0.0);
    var local_spec = 0.0;
    let num_lights = u32(lighting.local_light_params.x);
    let local_spec_power = lighting.local_light_params.y;
    let world_reflect = reflect(-view_dir, world_normal);

    for (var i = 0u; i < num_lights && i < 20u; i++) {
        let base = i * 8u;
        let light_pos = local_lights[base].xyz;
        let omni_mul = local_lights[base].w;
        let light_color = local_lights[base + 1u].xyz;
        let omni_add = local_lights[base + 1u].w;
        let decay_dist = local_lights[base + 2u].x;
        let spec_int = local_lights[base + 3u].w;

        let result = evaluate_omni_specular(
            light_pos, light_color, omni_mul, omni_add, decay_dist, spec_int,
            in.world_pos, world_normal, world_reflect, local_spec_power,
        );
        local_diffuse += result.xyz;
        local_spec += result.w;
    }

    // Combine: (ambient * ao + directional + local) * albedo + specular
    let ambient_sum = sh_ambient * ao_factor;
    var lit_color = (ambient_sum + dir_diffuse + local_diffuse) * color;
    // Specular: directional + local (terrain uses AO as specular mask)
    lit_color += (dir_spec * 0.15 + local_spec) * ao_factor;

    // === Blackmap (fog-of-war) ===
    if (lighting.blackmap_params1.w > 0.5) {
        let blackmap_uv = sample_uv; // Use terrain UV for blackmap lookup
        let vis_factor = textureSample(t_blackmap, s_terrain, blackmap_uv).a;
        let unexplored_sample = textureSample(t_unexplored, s_terrain, blackmap_uv);
        lit_color = apply_blackmap(
            lit_color, in.world_pos, vis_factor, unexplored_sample,
            lighting.blackmap_params0.xyz,  // bg_color
            lighting.blackmap_params0.w,    // fog_scalar
            lighting.blackmap_params1.x,    // unexplored_scalar
            lighting.blackmap_params1.yz,   // bounds_lo
            lighting.blackmap_params2.yz,   // bounds_hi
            lighting.blackmap_params2.w,    // bounds_falloff
        );
    }

    // Apply fog (interpolated from vertex shader)
    let final_color = compute_fog(lit_color, in.fog_densities.x, in.fog_densities.y, lighting.fog_color.xyz, lighting.planar_fog_color.xyz);

    // Debug modes
    if (params.debug_mode > 13.5 && params.debug_mode < 14.5) {
        // Debug mode 14: Show layer0 texture ID per chunk (visualize layer buffer lookup)
        // This helps verify if the layer buffer indexing matches screen position
        return vec4<f32>(f32(layer0) / 8.0, 0.0, 0.0, 1.0);
    }
    if (params.debug_mode > 12.5 && params.debug_mode < 13.5) {
        // Debug mode 13: Chunk grid visualization with corner markers
        // Corner markers (2x2 chunk areas):
        // - (0,0) corner: BRIGHT RED (world origin)
        // - (15,0) corner: BRIGHT BLUE (+X direction)
        // - (0,15) corner: BRIGHT GREEN (+Z direction)
        // - (15,15) corner: BRIGHT YELLOW (diagonal)
        // Rest: gradient R=chunk_x/16, G=chunk_y/16
        if (chunk_x < 2u && chunk_y < 2u) {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0); // RED = origin (0,0)
        }
        if (chunk_x > 13u && chunk_y < 2u) {
            return vec4<f32>(0.0, 0.0, 1.0, 1.0); // BLUE = +X direction (15,0)
        }
        if (chunk_x < 2u && chunk_y > 13u) {
            return vec4<f32>(0.0, 1.0, 0.0, 1.0); // GREEN = +Z direction (0,15)
        }
        if (chunk_x > 13u && chunk_y > 13u) {
            return vec4<f32>(1.0, 1.0, 0.0, 1.0); // YELLOW = diagonal (15,15)
        }
        return vec4<f32>(f32(chunk_x) / 16.0, f32(chunk_y) / 16.0, 0.0, 1.0);
    }
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

    return vec4<f32>(final_color, 1.0);
}
