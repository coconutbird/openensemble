import package::shared::fog::compute_fog;
import package::shared::fog::compute_planar_fog_density;
import package::shared::fog::compute_radial_fog_density;
import package::shared::lighting::compute_sh_fill_lighting;
import package::shared::normal::compute_tbn;
import package::shared::shadows::compute_dir_shadow;
import package::shared::specular::specular_schlick;

// Oracle-oriented terrain road shader, translated from terrainRoads shaders
// #0 (terrain-conforming vertex stage) and #1 (lit pixel stage).

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct RoadParams {
    // x = reciprocal terrain vertex count, y = tile scale.
    terrain_values: vec4<f32>,
    position_mid: vec4<f32>,
    position_range: vec4<f32>,
    camera_position: vec4<f32>,
    dir_light_vector: vec4<f32>,
    dir_light_color: vec4<f32>,
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
    // x = bump power, y = rational specular power.
    material_params: vec4<f32>,
    shadow_vp_col0: vec4<f32>,
    shadow_vp_col1: vec4<f32>,
    shadow_vp_col2: vec4<f32>,
    shadow_vp_col3: vec4<f32>,
    shadow_params: vec4<f32>,
};
@group(1) @binding(0) var<uniform> params: RoadParams;
@group(1) @binding(1) var t_terrain_position: texture_2d<f32>;
@group(1) @binding(2) var t_terrain_basis: texture_2d<f32>;
@group(1) @binding(3) var s_terrain: sampler;
@group(1) @binding(4) var t_shadow_map: texture_2d_array<f32>;

@group(2) @binding(0) var t_road_albedo: texture_2d<f32>;
@group(2) @binding(1) var t_road_normal: texture_2d<f32>;
@group(2) @binding(2) var t_road_specular: texture_2d<f32>;
@group(2) @binding(3) var s_road: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tangent: vec3<f32>,
    @location(2) binormal: vec3<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) shadow_position: vec3<f32>,
    @location(5) world_position: vec3<f32>,
    @location(6) fog_densities: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let terrain_uv = (in.position.zx / params.terrain_values.y)
        * params.terrain_values.x;
    let position_sample = textureSampleLevel(
        t_terrain_position,
        s_terrain,
        terrain_uv,
        0.0,
    );
    let basis_sample = textureSampleLevel(
        t_terrain_basis,
        s_terrain,
        terrain_uv,
        0.0,
    );
    let terrain_normal = basis_sample.zyx * 2.0 - 1.0;
    let tbn = compute_tbn(terrain_normal);
    let terrain_y = position_sample.y * params.position_range.y
        - params.position_mid.y
        + 0.01;
    let world_position = vec3<f32>(in.position.x, terrain_y, in.position.z);
    let shadow_vp = mat4x4<f32>(
        params.shadow_vp_col0,
        params.shadow_vp_col1,
        params.shadow_vp_col2,
        params.shadow_vp_col3,
    );
    let shadow_clip = shadow_vp * vec4<f32>(world_position, 1.0);
    let radial_fog = compute_radial_fog_density(
        world_position,
        params.camera_position.xyz,
        params.fog_params.x,
        params.fog_params.y,
    );
    var planar_fog = 1.0;
    if params.planar_fog_params.x > 0.5 {
        planar_fog = compute_planar_fog_density(
            world_position,
            params.camera_position.xyz,
            params.planar_fog_params.y,
            params.planar_fog_params.z,
        );
    }

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.uv = in.uv;
    out.tangent = tbn[0];
    out.binormal = tbn[1];
    out.normal = terrain_normal;
    out.shadow_position = shadow_clip.xyz / shadow_clip.w;
    out.world_position = world_position;
    out.fog_densities = vec2<f32>(radial_fog, planar_fog);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_road_albedo, s_road, in.uv).rgb;
    let encoded_xy = textureSample(t_road_normal, s_road, in.uv).xy * 2.0 - 1.0;
    let reconstructed_z = sqrt(abs(1.0 - dot(encoded_xy, encoded_xy)));
    let tangent_normal = normalize(vec3<f32>(
        encoded_xy * params.material_params.x,
        reconstructed_z,
    ));
    let world_normal = normalize(
        tangent_normal.x * in.tangent
            + tangent_normal.y * in.binormal
            + tangent_normal.z * in.normal,
    );

    let light_direction = normalize(params.dir_light_vector.xyz);
    let normal_dot_light = max(dot(world_normal, light_direction), 0.0);
    var directional = params.dir_light_color.rgb * normal_dot_light;
    var shadow = 1.0;
    if normal_dot_light > 0.0 && params.shadow_params.z > 0.5 {
        shadow = compute_dir_shadow(
            in.shadow_position,
            params.dir_light_color.a,
            params.shadow_params.x,
            params.shadow_params.y,
            t_shadow_map,
            s_road,
        );
    }
    directional *= shadow;

    let camera_to_surface = normalize(in.world_position - params.camera_position.xyz);
    let reflected_view = reflect(camera_to_surface, world_normal);
    let reflection_dot_light = max(dot(reflected_view, light_direction), 0.0);
    let specular_light = directional * specular_schlick(
        reflection_dot_light,
        params.material_params.y,
    );
    let ambient = compute_sh_fill_lighting(
        world_normal,
        params.sh_fill_ar,
        params.sh_fill_ag,
        params.sh_fill_ab,
        params.sh_fill_br,
        params.sh_fill_bg,
        params.sh_fill_bb,
        params.sh_fill_c,
    );
    let specular_map = textureSample(t_road_specular, s_road, in.uv).rgb;
    let lit_color = (ambient + directional) * albedo
        + specular_map * specular_light * 3.14;
    let fogged = compute_fog(
        lit_color,
        in.fog_densities.x,
        in.fog_densities.y,
        params.fog_color.xyz,
        params.planar_fog_color.xyz,
    );
    return vec4<f32>(fogged, 1.0);
}
