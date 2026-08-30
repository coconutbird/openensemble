// Local spot and dual-paraboloid shadow caster path for UGX meshes.

const HAS_OPACITY: u32 = 1u << 3u;
const TERRAIN_CONFORM: u32 = 1u << 16u;

struct Scene {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
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
    ao_params: vec4<f32>,
    frame_params: vec4<f32>,
    shadow_vp_col0: vec4<f32>,
    shadow_vp_col1: vec4<f32>,
    shadow_vp_col2: vec4<f32>,
    shadow_vp_col3: vec4<f32>,
    shadow_params: vec4<f32>,
    terrain_info: vec4<f32>,
    terrain_decode: vec4<f32>,
    local_light_params: vec4<f32>,
    light_volume_params: vec4<f32>,
    light_volume_row0: vec4<f32>,
    light_volume_row1: vec4<f32>,
    light_volume_row2: vec4<f32>,
    selection_color: vec4<f32>,
    selection_params: vec4<f32>,
};

struct Material {
    tint: vec4<f32>,
    specular: vec4<f32>,
    params: vec4<f32>,
    flags: vec4<u32>,
    channels0: vec4<u32>,
    channels1: vec4<u32>,
    channels2: vec4<u32>,
    environment: vec4<f32>,
    hdr_scales: vec4<f32>,
    uv_velocity0: vec4<f32>,
    uv_velocity1: vec4<f32>,
    uv_velocity2: vec4<f32>,
    uv_velocity3: vec4<f32>,
    uv_velocity4: vec4<f32>,
    uv_velocity5: vec4<f32>,
};

struct LocalShadowPass {
    transform: mat4x4<f32>,
    params: vec4<u32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var<storage, read> matrix_palette: array<mat4x4<f32>>;
@group(0) @binding(2) var terrain_heightfield: texture_2d<f32>;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(4) var opacity_map: texture_2d<f32>;
@group(1) @binding(8) var material_sampler: sampler;
@group(2) @binding(8) var<uniform> shadow_pass: LocalShadowPass;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) texcoord0: vec2<f32>,
    @location(5) texcoord1: vec2<f32>,
    @location(6) texcoord2: vec2<f32>,
    @location(7) color: vec4<f32>,
    @location(8) joints: vec4<u32>,
    @location(9) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texcoord0: vec2<f32>,
    @location(1) texcoord1: vec2<f32>,
    @location(2) texcoord2: vec2<f32>,
    @location(3) view_depth: f32,
    @location(4) @interpolate(flat) dual_paraboloid: u32,
};

fn has_flag(flag: u32) -> bool {
    return (material.flags.x & flag) != 0u;
}

fn skin_matrix(input: VertexInput) -> mat4x4<f32> {
    return matrix_palette[input.joints.x] * input.weights.x
        + matrix_palette[input.joints.y] * input.weights.y
        + matrix_palette[input.joints.z] * input.weights.z
        + matrix_palette[input.joints.w] * input.weights.w;
}

fn terrain_height(world_position: vec3<f32>) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(terrain_heightfield));
    let grid = vec2<i32>(
        i32((world_position.z - scene.terrain_decode.z) * scene.terrain_info.y),
        i32((world_position.x - scene.terrain_decode.y) * scene.terrain_info.y),
    );
    let coordinates = clamp(grid, vec2<i32>(0), dimensions - vec2<i32>(1));
    let normalized_y = textureLoad(terrain_heightfield, coordinates, 0).g;
    return (normalized_y - scene.terrain_decode.x) * scene.terrain_info.z
        - scene.terrain_info.w;
}

fn world_position(input: VertexInput) -> vec4<f32> {
    let world_matrix = scene.model * skin_matrix(input);
    var position = world_matrix * vec4<f32>(input.position, 1.0);
    if has_flag(TERRAIN_CONFORM) {
        let anchor = world_matrix[3].xyz;
        let anchor_offset = anchor.y - terrain_height(anchor);
        position.y = terrain_height(position.xyz) + anchor_offset;
    }
    return position;
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let transformed = shadow_pass.transform * world_position(input);
    var output: VertexOutput;
    if shadow_pass.params.x != 0u {
        let radial_distance = max(length(transformed.xyz), 0.0001);
        let direction = transformed.xyz / radial_distance;
        let denominator = max(direction.z + 1.0, 0.0001);
        output.clip_position = vec4<f32>(
            direction.xy / denominator,
            clamp((radial_distance - 1.0) / 100.0, 0.0, 1.0),
            1.0,
        );
    } else {
        output.clip_position = transformed;
    }
    output.view_depth = transformed.z;
    output.dual_paraboloid = shadow_pass.params.x;
    output.texcoord0 = input.texcoord0;
    output.texcoord1 = input.texcoord1;
    output.texcoord2 = input.texcoord2;
    return output;
}

fn opacity_uv(input: VertexOutput) -> vec2<f32> {
    var uv = input.texcoord0;
    if material.channels0.w == 1u {
        uv = input.texcoord1;
    } else if material.channels0.w == 2u {
        uv = input.texcoord2;
    }
    return uv + material.uv_velocity1.zw * scene.frame_params.x;
}

@fragment
fn fs_main(input: VertexOutput) {
    let tint_alpha = material.params.x * material.tint.a * scene.frame_params.y;
    if has_flag(HAS_OPACITY) {
        let opacity = tint_alpha
            * textureSample(opacity_map, material_sampler, opacity_uv(input)).r;
        if input.dual_paraboloid != 0u {
            if input.view_depth < 1.0
                || select(opacity < 0.5, opacity < (1.0 / 255.0), tint_alpha < 0.1)
            {
                discard;
            }
            return;
        }
        if material.params.y > 0.0 && opacity < material.params.y {
            discard;
        }
        return;
    }
    if input.dual_paraboloid != 0u
        && (input.view_depth < 0.1 || tint_alpha < (1.0 / 255.0))
    {
        discard;
    }
}
