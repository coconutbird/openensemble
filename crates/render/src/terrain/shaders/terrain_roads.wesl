import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::fog::compute_radial_fog_density;
import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::normal::compute_tbn;
import package::shared::normal::unpack_dxn_normal;
import package::shared::shadows::vsm_shadow;
import package::shared::blackmap::apply_blackmap;
import package::shared::local_lights::evaluate_omni_specular;

// Road shader - WGSL implementation of terrainRoads.fx
//
// Renders pre-tessellated road geometry that conforms to terrain.
// Roads are strips of triangles with per-vertex position and UV,
// snapped to terrain height with TBN derived from terrain normals.
//
// Features from original HLSL:
//   - Terrain position/normal sampling for height conforming
//   - TBN from terrain normal (GiveTBNFromNormal)
//   - Full lit pipeline: directional + SH ambient
//   - Normal mapping (DXN/BC5 encoded)
//   - Specular (Blinn-Phong)
//   - Fog (radial + planar)

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct RoadParams {
    // Terrain vals: x=numXVerts, y=scale (world size), z=unused, w=unused
    terrain_vals: vec4<f32>,
    // Position decompression: min.xyz
    pos_comp_min: vec4<f32>,
    // Position decompression: range.xyz
    pos_comp_range: vec4<f32>,
    // Camera position
    camera_pos: vec4<f32>,
    // Directional light
    dir_light_vec: vec4<f32>,
    dir_light_color: vec4<f32>,
    // SH fill lighting
    sh_fill_ar: vec4<f32>,
    sh_fill_ag: vec4<f32>,
    sh_fill_ab: vec4<f32>,
    sh_fill_br: vec4<f32>,
    sh_fill_bg: vec4<f32>,
    sh_fill_bb: vec4<f32>,
    sh_fill_c: vec4<f32>,
    // Fog
    fog_color: vec4<f32>,
    fog_params: vec4<f32>,
    planar_fog_color: vec4<f32>,
    planar_fog_params: vec4<f32>,
    // x=spec_power
    spec_power: vec4<f32>,
    // Shadow params
    shadow_vp_col0: vec4<f32>,
    shadow_vp_col1: vec4<f32>,
    shadow_vp_col2: vec4<f32>,
    shadow_vp_col3: vec4<f32>,
    shadow_params: vec4<f32>,       // x = csm_scale, y = num_passes, z = enabled
    // Blackmap params
    blackmap_params0: vec4<f32>,    // rgb = bg_color, w = fog_scalar
    blackmap_params1: vec4<f32>,    // x = unexplored_scalar, yz = bounds_lo_xz, w = enabled
    blackmap_params2: vec4<f32>,    // x = pad, yz = bounds_hi_xz, w = bounds_falloff
    // Local light params
    local_light_params: vec4<f32>,  // x = num_lights, y = spec_power
};
@group(1) @binding(0)
var<uniform> params: RoadParams;

// Terrain position texture (packed XYZ sampled as float)
@group(1) @binding(1)
var t_terrain_pos: texture_2d<f32>;
@group(1) @binding(2)
var s_terrain: sampler;

// Terrain basis/normal texture (packed normal)
@group(1) @binding(3)
var t_terrain_basis: texture_2d<f32>;

// Shadow, blackmap, and local light resources
@group(1) @binding(4)
var t_shadow_map: texture_2d<f32>;
@group(1) @binding(5)
var t_blackmap: texture_2d<f32>;
@group(1) @binding(6)
var t_unexplored: texture_2d<f32>;
@group(1) @binding(7)
var<storage, read> local_lights: array<vec4<f32>>;

// Road textures
@group(2) @binding(0)
var t_road_albedo: texture_2d<f32>;
@group(2) @binding(1)
var s_road: sampler;
@group(2) @binding(2)
var t_road_normal: texture_2d<f32>;
@group(2) @binding(3)
var t_road_specular: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) binormal: vec3<f32>,
    @location(5) fog_densities: vec2<f32>,
};

// --- Vertex shader ---
// Roads sample terrain position/normal textures to conform to terrain surface.
// Original: getTerrainDataAtPos() in terrainRoads.fx

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Convert world XZ to terrain UV for sampling position/normal textures
    // Original: uvVal = (gPos.zx / g_terrainVals.y) * g_terrainVals.x
    let terrain_uv = vec2<f32>(
        (in.position.z / params.terrain_vals.y) * params.terrain_vals.x,
        (in.position.x / params.terrain_vals.y) * params.terrain_vals.x
    );

    // Sample terrain position texture to get Y offset
    let pos_sample = textureSampleLevel(t_terrain_pos, s_terrain, terrain_uv, 0.0).xyz;
    let offset = pos_sample * params.pos_comp_range.xyz - params.pos_comp_min.xyz;

    // Road world position: keep input XZ, use terrain Y + small bias
    let world_pos = vec3<f32>(in.position.x, offset.y + 0.01, in.position.z);

    // Sample terrain normal for TBN
    let norm_sample = textureSampleLevel(t_terrain_basis, s_terrain, terrain_uv, 0.0).xyz;
    let terrain_normal = normalize(norm_sample * 2.0 - 1.0);
    let tbn = compute_tbn(terrain_normal);

    // Fog
    let z_fog = compute_radial_fog_density(world_pos, params.camera_pos.xyz, params.fog_params.x, params.fog_params.y);
    var p_fog = 1.0;
    if params.planar_fog_params.x > 0.5 {
        p_fog = compute_planar_fog_density(world_pos.y, params.planar_fog_params.y, params.planar_fog_params.z);
    }

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = in.uv;
    out.world_pos = world_pos;
    out.normal = tbn[2];
    out.tangent = tbn[0];
    out.binormal = tbn[1];
    out.fog_densities = vec2<f32>(z_fog, p_fog);

    return out;
}

// --- Fragment shader ---
// Full lit road rendering with albedo, normal map, specular.
// Matches mypsMain_ANS() in terrainRoads.fx

const SPEC_TO_DIFFUSE_RATIO: f32 = 3.14159265;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_road_albedo, s_road, in.uv);
    let tex_normal = unpack_dxn_normal(textureSample(t_road_normal, s_road, in.uv));
    let specular = textureSample(t_road_specular, s_road, in.uv);

    // Transform tangent-space normal to world space
    let world_normal = normalize(
        tex_normal.x * in.tangent +
        tex_normal.y * in.binormal +
        tex_normal.z * in.normal
    );

    // Directional lighting
    let n_dot_l = max(dot(world_normal, params.dir_light_vec.xyz), 0.0);
    var diffuse_sum = params.dir_light_color.rgb * n_dot_l;

    // Specular (Blinn-Phong)
    let view_dir = normalize(params.camera_pos.xyz - in.world_pos);
    let half_vec = normalize(params.dir_light_vec.xyz + view_dir);
    let n_dot_h = max(dot(world_normal, half_vec), 0.0);
    var spec_sum = params.dir_light_color.rgb * pow(n_dot_h, params.spec_power.x);

    // === Shadows (VSM) ===
    var shadow_factor = 1.0;
    if params.shadow_params.z > 0.5 {
        let shadow_vp = mat4x4<f32>(
            params.shadow_vp_col0,
            params.shadow_vp_col1,
            params.shadow_vp_col2,
            params.shadow_vp_col3,
        );
        let shadow_pos = shadow_vp * vec4<f32>(in.world_pos, 1.0);
        let shadow_ndc = shadow_pos.xyz / shadow_pos.w;
        let shadow_uv = vec2<f32>(shadow_ndc.x * 0.5 + 0.5, shadow_ndc.y * -0.5 + 0.5);
        let shadow_z = clamp(shadow_ndc.z, 0.0, 1.0);
        shadow_factor = vsm_shadow(shadow_uv, shadow_z, params.dir_light_color.a, t_shadow_map, s_road);
    }
    diffuse_sum *= shadow_factor;
    spec_sum *= shadow_factor;

    // SH ambient
    let ambient_sum = compute_sh_fill_lighting(world_normal, params.sh_fill_ar, params.sh_fill_ag, params.sh_fill_ab, params.sh_fill_br, params.sh_fill_bg, params.sh_fill_bb, params.sh_fill_c);

    // === Local lights ===
    var local_diffuse = vec3<f32>(0.0);
    var local_spec = 0.0;
    let num_lights = u32(params.local_light_params.x);
    let local_spec_power = params.local_light_params.y;
    let world_reflect = reflect(-view_dir, world_normal);
    for (var i = 0u; i < num_lights; i++) {
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

    // Combine
    diffuse_sum += ambient_sum + local_diffuse;
    var lit_color = diffuse_sum * albedo.rgb;
    lit_color += (spec_sum * specular.rgb + vec3<f32>(local_spec)) * SPEC_TO_DIFFUSE_RATIO;

    // === Blackmap (fog-of-war) ===
    if params.blackmap_params1.w > 0.5 {
        let bm_uv = in.uv;
        let vis_factor = textureSample(t_blackmap, s_road, bm_uv).a;
        let unexplored_sample = textureSample(t_unexplored, s_road, bm_uv);
        lit_color = apply_blackmap(
            lit_color, in.world_pos, vis_factor, unexplored_sample,
            params.blackmap_params0.xyz,
            params.blackmap_params0.w,
            params.blackmap_params1.x,
            params.blackmap_params1.yz,
            params.blackmap_params2.yz,
            params.blackmap_params2.w,
        );
    }

    // Fog
    lit_color = compute_fog(lit_color, in.fog_densities.x, in.fog_densities.y, params.fog_color.xyz, params.planar_fog_color.xyz);

    return vec4<f32>(lit_color, 1.0);
}
