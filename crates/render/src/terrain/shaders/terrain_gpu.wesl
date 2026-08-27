import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::fog::compute_radial_fog_density;
import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::normal::compute_tbn;
import package::shared::shadows::compute_dir_shadow;
import package::shared::blackmap::apply_blackmap;
import package::shared::local_lights::evaluate_light_volume;
import package::shared::local_lights::evaluate_spot_specular_shadowed;
import package::shared::specular::specular_schlick;

// GPU tessellation shader - aligned with original Halo Wars gpuTerrainXbox.fx
//
// This shader implements the terrain rendering pipeline:
// - Instanced patch rendering with vertex displacement from textures
// - Precomposited unique albedo, normal, and specular maps
// - Ambient occlusion and terrain holes (alpha)
//
// Original Halo Wars approach:
// - Uses pre-composited "unique" textures from gpuTerrainComposite.fx
// - Samples UniqueAlbedoSampler for composited color
// - Applies lighting, AO, and fog
//
// Source layers are composited before this shader runs, as in the oracle.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Tessellation parameters for decoding packed positions
struct TessParams {
    mid: vec4<f32>,           // xyz = decode minimum, w = normalized Y offset
    range: vec4<f32>,         // x, y, z, pad
    terrain_info: vec4<f32>,  // num_verts, tile_scale, num_patches_x, num_patches_z
    world_min: vec4<f32>,     // x, y, z, pad
    world_max: vec4<f32>,     // x, y, z, pad
};
@group(1) @binding(0)
var<uniform> tess_params: TessParams;

// Position texture in the oracle's filterable R10G10B10A2_UNORM format.
@group(1) @binding(1)
var t_positions: texture_2d<f32>;

// Normal/basis texture in R10G10B10A2_UNORM format.
@group(1) @binding(2)
var t_normals: texture_2d<f32>;

@group(1) @binding(3)
var s_position: sampler;

@group(1) @binding(4)
var s_terrain: sampler;

// Terrain params (reuse from main shader for debug mode)
struct TerrainParams {
    terrain_size: vec2<f32>,
    chunk_count: vec2<f32>,
    texture_tile_scale: f32,
    debug_mode: f32,
    bump_power: f32,
    _padding: f32,
};
@group(1) @binding(5)
var<uniform> params: TerrainParams;

// AO texture (R8Unorm - 0=occluded, 1=fully lit)
@group(1) @binding(6)
var t_ao: texture_2d<f32>;

// Alpha texture (R8Unorm - 255=opaque, 0=transparent/hole)
@group(1) @binding(7)
var t_alpha: texture_2d<f32>;

// GPU-composited albedo atlas (from compositor, 8K×8K for 256 chunks)
@group(1) @binding(14)
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
    local_light_params: vec4<f32>,  // x = count, y = spec power, z = shadows, w = enabled
    // Bump fadeout params (HWDE cb4[35])
    fadeout_params: vec4<f32>,      // x = fadeout_min, y = fadeout_max, z = fadeout_bias
    // Blackmap world-position UV scales (HWDE cb4[33-34])
    blackmap_uv_scales: vec4<f32>,  // x = scale_x (cb4[34].x), y = scale_z (cb4[33].w)
    // Secondary directional specular (HWDE cb4[37-38])
    fill_spec_direction_power: vec4<f32>,
    fill_spec_color_shadow: vec4<f32>,
    // 3D local-light volume (HWDE cb4[39-42])
    light_volume_params: vec4<f32>, // x = enabled
    light_volume_row0: vec4<f32>,
    light_volume_row1: vec4<f32>,
    light_volume_row2: vec4<f32>,
};
@group(1) @binding(15)
var<uniform> lighting: LightingParams;

// Shadow map texture (depth values from light's perspective)
@group(1) @binding(16)
var t_shadow_map: texture_2d_array<f32>;

// Blackmap visibility texture (alpha = visibility factor)
@group(1) @binding(17)
var t_blackmap: texture_2d<f32>;

// Blackmap unexplored mask texture
@group(1) @binding(18)
var t_unexplored: texture_2d<f32>;

// Local lights storage buffer (8 vec4s per light, up to 20 lights)
@group(1) @binding(19)
var<storage, read> local_lights: array<vec4<f32>>;

@group(1) @binding(20)
var s_lighting: sampler;

struct VertexInput {
    // Per-vertex: local UV within patch [0, 1]
    @location(0) local_uv: vec2<f32>,
    // Per-instance: patch index (x + z * num_patches_x)
    @location(1) patch_index: u32,
    // Neighbor-aware hull-shader factors: left, top, right, bottom.
    @location(2) edge_factors: vec4<u32>,
    // Hull-shader inside factors for the domain X and Z axes.
    @location(3) inside_factors: vec2<u32>,
};

// Light texture (L8 luminance from XTD, pre-baked per-vertex lighting)
@group(1) @binding(21)
var t_light: texture_2d<f32>;

// Dynamic terrain visibility mask. Each R32Uint texel stores 32 X-axis bits.
// A set bit keeps the corresponding terrain sample; a clear bit cuts a hole.
@group(1) @binding(22)
var t_dynamic_alpha: texture_2d<u32>;

@group(1) @binding(23)
var t_gpu_composited_normal: texture_2d<f32>;

@group(1) @binding(24)
var t_gpu_composited_specular: texture_2d<f32>;

@group(1) @binding(25)
var t_local_shadow_map: texture_2d_array<f32>;

@group(1) @binding(26)
var t_light_volume_color: texture_3d<f32>;

@group(1) @binding(27)
var t_light_volume_vector: texture_3d<f32>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) binormal: vec3<f32>,
    @location(5) fog_densities: vec2<f32>,  // x = radial, y = planar
    @location(6) light_color: vec3<f32>,    // pre-baked light texture (HWDE v8.xyz)
    @location(7) @interpolate(linear) bump_fadeout: f32, // normal map distance fadeout (HWDE v8.w)
};

fn dynamic_alpha_enabled(grid_coord: vec2<u32>) -> bool {
    let dimensions = textureDimensions(t_dynamic_alpha);
    let word_x = min(grid_coord.x >> 5u, dimensions.x - 1u);
    let row = min(grid_coord.y, dimensions.y - 1u);
    let word = textureLoad(t_dynamic_alpha, vec2<u32>(word_x, row), 0).r;
    return (word & (1u << (grid_coord.x & 31u))) != 0u;
}

fn quantize_domain(value: f32, factor: u32) -> f32 {
    let divisions = f32(max(factor, 1u));
    return round(value * divisions) / divisions;
}

fn tessellated_domain(in: VertexInput) -> vec2<f32> {
    var domain = vec2<f32>(
        quantize_domain(in.local_uv.x, in.inside_factors.x),
        quantize_domain(in.local_uv.y, in.inside_factors.y),
    );

    // Preserve the independently tessellated boundary edges. The fixed 16x16
    // carrier grid then stitches finer shared edges to a coarser patch interior
    // with degenerate triangles, without opening cracks between patches.
    if in.local_uv.y <= 0.0 {
        domain.x = quantize_domain(in.local_uv.x, in.edge_factors.y);
    } else if in.local_uv.y >= 1.0 {
        domain.x = quantize_domain(in.local_uv.x, in.edge_factors.w);
    }
    if in.local_uv.x <= 0.0 {
        domain.y = quantize_domain(in.local_uv.y, in.edge_factors.x);
    } else if in.local_uv.x >= 1.0 {
        domain.y = quantize_domain(in.local_uv.y, in.edge_factors.z);
    }
    return domain;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts = u32(tess_params.terrain_info.x);
    let tile_scale = tess_params.terrain_info.y;
    let num_patches_x = u32(tess_params.terrain_info.z);

    // Fresh domain shaders #23-#27 derive world X from PrimitiveID / patch
    // count and world Z from PrimitiveID % patch count.
    let patch_x = in.patch_index / num_patches_x;
    let patch_z = in.patch_index % num_patches_x;
    let domain = tessellated_domain(in);

    // The oracle advances 16 logical cells per patch.  Packed texture width is
    // the cell count, not the vertex count: the final edge vertex is at
    // `num_verts` and samples the last packed texel through clamping.
    let cell_x = min(patch_x * 16u + u32(round(domain.x * 16.0)), num_verts);
    let cell_z = min(patch_z * 16u + u32(round(domain.y * 16.0)), num_verts);
    let global_u = f32(cell_x) / f32(num_verts);
    let global_v = f32(cell_z) / f32(num_verts);
    // DS #23 samples the filterable position texture at .yx and, unusually,
    // uses the global X UV as its explicit LOD. This blends mip 0 and mip 1
    // across the map and is material to the breadth of terrain features.
    let position_sample = textureSampleLevel(
        t_positions,
        s_position,
        vec2<f32>(global_v, global_u),
        global_u,
    ).bgr;
    let local_pos = vec3<f32>(
        position_sample.x,
        position_sample.y - tess_params.mid.w,
        position_sample.z,
    ) * tess_params.range.xyz - tess_params.mid.xyz;
    let grid_x = f32(cell_x);
    let grid_z = f32(cell_z);

    let world_pos = vec3<f32>(
        grid_x * tile_scale + local_pos.x,
        local_pos.y,
        grid_z * tile_scale + local_pos.z
    );

    // The basis sampler is fixed at LOD 0 and consumed as .zyx * 2 - 1.
    let normal = textureSampleLevel(
        t_normals,
        s_position,
        vec2<f32>(global_v, global_u),
        0.0,
    ).bgr * 2.0 - 1.0;

    // Compute TBN from vertex normal (matches original GiveTBNFromNormal)
    let tbn = compute_tbn(normal);

    // Compute per-vertex fog densities (matches original fogHelpers.inc)
    let z_fog_density = compute_radial_fog_density(world_pos, lighting.world_camera_pos.xyz, lighting.fog_params.x, lighting.fog_params.y);
    var planar_fog_density = 1.0;
    if lighting.planar_fog_params.x > 0.5 {
        planar_fog_density = compute_planar_fog_density(world_pos, lighting.world_camera_pos.xyz, lighting.planar_fog_params.y, lighting.planar_fog_params.z);
    }

    // Sample light texture (HWDE: t2 = gVertSampler_light_Texture)
    // Luminance (L8) sampled at grid UV, passed to PS as v8.xyz
    let light_sample = textureSampleLevel(t_light, s_terrain, vec2<f32>(global_u, global_v), 0.0).r;
    let light_rgb = vec3<f32>(light_sample, light_sample, light_sample);

    // Compute bump fadeout factor (HWDE VS lines 208-215)
    // fadeout = 1.0 - saturate((dist - fadeout_min) / (fadeout_max - fadeout_min + fadeout_bias))
    // fadeout_min = cb4[35].x, fadeout_range = cb4[35].y - cb4[35].x + cb4[35].z
    let cam_vec = world_pos - lighting.world_camera_pos.xyz;
    let cam_dist = length(cam_vec);
    let fadeout_min = lighting.fadeout_params.x;
    let fadeout_range = lighting.fadeout_params.y - lighting.fadeout_params.x + lighting.fadeout_params.z;
    let fadeout = 1.0 - saturate((cam_dist - fadeout_min) / fadeout_range);

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.world_pos = world_pos;
    // Z→U, X→V: match the game's UV convention (e.g. roads shader uses gPos.zx).
    // global_u is X-direction (from patch_x), global_v is Z-direction (from patch_z).
    // Swap so that UV.x = Z, UV.y = X.
    out.uv = vec2<f32>(global_v, global_u);
    out.tangent = tbn[0];
    out.binormal = tbn[1];
    out.fog_densities = vec2<f32>(z_fog_density, planar_fog_density);
    out.light_color = light_rgb;
    out.bump_fadeout = fadeout;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Use grid-based UV from vertex shader, NOT world position.
    // The game computes uvCoord0 = gPos / 64 (grid position only, no displacement).
    // The composited atlas is written using grid-based coordinates, so sampling must
    // also use grid-based UVs. Using world_pos would include the position displacement
    // from unpack_position(), shifting the texture sampling and causing textures to
    // "climb" onto neighboring areas.
    //
    // Vertex shader outputs: out.uv = vec2(global_v, global_u) where
    //   global_u = X direction, global_v = Z direction
    // So in.uv.y = X, in.uv.x = Z. We need sample_uv = (X, Z):
    let sample_uv = vec2<f32>(in.uv.y, in.uv.x);

    if params.debug_mode > 13.5 && params.debug_mode < 14.5 {
        // Debug mode 14: raw GPU geometry height. This intentionally runs
        // before alpha/hole rejection and contains no material, road, foliage,
        // lighting, or shadow contribution.
        let height_range = max(tess_params.world_max.y - tess_params.world_min.y, 0.0001);
        let height = saturate((in.world_pos.y - tess_params.world_min.y) / height_range);
        return vec4<f32>(height, height, height, 1.0);
    }
    if params.debug_mode > 14.5 && params.debug_mode < 15.5 {
        // Debug mode 15: registered material/geometry diagnostic. Magenta
        // contours come from the packed GPU height while the underlying color
        // comes from the canonical compositor.
        let height_range = max(tess_params.world_max.y - tess_params.world_min.y, 0.0001);
        let height = saturate((in.world_pos.y - tess_params.world_min.y) / height_range);
        let contour_phase = fract(height * 32.0);
        let contour_distance = min(contour_phase, 1.0 - contour_phase);
        let contour_width = max(fwidth(height * 32.0) * 0.75, 0.001);
        let contour = 1.0 - smoothstep(0.0, contour_width, contour_distance);
        let albedo = textureSample(t_gpu_composited, s_terrain, sample_uv).rgb;
        return vec4<f32>(mix(albedo, vec3<f32>(1.0, 0.0, 0.8), contour * 0.85), 1.0);
    }

    let num_verts = u32(tess_params.terrain_info.x);
    // XTD/unique texture UV: same grid-based UV.
    let xtd_uv = sample_uv;

    // Sample alpha texture for terrain holes/transparency (XTD-decoded)
    let alpha = textureSample(t_alpha, s_terrain, xtd_uv).r;
    let alpha_grid = vec2<u32>(
        min(u32(saturate(xtd_uv.x) * f32(num_verts - 1u)), num_verts - 1u),
        min(u32(saturate(xtd_uv.y) * f32(num_verts - 1u)), num_verts - 1u),
    );
    let combined_alpha = select(0.0, alpha, dynamic_alpha_enabled(alpha_grid));
    if combined_alpha < 0.66666 {
        discard;
    }

    let vertex_normal = normalize(in.normal);

    // Chunk coordinates are retained only for resource-free debug views.
    let chunk_count = vec2<f32>(params.chunk_count.x, params.chunk_count.y);
    let chunk_uv = sample_uv * chunk_count;
    let grid_x = u32(clamp(floor(chunk_uv.x), 0.0, chunk_count.x - 1.0));
    let grid_z = u32(clamp(floor(chunk_uv.y), 0.0, chunk_count.y - 1.0));
    let in_chunk_uv = fract(chunk_uv);
    let color = textureSample(t_gpu_composited, s_terrain, sample_uv).rgb;
    let unique_xy = textureSample(t_gpu_composited_normal, s_terrain, sample_uv).rg * 2.0 - 1.0;
    var tangent_normal = vec3<f32>(
        unique_xy,
        sqrt(abs(1.0 - dot(unique_xy, unique_xy))),
    );
    let specular_map = textureSample(t_gpu_composited_specular, s_terrain, sample_uv).rgb
        * in.bump_fadeout;

    // The oracle reconstructs Z before applying bump power, then normalizes the
    // scaled XY together with that original Z value.
    tangent_normal.x = tangent_normal.x * params.bump_power;
    tangent_normal.y = tangent_normal.y * params.bump_power;
    tangent_normal = normalize(tangent_normal);

    // Apply distance-based bump fadeout (HWDE PS lines 404-405)
    // Blends bumped normal toward flat (0,0,1) as distance increases.
    // fadeout=1 at close range (full bump), fadeout=0 at far range (flat normal).
    tangent_normal = mix(vec3<f32>(0.0, 0.0, 1.0), tangent_normal, in.bump_fadeout);

    // Build TBN matrix from interpolated vertex tangent/binormal/normal
    // (matches original GiveTBNFromNormal computed per-vertex)
    let tangent = normalize(in.tangent);
    let binormal = normalize(in.binormal);
    let tbn = mat3x3<f32>(tangent, binormal, vertex_normal);
    let world_normal = normalize(tbn * tangent_normal);

    // Sample AO texture (XTD-decoded, uses xtd_uv)
    let ao = textureSample(t_ao, s_terrain, xtd_uv).r;
    let ao_intensity = lighting.ao_params.x;

    // === Lighting pipeline (matches original shFillLighting.inc + dirLighting.inc) ===

    // SH fill lighting (ambient)
    let sh_ambient = compute_sh_fill_lighting(world_normal, lighting.sh_fill_ar, lighting.sh_fill_ag, lighting.sh_fill_ab, lighting.sh_fill_br, lighting.sh_fill_bg, lighting.sh_fill_bb, lighting.sh_fill_c);

    // Directional light
    let light_dir = normalize(lighting.dir_light_vec.xyz);
    let n_dot_l = max(dot(world_normal, light_dir), 0.0);
    var dir_diffuse = lighting.dir_light_color.rgb * n_dot_l;

    // === Directional shadow (VSM) ===
    var shadow_factor = 1.0;
    if lighting.shadow_params.z > 0.5 {
        // Transform world position to shadow space
        let shadow_vp = mat4x4<f32>(
            lighting.shadow_vp_col0,
            lighting.shadow_vp_col1,
            lighting.shadow_vp_col2,
            lighting.shadow_vp_col3,
        );
        let shadow_pos = shadow_vp * vec4<f32>(in.world_pos, 1.0);
        let shadow_ndc = shadow_pos.xyz / shadow_pos.w;
        shadow_factor = compute_dir_shadow(
            shadow_ndc,
            lighting.dir_light_color.a,
            lighting.shadow_params.x,
            lighting.shadow_params.y,
            t_shadow_map,
            s_lighting,
        );
    }
    dir_diffuse *= shadow_factor;

    let view_dir = normalize(lighting.world_camera_pos.xyz - in.world_pos);
    let world_reflect = reflect(-view_dir, world_normal);
    let local_spec_power = lighting.local_light_params.y;
    let directional_reflection = max(dot(world_reflect, light_dir), 0.0);
    let directional_specular = dir_diffuse
        * specular_schlick(directional_reflection, local_spec_power);

    let fill_direction = normalize(lighting.fill_spec_direction_power.xyz);
    let fill_reflection = max(dot(world_reflect, fill_direction), 0.0);
    let fill_shadow = 1.0
        + lighting.fill_spec_color_shadow.w * (shadow_factor - 1.0);
    let fill_specular = lighting.fill_spec_color_shadow.rgb
        * specular_schlick(fill_reflection, lighting.fill_spec_direction_power.w)
        * fill_shadow;

    // === Local/point lights ===
    var local_diffuse = vec3<f32>(0.0);
    var local_specular = vec3<f32>(0.0);
    let num_lights = select(0u, u32(lighting.local_light_params.x), lighting.local_light_params.w > 0.5);

    for (var i = 0u; i < num_lights && i < 20u; i++) {
        let base = i * 8u;
        let result = evaluate_spot_specular_shadowed(
            local_lights[base],
            local_lights[base + 1u],
            local_lights[base + 2u],
            local_lights[base + 3u],
            local_lights[base + 4u],
            local_lights[base + 5u],
            local_lights[base + 6u],
            local_lights[base + 7u],
            in.world_pos,
            world_normal,
            world_reflect,
            local_spec_power,
            lighting.local_light_params.z > 0.5,
            t_local_shadow_map,
            s_lighting,
        );
        local_diffuse += result.diffuse;
        local_specular += result.specular;
    }

    if lighting.light_volume_params.x > 0.5 {
        let volume = evaluate_light_volume(
            in.world_pos,
            world_normal,
            world_reflect,
            local_spec_power,
            lighting.light_volume_row0,
            lighting.light_volume_row1,
            lighting.light_volume_row2,
            t_light_volume_color,
            t_light_volume_vector,
            s_lighting,
        );
        local_diffuse += volume.diffuse;
        local_specular += volume.specular;
    }

    // === Combine lighting (HWDE PS lines 493-507) ===
    // HWDE formula:
    //   light = dir_diffuse * shadow + lightmap * 2.0
    //   ao_blend = mix(light, light * ao, ao_intensity)  // cb4[36].w
    //   final = (sh_ambient * ao + ao_blend) * albedo + specular
    var dir_light = dir_diffuse + in.light_color * 2.0 + local_diffuse;
    let ao_blended = mix(dir_light, dir_light * ao, ao_intensity);
    let ambient_sum = sh_ambient * ao;
    var lit_color = (ambient_sum + ao_blended) * color
        + specular_map * (directional_specular + fill_specular + local_specular) * 3.14;

    // === Blackmap (fog-of-war) ===
    // HWDE uses world-position-derived UVs with separate X/Z scales (cb4[33-34])
    if lighting.blackmap_params1.w > 0.5 {
        let visibility_uv = vec2<f32>(
            in.world_pos.x * lighting.blackmap_uv_scales.x,
            -in.world_pos.z * lighting.blackmap_uv_scales.x + 1.0,
        );
        let unexplored_uv = in.world_pos.xz * lighting.blackmap_uv_scales.y;
        let vis_factor = textureSample(t_blackmap, s_terrain, visibility_uv).a;
        let unexplored_sample = textureSample(t_unexplored, s_terrain, unexplored_uv);
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
    if params.debug_mode > 12.5 && params.debug_mode < 13.5 {
        // Debug mode 13: Chunk grid visualization with corner markers
        // Corner markers (2x2 chunk areas):
        // - (0,0) corner: BRIGHT RED (world origin)
        // - (15,0) corner: BRIGHT BLUE (+X direction)
        // - (0,15) corner: BRIGHT GREEN (+Z direction)
        // - (15,15) corner: BRIGHT YELLOW (diagonal)
        // Rest: gradient R=grid_x/16, G=grid_z/16
        if grid_x < 2u && grid_z < 2u {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0); // RED = origin (0,0)
        }
        if grid_x > 13u && grid_z < 2u {
            return vec4<f32>(0.0, 0.0, 1.0, 1.0); // BLUE = +X direction (15,0)
        }
        if grid_x < 2u && grid_z > 13u {
            return vec4<f32>(0.0, 1.0, 0.0, 1.0); // GREEN = +Z direction (0,15)
        }
        if grid_x > 13u && grid_z > 13u {
            return vec4<f32>(1.0, 1.0, 0.0, 1.0); // YELLOW = diagonal (15,15)
        }
        return vec4<f32>(f32(grid_x) / 16.0, f32(grid_z) / 16.0, 0.0, 1.0);
    }
    if params.debug_mode > 11.5 && params.debug_mode < 12.5 {
        // Debug mode 12: GPU-composited atlas output
        // Sample from the 8K×8K GPU-composited atlas
        let gpu_composited_color = textureSample(t_gpu_composited, s_terrain, sample_uv);
        return vec4<f32>(gpu_composited_color.rgb, 1.0);
    }
    if params.debug_mode > 10.5 && params.debug_mode < 11.5 {
        // Debug mode 11: terrain UV visualization
        return vec4<f32>(sample_uv.x, sample_uv.y, 0.0, 1.0);
    }
    if params.debug_mode > 9.5 && params.debug_mode < 10.5 {
        return vec4<f32>(0.5, 0.7, 0.3, 1.0);  // Solid green
    }
    if params.debug_mode > 7.5 && params.debug_mode < 8.5 {
        return vec4<f32>(ao, ao, ao, 1.0);
    }
    if params.debug_mode > 8.5 && params.debug_mode < 9.5 {
        return vec4<f32>(specular_map, 1.0);
    }
    if params.debug_mode > 6.5 && params.debug_mode < 7.5 {
        return vec4<f32>(unique_xy * 0.5 + 0.5, 1.0, 1.0);
    }
    if params.debug_mode > 0.5 && params.debug_mode < 1.5 {
        let edge_threshold = 0.02;
        let is_edge = in_chunk_uv.x < edge_threshold || in_chunk_uv.x > (1.0 - edge_threshold) ||
                      in_chunk_uv.y < edge_threshold || in_chunk_uv.y > (1.0 - edge_threshold);
        if is_edge {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    }

    return vec4<f32>(final_color, combined_alpha);
}
