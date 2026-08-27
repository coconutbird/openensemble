import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::fog::compute_radial_fog_density;
import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::shadows::compute_dir_shadow;
import package::shared::blackmap::apply_blackmap;
import package::shared::local_lights::evaluate_spot_diffuse;

// Foliage shader - WGSL implementation of terrainFoliage.fx
//
// Renders grass/foliage blades as instanced geometry that conforms to terrain.
// Each blade is fetched from a geometry texture, randomly rotated and scaled,
// then positioned on the terrain surface.
//
// Lighting pipeline (matching original):
// - Directional diffuse (clamped by backside shadow scalar)
// - SH ambient fill lighting (2nd-order spherical harmonics)
// - Two-sided lighting (normal flipped toward camera)
// - Fog (radial + planar)
// - Distance-based alpha fade

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct FoliageParams {
    // Terrain info: num_verts_per_axis, tile_scale, chunk_offset_x, chunk_offset_z
    terrain_info: vec4<f32>,
    // Position compression constants used by the terrain position texture.
    position_mid: vec3<f32>,
    _pad0: f32,
    position_range: vec3<f32>,
    _pad1: f32,
    // Foliage params: num_verts_per_blade, rcp_num_blades, fade_start, fade_end
    foliage_info: vec4<f32>,
    // Camera position for distance fade and two-sided lighting
    camera_pos: vec3<f32>,
    time: f32,
    // Directional light
    dir_light_vec: vec3<f32>,
    _dir_light_pad: f32,
    dir_light_color: vec3<f32>,
    dir_shadow_darkness: f32,
    // Fog
    fog_params: vec4<f32>,       // x = density^2, y = start^2, z/w unused
    fog_color: vec4<f32>,
    planar_fog_params: vec4<f32>, // x = enabled, y = start_y, z = density^2
    planar_fog_color: vec4<f32>,
    // SH fill lighting coefficients
    sh_fill_ar: vec4<f32>,
    sh_fill_ag: vec4<f32>,
    sh_fill_ab: vec4<f32>,
    sh_fill_br: vec4<f32>,
    sh_fill_bg: vec4<f32>,
    sh_fill_bb: vec4<f32>,
    sh_fill_c: vec4<f32>,
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
    local_light_params: vec4<f32>,  // x = number of local lights
    blackmap_uv_scales: vec4<f32>,  // x = visibility scale, y = unexplored scale
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Per-chunk data (dynamic uniform buffer — offset changes per draw call)
struct ChunkInfo {
    chunk_offset: vec2<f32>,   // mMinXVert, mMinZVert from quad node
    num_verts_per_blade: f32,
    blade_data_offset: f32,    // offset into blade map texture for this draw call
};

@group(1) @binding(0)
var<uniform> params: FoliageParams;

@group(1) @binding(1)
var<uniform> chunk_info: ChunkInfo;

// Terrain heightmap for positioning blades
@group(1) @binding(2)
var t_heightmap: texture_2d<f32>;

@group(1) @binding(3)
var s_heightmap: sampler;

// Shadow and blackmap textures
@group(1) @binding(4)
var t_shadow_map: texture_2d_array<f32>;
@group(1) @binding(5)
var t_blackmap: texture_2d<f32>;
@group(1) @binding(6)
var t_unexplored: texture_2d<f32>;

// Blade map texture: each texel is (grid_position, blade_type) as u32 pair.
// Indexed by blade_data_offset + instance_index.
@group(1) @binding(7)
var t_blade_map: texture_2d<u32>;

@group(1) @binding(8)
var<storage, read> local_lights: array<vec4<f32>>;

// Per-set material textures + blade geometry
@group(2) @binding(0)
var t_albedo: texture_2d<f32>;

@group(2) @binding(1)
var t_opacity: texture_2d<f32>;

@group(2) @binding(2)
var s_foliage: sampler;

// Blade geometry textures (position.xyz + u, normal.xyz + v) — per set
@group(2) @binding(3)
var t_blade_positions: texture_2d<f32>;

@group(2) @binding(4)
var t_blade_normals: texture_2d<f32>;

@group(2) @binding(5)
var s_blade: sampler;

struct FoliageMaterialParams {
    values: vec4<f32>,
};

@group(2) @binding(6)
var<uniform> material: FoliageMaterialParams;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) fog_densities: vec2<f32>,  // x = radial, y = planar
    @location(3) alpha_fade: f32,
    @location(4) world_pos: vec3<f32>,
};

// Exact scalar sequence from terrainFoliage VS #0/#2.
fn foliage_random(blade_index: f32) -> f32 {
    let seed = fract(blade_index * 0.0012385598) * 257.0 + 1.0;
    return fract(seed * seed);
}

fn rotate_2d(input: vec2<f32>, theta: f32) -> vec2<f32> {
    let sintheta = sin(theta);
    let costheta = cos(theta);
    return vec2<f32>(
        input.x * costheta + input.y * sintheta,
        input.y * costheta - input.x * sintheta
    );
}

// Sample terrain displacement at given UV position.
// Game samples full XYZ displacement from position texture (getTerrainDataAtPos).
fn get_terrain_offset(uv: vec2<f32>) -> vec3<f32> {
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    // terrainFoliage VS #0/#2 samples the position texture at .yx.
    let sample = textureSampleLevel(t_heightmap, s_heightmap, clamped_uv.yx, 0.0);
    // The oracle consumes R10G10B10A2_UNORM as .zyx, and foliage deliberately
    // does not subtract the terrain-only g_yOffset.
    return sample.bgr * params.position_range - params.position_mid;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts_per_blade = u32(chunk_info.num_verts_per_blade);
    let num_blades_per_node: u32 = 64u;

    let vertex_in_blade = in.vertex_index % num_verts_per_blade;

    // Chunk offset from QN chunk data (mMinXVert, mMinZVert), passed via dynamic uniform
    let chunk_offset_x = chunk_info.chunk_offset.x;
    let chunk_offset_z = chunk_info.chunk_offset.y;

    // Look up blade placement from blade map texture.
    // Each instance maps to a (grid_position, blade_type) entry parsed from the
    // original index buffer. blade_data_offset locates this draw call's entries.
    let blade_map_idx = u32(chunk_info.blade_data_offset) + in.instance_index;
    let blade_map_width = textureDimensions(t_blade_map, 0).x;
    let blade_map_coord = vec2<u32>(blade_map_idx % blade_map_width, blade_map_idx / blade_map_width);
    let blade_info = textureLoad(t_blade_map, blade_map_coord, 0);
    let blade_index = blade_info.r;  // grid position (0..4095) = localIndex from export
    let blade_type = blade_info.g;   // blade type from index buffer

    // Fetch blade vertex from geometry texture using the correct blade type
    let pixel_x = vertex_in_blade + blade_type * num_verts_per_blade;
    let pos_sample = textureLoad(t_blade_positions, vec2<u32>(pixel_x, 0u), 0);
    let norm_sample = textureLoad(t_blade_normals, vec2<u32>(pixel_x, 0u), 0);

    // Blade geometry and UV are passed through exactly as loaded by the oracle.
    var pos = pos_sample.xyz;
    let blade_uv = vec2<f32>(pos_sample.w, norm_sample.w);
    var normal = norm_sample.xyz;

    // The oracle derives one scalar from the local 0..4095 blade index and reuses
    // it for both jitter axes, rotation, and blade-height variation.
    let rnd = foliage_random(f32(blade_index));

    // Fresh PC VS #0/#2: local source X is bladeIndex >> 6 and local source Z
    // is bladeIndex & 63. QN parent metadata has already selected the world
    // chunk, so its origin must not receive the packed-atlas transpose.
    var source_local_x = f32(blade_index / num_blades_per_node) + 0.5;
    var source_local_z = f32(blade_index % num_blades_per_node);

    // Jitter
    source_local_x += (rnd * 2.0 - 1.0) * 0.9;
    source_local_z += (rnd * 2.0 - 1.0) * 0.9;

    // The packed local grid uses the opposite X/Z order from the viewer. Keep
    // the parent origin fixed and transpose only the local placement.
    let trns_x = chunk_offset_x + source_local_z;
    let trns_z = chunk_offset_z + source_local_x;

    // Rotate blade around Y axis
    let rot_angle = rnd * 360.0;
    let pos_rot = rotate_2d(pos.xz, rot_angle);
    let norm_rot = rotate_2d(normal.xz, rot_angle);
    normal.x = norm_rot.y;
    normal.z = norm_rot.x;
    normal = normalize(normal);

    pos.x = pos_rot.y;
    pos.z = pos_rot.x;
    let tile_scale = params.terrain_info.y;
    pos.x += trns_x * tile_scale;
    pos.z += trns_z * tile_scale;
    pos.y *= max(0.25, rnd);

    // Terrain heightmap UV
    // Fresh PC VS #0/#2: getTerrainDataAtPos(trns * g_terrainVals.x), where .x = 1/numVerts.
    let num_verts = params.terrain_info.x;
    let terrain_uv = vec2<f32>(trns_x, trns_z) / num_verts;
    let terrain_offset = get_terrain_offset(terrain_uv);
    pos += terrain_offset;

    // Fog densities
    let z_fog = compute_radial_fog_density(pos, params.camera_pos, params.fog_params.x, params.fog_params.y);
    var p_fog = 1.0;
    if params.planar_fog_params.x > 0.5 {
        p_fog = compute_planar_fog_density(pos, params.camera_pos, params.planar_fog_params.y, params.planar_fog_params.z);
    }

    // Distance-based alpha fade
    let dist_to_camera = length(params.camera_pos - pos);
    let fade_start = params.foliage_info.z;
    let fade_end = params.foliage_info.w;
    let alpha_fade = 1.0 - clamp((dist_to_camera - fade_start) / (fade_end - fade_start), 0.0, 1.0);

    // Output
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.uv = blade_uv;
    out.normal = normal;
    out.fog_densities = vec2<f32>(z_fog, p_fog);
    out.alpha_fade = alpha_fade;
    out.world_pos = pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // DEBUG: visualize UVs as color (uncomment to debug)
    // return vec4<f32>(in.uv.x, in.uv.y, 0.0, 1.0);

    // Sample textures
    let albedo = textureSample(t_albedo, s_foliage, in.uv);
    let opacity = textureSample(t_opacity, s_foliage, in.uv).r;

    // Alpha test (game uses 0.6666 threshold — confirmed in bytecode PS)
    let final_alpha = opacity * in.alpha_fade;
    if final_alpha < 0.6666 {
        discard;
    }

    // Pixel-lit terrainFoliage shader #3. Normal flipping, directional light,
    // SH, local spots, and the shadow are deliberately evaluated together in
    // this stage; the old implementation mixed vertex-lit #0 with pixel-lit #3.
    var world_normal = normalize(in.normal);
    let to_camera = normalize(params.camera_pos - in.world_pos);
    if dot(to_camera, world_normal) < 0.0 {
        world_normal = -world_normal;
    }
    let n_dot_l = max(dot(world_normal, params.dir_light_vec), 0.0);
    var direct_lighting = params.dir_light_color * n_dot_l;

    var local_diffuse = vec3<f32>(0.0);
    let light_count = select(
        0u,
        min(u32(params.local_light_params.x), 20u),
        params.local_light_params.w > 0.5,
    );
    for (var light_index = 0u; light_index < light_count; light_index++) {
        let base = light_index * 8u;
        local_diffuse += evaluate_spot_diffuse(
            local_lights[base].xyz,
            local_lights[base + 1u].xyz,
            local_lights[base].w,
            local_lights[base + 1u].w,
            local_lights[base + 2u].x,
            local_lights[base + 2u].y,
            local_lights[base + 2u].z,
            local_lights[base + 3u].xyz,
            in.world_pos,
            world_normal,
        );
    }
    direct_lighting += local_diffuse;
    direct_lighting = clamp(
        direct_lighting,
        vec3<f32>(1.0 - material.values.x),
        vec3<f32>(1.0),
    );

    // === Shadows (VSM) — only when nDotL > 0, matching game ===
    if params.shadow_params.z > 0.5 && n_dot_l > 0.0 {
        let shadow_vp = mat4x4<f32>(
            params.shadow_vp_col0,
            params.shadow_vp_col1,
            params.shadow_vp_col2,
            params.shadow_vp_col3,
        );
        let shadow_pos = shadow_vp * vec4<f32>(in.world_pos, 1.0);
        let shadow_ndc = shadow_pos.xyz / shadow_pos.w;
        let shadow_factor = compute_dir_shadow(
            shadow_ndc,
            params.dir_shadow_darkness,
            params.shadow_params.x,
            params.shadow_params.y,
            t_shadow_map,
            s_foliage,
        );
        direct_lighting *= shadow_factor;
    }

    let ambient = compute_sh_fill_lighting(
        world_normal,
        params.sh_fill_ar, params.sh_fill_ag, params.sh_fill_ab,
        params.sh_fill_br, params.sh_fill_bg, params.sh_fill_bb,
        params.sh_fill_c,
    );
    let diffuse = direct_lighting + ambient;
    var lit_color = diffuse * albedo.rgb;

    // === Blackmap (fog-of-war) ===
    if params.blackmap_params1.w > 0.5 {
        let visibility_uv = vec2<f32>(
            in.world_pos.x * params.blackmap_uv_scales.x,
            -in.world_pos.z * params.blackmap_uv_scales.x + 1.0,
        );
        let unexplored_uv = in.world_pos.xz * params.blackmap_uv_scales.y;
        let vis_factor = textureSample(t_blackmap, s_foliage, visibility_uv).a;
        let unexplored_sample = textureSample(t_unexplored, s_foliage, unexplored_uv);
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

    // Apply fog
    lit_color = compute_fog(lit_color, in.fog_densities.x, in.fog_densities.y, params.fog_color.xyz, params.planar_fog_color.xyz);

    // Game outputs opacity * alphaDistScalar as alpha, uses alpha blending
    // The discard at 0.6666 culls fully transparent pixels; remaining are blended
    return vec4<f32>(lit_color, final_alpha);
}

// terrainFoliage shader #7: alpha-tested VSM caster with the derivative depth
// bias used by the original shader.
@fragment
fn fs_shadow(in: VertexOutput) -> @location(0) vec2<f32> {
    let opacity = textureSample(t_opacity, s_foliage, in.uv).r * in.alpha_fade;
    if opacity < 0.5 {
        discard;
    }
    let slope = clamp(max(dpdyCoarse(in.clip_position.z), dpdxCoarse(in.clip_position.z)), 0.0, 0.004);
    return vec2<f32>(in.clip_position.z + slope * 1.2 + 0.0005, 0.0);
}
