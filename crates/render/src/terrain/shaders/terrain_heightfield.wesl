import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::fog::compute_radial_fog_density;
import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::shadows::compute_dir_shadow;
import package::shared::blackmap::apply_blackmap;
import package::shared::local_lights::evaluate_light_volume;
import package::shared::local_lights::evaluate_spot_specular_shadowed;
import package::shared::specular::specular_schlick;

// Terrain-conforming patch translation of terrainheightfield shaders #5-#10.
// The PC pipeline fed one five-float4 control point into a quad hull/domain
// shader. wgpu has no tessellation stages, so the renderer supplies the same
// 16x16 domain as a static indexed carrier mesh. Instance locations 1-5 retain
// the original POSITION/TEXCOORD0/TEXCOORD1/TEXCOORD2/COLOR packing exactly.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct PatchParams {
    // x = texture dimension, y = inverse tile scale, z = Y range, w = Y midpoint
    terrain_info: vec4<f32>,
    // x = normalized Y bias, yz = world X/Z origin, w = conform switch
    terrain_origin: vec4<f32>,
    // x = reciprocal-specular power, y = terrain-alpha switch
    material_params: vec4<f32>,
};

struct LightingParams {
    dir_light_vec: vec4<f32>,
    dir_light_color: vec4<f32>,
    world_camera_pos: vec4<f32>,
    sh_fill_ar: vec4<f32>,
    sh_fill_ag: vec4<f32>,
    sh_fill_ab: vec4<f32>,
    sh_fill_br: vec4<f32>,
    sh_fill_bg: vec4<f32>,
    sh_fill_bb: vec4<f32>,
    sh_fill_c: vec4<f32>,
    fog_color: vec4<f32>,
    fog_params: vec4<f32>,
    planar_fog_color: vec4<f32>,
    planar_fog_params: vec4<f32>,
    ao_params: vec4<f32>,
    shadow_vp_col0: vec4<f32>,
    shadow_vp_col1: vec4<f32>,
    shadow_vp_col2: vec4<f32>,
    shadow_vp_col3: vec4<f32>,
    shadow_params: vec4<f32>,
    blackmap_params0: vec4<f32>,
    blackmap_params1: vec4<f32>,
    blackmap_params2: vec4<f32>,
    local_light_params: vec4<f32>,
    fadeout_params: vec4<f32>,
    blackmap_uv_scales: vec4<f32>,
    fill_spec_direction_power: vec4<f32>,
    fill_spec_color_shadow: vec4<f32>,
    light_volume_params: vec4<f32>,
    light_volume_row0: vec4<f32>,
    light_volume_row1: vec4<f32>,
    light_volume_row2: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;
@group(0) @binding(1)
var<uniform> patch_params: PatchParams;
@group(0) @binding(2)
var<uniform> lighting: LightingParams;
@group(0) @binding(3)
var t_heightfield: texture_2d<f32>;
@group(0) @binding(4)
var s_heightfield: sampler;
@group(0) @binding(5)
var t_terrain_alpha: texture_2d<u32>;
@group(0) @binding(6)
var t_shadow_map: texture_2d_array<f32>;
@group(0) @binding(7)
var t_blackmap: texture_2d<f32>;
@group(0) @binding(8)
var t_unexplored: texture_2d<f32>;
@group(0) @binding(9)
var<storage, read> local_lights: array<vec4<f32>>;
@group(0) @binding(10)
var s_lighting: sampler;
@group(0) @binding(11)
var t_local_shadow_map: texture_depth_2d_array;
@group(0) @binding(12)
var t_light_volume_color: texture_3d<f32>;
@group(0) @binding(13)
var t_light_volume_vector: texture_3d<f32>;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var t_normal: texture_2d<f32>;
@group(1) @binding(2)
var t_opacity: texture_2d<f32>;
@group(1) @binding(3)
var t_specular: texture_2d<f32>;
@group(1) @binding(4)
var s_material: sampler;

struct VertexInput {
    @location(0) domain: vec2<f32>,
    @location(1) position: vec4<f32>,
    @location(2) axis_v_and_axis_u_x: vec4<f32>,
    @location(3) axis_u_yz_offset_intensity: vec4<f32>,
    @location(4) tex_uv: vec4<f32>,
    @location(5) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tangent: vec3<f32>,
    @location(1) binormal: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) domain: vec2<f32>,
    @location(4) tex_uv: vec4<f32>,
    @location(5) world_pos: vec3<f32>,
    @location(6) fog_densities: vec2<f32>,
    @location(7) color: vec4<f32>,
};

fn heightfield_uv(world_pos: vec3<f32>) -> vec2<f32> {
    let grid = vec2<f32>(
        (world_pos.z - patch_params.terrain_origin.z) * patch_params.terrain_info.y,
        (world_pos.x - patch_params.terrain_origin.y) * patch_params.terrain_info.y,
    );
    return grid / patch_params.terrain_info.x;
}

fn dynamic_alpha_uv(world_pos: vec3<f32>) -> vec2<f32> {
    let grid = vec2<f32>(
        (world_pos.x - patch_params.terrain_origin.y) * patch_params.terrain_info.y,
        (world_pos.z - patch_params.terrain_origin.z) * patch_params.terrain_info.y,
    );
    return grid / patch_params.terrain_info.x;
}

fn dynamic_alpha_enabled(uv: vec2<f32>) -> bool {
    let dimensions = textureDimensions(t_terrain_alpha);
    let maximum_pixel = dimensions.y - 1u;
    let pixel = vec2<u32>(
        min(u32(clamp(uv.x, 0.0, 1.0) * f32(maximum_pixel)), maximum_pixel),
        min(u32(clamp(uv.y, 0.0, 1.0) * f32(maximum_pixel)), maximum_pixel),
    );
    let word_x = min(pixel.x >> 5u, dimensions.x - 1u);
    let word = textureLoad(t_terrain_alpha, vec2<u32>(word_x, pixel.y), 0).r;
    return (word & (1u << (pixel.x & 31u))) != 0u;
}

fn terrain_height(world_pos: vec3<f32>) -> f32 {
    let normalized_y = textureSampleLevel(
        t_heightfield,
        s_heightfield,
        heightfield_uv(world_pos),
        0.0,
    ).g;
    return (normalized_y - patch_params.terrain_origin.x)
        * patch_params.terrain_info.z
        - patch_params.terrain_info.w;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Decompiled domain shader #9:
    //   axis U = (TEXCOORD0.w, TEXCOORD1.x, TEXCOORD1.y)
    //   axis V = TEXCOORD0.xyz
    // Both axes use (1 - 2 * domain), including the original orientation.
    let axis_u = vec3<f32>(
        in.axis_v_and_axis_u_x.w,
        in.axis_u_yz_offset_intensity.x,
        in.axis_u_yz_offset_intensity.y,
    );
    let axis_v = in.axis_v_and_axis_u_x.xyz;
    var world_pos = in.position.xyz
        + axis_u * (1.0 - 2.0 * in.domain.x)
        + axis_v * (1.0 - 2.0 * in.domain.y);

    var terrain_alpha = 1.0;
    if patch_params.terrain_origin.w > 0.5 {
        world_pos.y = terrain_height(world_pos);
        if patch_params.material_params.y > 0.5 {
            terrain_alpha = select(
                0.0,
                1.0,
                dynamic_alpha_enabled(dynamic_alpha_uv(world_pos)),
            );
        }
    }
    world_pos.y += in.axis_u_yz_offset_intensity.z;

    let radial_fog = compute_radial_fog_density(
        world_pos,
        lighting.world_camera_pos.xyz,
        lighting.fog_params.x,
        lighting.fog_params.y,
    );
    var planar_fog = 1.0;
    if lighting.planar_fog_params.x > 0.5 {
        planar_fog = compute_planar_fog_density(
            world_pos,
            lighting.world_camera_pos.xyz,
            lighting.planar_fog_params.y,
            lighting.planar_fog_params.z,
        );
    }

    let saturated_color = clamp(in.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.tangent = vec3<f32>(1.0, 0.0, 0.0);
    out.binormal = vec3<f32>(0.0, 0.0, -1.0);
    out.normal = vec3<f32>(0.0, 1.0, 0.0);
    out.domain = in.domain;
    out.tex_uv = in.tex_uv;
    out.world_pos = world_pos;
    out.fog_densities = vec2<f32>(radial_fog, planar_fog);
    out.color = vec4<f32>(
        saturated_color * saturated_color * in.axis_u_yz_offset_intensity.w,
        terrain_alpha * in.color.a,
    );
    return out;
}

fn material_uv(in: VertexOutput) -> vec2<f32> {
    return in.domain * in.tex_uv.zw + in.tex_uv.xy;
}

@fragment
fn fs_unlit(in: VertexOutput) -> @location(0) vec4<f32> {
    let diffuse = textureSample(t_diffuse, s_material, material_uv(in));
    if diffuse.a < 0.1 {
        discard;
    }
    let saturated = clamp(diffuse.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(saturated * saturated * in.color.rgb, diffuse.a)
        * in.color.a;
}

@fragment
fn fs_lit(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = material_uv(in);
    let diffuse_sample = clamp(
        textureSample(t_diffuse, s_material, uv).rgb,
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    let albedo = diffuse_sample * diffuse_sample;
    let specular_map = textureSample(t_specular, s_material, uv).rgb;
    let opacity = textureSample(t_opacity, s_material, uv).r;
    let normal_xy = textureSample(t_normal, s_material, uv).xy * 2.0
        - vec2<f32>(1.0);
    let tangent_normal = vec3<f32>(
        normal_xy,
        sqrt(max(1.0 - dot(normal_xy, normal_xy), 0.0)),
    );
    let world_normal = normalize(
        tangent_normal.x * in.tangent
        + tangent_normal.y * in.binormal
        + tangent_normal.z * in.normal,
    );

    let light_direction = normalize(lighting.dir_light_vec.xyz);
    let n_dot_l = max(dot(world_normal, light_direction), 0.0);
    var directional_diffuse = lighting.dir_light_color.rgb * n_dot_l
        * lighting.dir_light_vec.w;

    var shadow = 1.0;
    if lighting.shadow_params.z > 0.5 && n_dot_l > 0.0 {
        let shadow_vp = mat4x4<f32>(
            lighting.shadow_vp_col0,
            lighting.shadow_vp_col1,
            lighting.shadow_vp_col2,
            lighting.shadow_vp_col3,
        );
        let projected = shadow_vp * vec4<f32>(in.world_pos, 1.0);
        shadow = compute_dir_shadow(
            projected.xyz / projected.w,
            lighting.dir_light_color.a,
            lighting.shadow_params.x,
            lighting.shadow_params.y,
            t_shadow_map,
            s_lighting,
        );
    }
    directional_diffuse *= shadow;

    let view_direction = normalize(lighting.world_camera_pos.xyz - in.world_pos);
    let world_reflect = reflect(-view_direction, world_normal);
    let specular_power = max(patch_params.material_params.x, 0.000001);
    let directional_reflection = max(dot(world_reflect, light_direction), 0.0);
    var specular_sum = directional_diffuse
        * specular_schlick(directional_reflection, specular_power);
    var diffuse_sum = directional_diffuse;

    let light_count = select(
        0u,
        min(u32(lighting.local_light_params.x), 20u),
        lighting.local_light_params.w > 0.5,
    );
    for (var index = 0u; index < light_count; index++) {
        let base = index * 8u;
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
            specular_power,
            lighting.local_light_params.z > 0.5,
            t_local_shadow_map,
            s_lighting,
        );
        diffuse_sum += result.diffuse;
        specular_sum += result.specular;
    }

    if lighting.light_volume_params.x > 0.5 {
        let volume = evaluate_light_volume(
            in.world_pos,
            world_normal,
            world_reflect,
            specular_power,
            lighting.light_volume_row0,
            lighting.light_volume_row1,
            lighting.light_volume_row2,
            t_light_volume_color,
            t_light_volume_vector,
            s_lighting,
        );
        diffuse_sum += volume.diffuse;
        specular_sum += volume.specular;
    }

    let ambient = compute_sh_fill_lighting(
        world_normal,
        lighting.sh_fill_ar,
        lighting.sh_fill_ag,
        lighting.sh_fill_ab,
        lighting.sh_fill_br,
        lighting.sh_fill_bg,
        lighting.sh_fill_bb,
        lighting.sh_fill_c,
    );
    var lit_color = (ambient + diffuse_sum) * albedo
        + specular_map * specular_sum * 3.14;

    if lighting.blackmap_params1.w > 0.5 {
        let visibility_uv = vec2<f32>(
            in.world_pos.x * lighting.blackmap_uv_scales.x,
            -in.world_pos.z * lighting.blackmap_uv_scales.x + 1.0,
        );
        let unexplored_uv = in.world_pos.xz * lighting.blackmap_uv_scales.y;
        let visibility = textureSample(t_blackmap, s_lighting, visibility_uv).a;
        let unexplored = textureSample(t_unexplored, s_lighting, unexplored_uv);
        lit_color = apply_blackmap(
            lit_color,
            in.world_pos,
            visibility,
            unexplored,
            lighting.blackmap_params0.xyz,
            lighting.blackmap_params0.w,
            lighting.blackmap_params1.x,
            lighting.blackmap_params1.yz,
            lighting.blackmap_params2.yz,
            lighting.blackmap_params2.w,
        );
    }

    let fogged = compute_fog(
        lit_color,
        in.fog_densities.x,
        in.fog_densities.y,
        lighting.fog_color.xyz,
        lighting.planar_fog_color.xyz,
    );
    return vec4<f32>(fogged, opacity * in.color.a);
}
