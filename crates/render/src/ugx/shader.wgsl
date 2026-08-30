// Translation of the Halo Wars PC parametricshader legacy-material path.

const HAS_DIFFUSE: u32 = 1u << 0u;
const HAS_NORMAL: u32 = 1u << 1u;
const HAS_GLOSS: u32 = 1u << 2u;
const HAS_OPACITY: u32 = 1u << 3u;
const HAS_XFORM: u32 = 1u << 4u;
const HAS_EMISSIVE: u32 = 1u << 5u;
const HAS_AO: u32 = 1u << 6u;
const COLOR_GLOSS: u32 = 1u << 7u;
const TWO_SIDED: u32 = 1u << 8u;
const HAS_ENVIRONMENT: u32 = 1u << 9u;
const HAS_ENVIRONMENT_MASK: u32 = 1u << 10u;
const HAS_EMISSIVE_XFORM: u32 = 1u << 11u;
const HAS_HIGHLIGHT: u32 = 1u << 12u;
const HAS_MODULATE: u32 = 1u << 13u;
const HAS_DISTORTION: u32 = 1u << 14u;
const RECEIVES_SHADOWS: u32 = 1u << 15u;
const TERRAIN_CONFORM: u32 = 1u << 16u;
override shadow_cascade_scale: f32 = 1.0;

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

@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var<storage, read> matrix_palette: array<mat4x4<f32>>;
@group(0) @binding(2) var terrain_heightfield: texture_2d<f32>;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var diffuse_map: texture_2d<f32>;
@group(1) @binding(2) var normal_map: texture_2d<f32>;
@group(1) @binding(3) var gloss_map: texture_2d<f32>;
@group(1) @binding(4) var opacity_map: texture_2d<f32>;
@group(1) @binding(5) var xform_map: texture_2d<f32>;
@group(1) @binding(6) var emissive_map: texture_2d<f32>;
@group(1) @binding(7) var ao_map: texture_2d<f32>;
@group(1) @binding(8) var material_sampler: sampler;
@group(1) @binding(9) var environment_mask_map: texture_2d<f32>;
@group(1) @binding(10) var environment_map: texture_cube<f32>;
@group(1) @binding(11) var environment_sampler: sampler;
@group(1) @binding(12) var emissive_xform_map: texture_2d<f32>;
@group(1) @binding(13) var distortion_map: texture_2d<f32>;
@group(1) @binding(14) var highlight_map: texture_2d<f32>;
@group(1) @binding(15) var modulate_map: texture_2d<f32>;
@group(2) @binding(0) var directional_shadow_map: texture_2d_array<f32>;
@group(2) @binding(1) var directional_shadow_sampler: sampler;
@group(2) @binding(2) var<storage, read> local_lights: array<vec4<f32>>;
@group(2) @binding(3) var local_shadow_map: texture_depth_2d_array;
@group(2) @binding(4) var local_shadow_sampler: sampler;
@group(2) @binding(5) var light_volume_color: texture_3d<f32>;
@group(2) @binding(6) var light_volume_vector: texture_3d<f32>;
@group(2) @binding(7) var light_volume_sampler: sampler;

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
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec3<f32>,
    @location(3) world_binormal: vec3<f32>,
    @location(4) texcoord0: vec2<f32>,
    @location(5) texcoord1: vec2<f32>,
    @location(6) texcoord2: vec2<f32>,
    @location(7) color: vec4<f32>,
    @location(8) fog_densities: vec2<f32>,
};

struct ShadowVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) raw_depth: f32,
    @location(1) texcoord0: vec2<f32>,
    @location(2) texcoord1: vec2<f32>,
    @location(3) texcoord2: vec2<f32>,
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

fn radial_fog(world_position: vec3<f32>) -> f32 {
    let camera_vector = scene.camera_position.xyz - world_position;
    let distance_squared = dot(camera_vector, camera_vector);
    return clamp(
        exp2(-scene.fog_params.x * max(0.0, distance_squared - scene.fog_params.y)),
        0.0,
        1.0,
    );
}

fn planar_fog(world_position: vec3<f32>) -> f32 {
    if scene.planar_fog_params.x <= 0.5 {
        return 1.0;
    }
    let camera_vector = scene.camera_position.xyz - world_position;
    let distance = length(camera_vector);
    if distance < 0.001 {
        return 1.0;
    }
    let camera_y_direction = camera_vector.y / distance;
    let fog_height = world_position.y - scene.planar_fog_params.y;
    let inverse_y_direction = clamp(1.0 / -camera_y_direction, -3.4e38, 3.4e38);
    var effective_distance: f32;
    if camera_y_direction < 0.0 {
        effective_distance = distance - fog_height * inverse_y_direction;
    } else {
        effective_distance = fog_height * inverse_y_direction;
    }
    effective_distance = min(distance, max(0.0, effective_distance));
    return clamp(
        exp2(-scene.planar_fog_params.z * effective_distance * effective_distance),
        0.0,
        1.0,
    );
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

fn world_position(
    world_matrix: mat4x4<f32>,
    local_position: vec3<f32>,
) -> vec4<f32> {
    var position = world_matrix * vec4<f32>(local_position, 1.0);
    if has_flag(TERRAIN_CONFORM) {
        // The PC terrain-conform variant selects the heightfield surface at
        // the skinned matrix origin, then applies the same vertical offset to
        // the vertex sample. The current XTD position texture has one surface,
        // so the oracle's nearest-of-two channel choice collapses to this.
        let anchor = world_matrix[3].xyz;
        let anchor_offset = anchor.y - terrain_height(anchor);
        position.y = terrain_height(position.xyz) + anchor_offset;
    }
    return position;
}

fn build_vertex_output(input: VertexInput) -> VertexOutput {
    let skin = skin_matrix(input);
    let world_matrix = scene.model * skin;
    let world_position4 = world_position(world_matrix, input.position);
    let normal = normalize((world_matrix * vec4<f32>(input.normal, 0.0)).xyz);
    var tangent = normalize((world_matrix * vec4<f32>(input.tangent.xyz, 0.0)).xyz);
    tangent = normalize(tangent - normal * dot(tangent, normal));
    let handedness = select(1.0, sign(input.tangent.w), abs(input.tangent.w) > 0.0001);
    let binormal = normalize(cross(tangent, normal)) * handedness;

    var output: VertexOutput;
    output.clip_position = scene.view_projection * world_position4;
    output.world_position = world_position4.xyz;
    output.world_normal = normal;
    output.world_tangent = tangent;
    output.world_binormal = binormal;
    output.texcoord0 = input.texcoord0;
    output.texcoord1 = input.texcoord1;
    output.texcoord2 = input.texcoord2;
    output.color = input.color;
    output.fog_densities = vec2<f32>(radial_fog(world_position4.xyz), planar_fog(world_position4.xyz));
    return output;
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    return build_vertex_output(input);
}

// Sky visuals use the ordinary UGX material path, but are camera-relative
// backgrounds. Pinning their depth to the far plane keeps the authored dome
// behind terrain and units regardless of the dome mesh's physical radius.
@vertex
fn vs_sky(input: VertexInput) -> VertexOutput {
    var output = build_vertex_output(input);
    output.clip_position.z = output.clip_position.w;
    output.fog_densities = vec2<f32>(1.0);
    return output;
}

@vertex
fn vs_shadow(input: VertexInput) -> ShadowVertexOutput {
    let skin = skin_matrix(input);
    let world_matrix = scene.model * skin;
    let conformed_position = world_position(world_matrix, input.position);
    let shadow_view_projection = mat4x4<f32>(
        scene.shadow_vp_col0,
        scene.shadow_vp_col1,
        scene.shadow_vp_col2,
        scene.shadow_vp_col3,
    );
    var clip_position = shadow_view_projection * conformed_position;
    clip_position.x *= shadow_cascade_scale;
    clip_position.y *= shadow_cascade_scale;

    var output: ShadowVertexOutput;
    output.clip_position = clip_position;
    output.raw_depth = clip_position.z;
    output.texcoord0 = input.texcoord0;
    output.texcoord1 = input.texcoord1;
    output.texcoord2 = input.texcoord2;
    return output;
}

fn choose_uv(channel: u32, input: VertexOutput) -> vec2<f32> {
    switch channel {
        case 1u: { return input.texcoord1; }
        case 2u: { return input.texcoord2; }
        default: { return input.texcoord0; }
    }
}

fn animated_uv(channel: u32, input: VertexOutput, velocity: vec2<f32>) -> vec2<f32> {
    return choose_uv(channel, input) + velocity * scene.frame_params.x;
}

fn choose_shadow_uv(channel: u32, input: ShadowVertexOutput) -> vec2<f32> {
    switch channel {
        case 1u: { return input.texcoord1; }
        case 2u: { return input.texcoord2; }
        default: { return input.texcoord0; }
    }
}

@fragment
fn fs_shadow(input: ShadowVertexOutput) -> @location(0) vec2<f32> {
    var opacity = material.params.x;
    if has_flag(HAS_OPACITY) {
        let uv = choose_shadow_uv(material.channels0.w, input)
            + material.uv_velocity1.zw * scene.frame_params.x;
        opacity *= textureSample(opacity_map, material_sampler, uv).r;
    }
    if material.params.y > 0.0 && opacity < material.params.y {
        discard;
    }

    let slope = clamp(
        max(dpdyCoarse(input.raw_depth), dpdxCoarse(input.raw_depth)),
        0.0,
        0.004,
    );
    let biased_depth = input.raw_depth + slope * 1.2 + 0.0005;
    return vec2<f32>(biased_depth, 0.0);
}

@fragment
fn fs_distortion(input: VertexOutput) -> @location(0) vec4<f32> {
    if !has_flag(HAS_DISTORTION) {
        discard;
    }
    var offset = textureSample(
        distortion_map,
        material_sampler,
        animated_uv(material.channels2.y, input, material.uv_velocity4.zw),
    ).rg * 2.0 - vec2<f32>(1.0);
    let dead_zone = 3.0 / 255.0;
    if abs(offset.x) <= dead_zone {
        offset.x = 0.0;
    }
    if abs(offset.y) <= dead_zone {
        offset.y = 0.0;
    }
    offset *= material.params.x * scene.frame_params.y;
    let magnitude = abs(offset.x) + abs(offset.y);
    if magnitude < material.params.y {
        discard;
    }
    return vec4<f32>(offset, 0.0, magnitude);
}

fn sh_fill(normal: vec3<f32>) -> vec3<f32> {
    let normal4 = vec4<f32>(normal, 1.0);
    let linear = vec3<f32>(
        dot(normal4, scene.sh_fill_ar),
        dot(normal4, scene.sh_fill_ag),
        dot(normal4, scene.sh_fill_ab),
    );
    let quadratic_input = vec4<f32>(
        normal.x * normal.y,
        normal.y * normal.z,
        normal.z * normal.z,
        normal.z * normal.x,
    );
    let quadratic = vec3<f32>(
        dot(quadratic_input, scene.sh_fill_br),
        dot(quadratic_input, scene.sh_fill_bg),
        dot(quadratic_input, scene.sh_fill_bb),
    );
    return linear + quadratic + scene.sh_fill_c.xyz * (normal.x * normal.x - normal.y * normal.y);
}

fn reciprocal_specular(response: f32, power: f32) -> f32 {
    let denominator = power - power * response + response;
    return select(0.0, response / denominator, denominator >= 0.0001);
}

fn fog_color(color: vec3<f32>, densities: vec2<f32>) -> vec3<f32> {
    let planar = mix(scene.planar_fog_color.xyz, color, densities.y);
    return mix(scene.fog_color.xyz, planar, densities.x);
}

const VSM_EPSILON: f32 = 0.000000155;

fn sample_shadow_depth(uv: vec2<f32>, cascade: u32) -> f32 {
    return textureSampleLevel(
        directional_shadow_map,
        directional_shadow_sampler,
        uv,
        cascade,
        0.0,
    ).r;
}

// dirLighting.inc blends two adjacent 2x3 neighborhoods into a nine-sample
// tent filter, then derives the first and second moments from those samples.
fn oracle_vsm_filter(
    shadow_uv: vec2<f32>,
    fragment_z: f32,
    cascade: u32,
    shadow_darkness: f32,
) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(directional_shadow_map, 0));
    let texel = 1.0 / dimensions;
    let fractional = fract(dimensions * shadow_uv);

    let column0 = vec3<f32>(
        sample_shadow_depth(shadow_uv + vec2<f32>(-texel.x, -texel.y), cascade),
        sample_shadow_depth(shadow_uv + vec2<f32>(-texel.x, 0.0), cascade),
        sample_shadow_depth(shadow_uv + vec2<f32>(-texel.x, texel.y), cascade),
    );
    let column1 = vec3<f32>(
        sample_shadow_depth(shadow_uv + vec2<f32>(0.0, -texel.y), cascade),
        sample_shadow_depth(shadow_uv, cascade),
        sample_shadow_depth(shadow_uv + vec2<f32>(0.0, texel.y), cascade),
    );
    let column2 = vec3<f32>(
        sample_shadow_depth(shadow_uv + vec2<f32>(texel.x, -texel.y), cascade),
        sample_shadow_depth(shadow_uv + vec2<f32>(texel.x, 0.0), cascade),
        sample_shadow_depth(shadow_uv + texel, cascade),
    );

    let upper = mix(column0, column1, fractional.x);
    let lower = mix(column1, column2, fractional.x);
    let y_weight = fractional.y * 0.25;
    let inv_y_weight = 0.25 - y_weight;
    let mean = dot(upper.zxy, vec3<f32>(y_weight, inv_y_weight, 0.25))
        + dot(lower.zyx, vec3<f32>(y_weight, 0.25, inv_y_weight));

    let moment_weights = vec3<f32>(inv_y_weight, 0.25, y_weight);
    let column0_squared = column0 * column0;
    let column1_squared = column1 * column1;
    let column2_squared = column2 * column2;
    let second_moment = dot(
        mix(column1_squared, column2_squared, fractional.x),
        moment_weights,
    ) + dot(
        mix(column0_squared, column1_squared, fractional.x),
        moment_weights,
    );

    if mean >= fragment_z {
        return 1.0;
    }

    let variance = saturate(second_moment - mean * mean + VSM_EPSILON);
    let depth_delta = fragment_z - mean;
    let probability = saturate(
        (variance / (depth_delta * depth_delta + variance) - 0.43) * 1.754386,
    );
    let smooth_probability = probability * probability * (3.0 - 2.0 * probability);
    return mix(shadow_darkness, 1.0, smooth_probability);
}

fn compute_dir_shadow(
    shadow_coords: vec3<f32>,
    shadow_darkness: f32,
    csm_scale: f32,
    max_cascade: f32,
) -> f32 {
    let level_seed = floor(max(abs(shadow_coords.x), abs(shadow_coords.y)) * csm_scale);
    var cascade = 0.0;
    if level_seed >= 1.0 {
        cascade = floor(min(log2(level_seed) + 1.0, max_cascade));
    }

    let scale = max(exp2(max_cascade - cascade), 1.0);
    let scaled_coords = shadow_coords * vec3<f32>(scale, scale, 1.0);
    if cascade >= max_cascade && max(abs(scaled_coords.x), abs(scaled_coords.y)) > 0.999 {
        return 1.0;
    }

    let shadow_uv = scaled_coords.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return oracle_vsm_filter(
        shadow_uv,
        saturate(scaled_coords.z),
        u32(cascade),
        shadow_darkness,
    );
}

struct LocalLightResult {
    diffuse: vec3<f32>,
    specular: vec3<f32>,
};

fn smooth_attenuation(value: f32) -> f32 {
    let saturated = clamp(value, 0.0, 1.0);
    return saturated * saturated * (3.0 - 2.0 * saturated);
}

fn local_light_attenuation(
    light_pos: vec3<f32>,
    omni_mul: f32,
    omni_add: f32,
    decay_dist: f32,
    spot_mul: f32,
    spot_add: f32,
    spot_at: vec3<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
) -> vec4<f32> {
    let light_vector = light_pos - world_pos;
    let length_squared = dot(light_vector, light_vector);
    let inverse_distance = inverseSqrt(max(length_squared, 0.000001));
    let distance = length_squared * inverse_distance;
    let light_direction = light_vector * inverse_distance;
    let normal_attenuation = smooth_attenuation(dot(light_direction, world_normal));
    let spot_attenuation = smooth_attenuation(
        -dot(spot_at, light_direction) * spot_mul + spot_add,
    );
    let distance_attenuation = smooth_attenuation(distance * omni_mul + omni_add);
    let decay = clamp(inverse_distance * decay_dist, 0.0, 1.0);
    return vec4<f32>(
        light_direction,
        normal_attenuation * spot_attenuation * distance_attenuation * decay,
    );
}

fn evaluate_local_light(
    position_and_omni: vec4<f32>,
    color_and_omni: vec4<f32>,
    attenuation_and_shadow: vec4<f32>,
    spot_and_specular: vec4<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    world_reflect: vec3<f32>,
    spec_power: f32,
) -> LocalLightResult {
    let light = local_light_attenuation(
        position_and_omni.xyz,
        position_and_omni.w,
        color_and_omni.w,
        attenuation_and_shadow.x,
        attenuation_and_shadow.y,
        attenuation_and_shadow.z,
        spot_and_specular.xyz,
        world_pos,
        world_normal,
    );
    let diffuse = color_and_omni.xyz * light.w;
    let reflect_dot_light = max(dot(world_reflect, light.xyz), 0.0);
    return LocalLightResult(
        diffuse,
        diffuse
            * reciprocal_specular(reflect_dot_light, spec_power)
            * spot_and_specular.w,
    );
}

fn local_shadow_bounds(preset: u32) -> vec4<f32> {
    switch min(preset, 4u) {
        case 0u: {
            return vec4<f32>(0.0, 0.0, 1.0, 1.0);
        }
        case 1u: {
            return vec4<f32>(0.0, 0.0, 0.5, 0.5);
        }
        case 2u: {
            return vec4<f32>(0.5, 0.0, 1.0, 0.5);
        }
        case 3u: {
            return vec4<f32>(0.0, 0.5, 0.5, 1.0);
        }
        default: {
            return vec4<f32>(0.5, 0.5, 1.0, 1.0);
        }
    }
}

fn local_shadow_layer(normalized_coordinate: f32) -> u32 {
    return u32(clamp(
        floor(normalized_coordinate * 8.0),
        0.0,
        7.0,
    ));
}

fn load_local_shadow(uv: vec2<f32>, layer: u32) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(local_shadow_map, 0));
    let texel = clamp(
        vec2<i32>(floor(uv * vec2<f32>(dimensions))),
        vec2<i32>(0),
        dimensions - vec2<i32>(1),
    );
    return textureLoad(local_shadow_map, texel, i32(layer), 0);
}

fn local_shadow_bilinear(
    uv: vec2<f32>,
    depth: f32,
    layer: u32,
) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(local_shadow_map, 0));
    let half_texel = vec2<f32>(0.5) / dimensions;
    let fractional = fract(dimensions * uv);
    let inverse = vec2<f32>(1.0) - fractional;
    let comparisons = clamp(
        vec4<f32>(
            load_local_shadow(uv + vec2<f32>(-half_texel.x, -half_texel.y), layer),
            load_local_shadow(uv + vec2<f32>( half_texel.x, -half_texel.y), layer),
            load_local_shadow(uv + vec2<f32>(-half_texel.x,  half_texel.y), layer),
            load_local_shadow(uv + vec2<f32>( half_texel.x,  half_texel.y), layer),
        ) - vec4<f32>(depth),
        vec4<f32>(0.0),
        vec4<f32>(1.0 / 20000.0),
    ) * 20000.0;
    let weights = vec4<f32>(
        inverse.x * inverse.y,
        fractional.x * inverse.y,
        inverse.x * fractional.y,
        fractional.x * fractional.y,
    );
    return dot(comparisons, weights);
}

fn evaluate_local_shadow(
    world_pos: vec3<f32>,
    shadow_index: f32,
    shadow_row0: vec4<f32>,
    shadow_row1: vec4<f32>,
    shadow_row2: vec4<f32>,
    bounds_preset: f32,
    shadow_fade: f32,
) -> f32 {
    let homogeneous_position = vec4<f32>(world_pos, 1.0);
    let transformed = vec3<f32>(
        dot(homogeneous_position, shadow_row0),
        dot(homogeneous_position, shadow_row1),
        dot(homogeneous_position, shadow_row2),
    );

    var uv: vec2<f32>;
    var depth: f32;
    var layer: u32;
    if shadow_index < -1.0 {
        let distance = length(transformed);
        let safe_denominator = max(distance + abs(transformed.z), 0.000001);
        uv = transformed.xy / safe_denominator * vec2<f32>(0.5, -0.5)
            + vec2<f32>(0.5);
        depth = (distance - 1.0) * 0.01;
        let layer_coordinate = select(
            -shadow_index - 1.0,
            -shadow_index - 0.875,
            transformed.z < 0.0,
        );
        layer = local_shadow_layer(max(layer_coordinate, 0.0));
    } else {
        let safe_z = select(
            transformed.z,
            select(-0.000001, 0.000001, transformed.z >= 0.0),
            abs(transformed.z) >= 0.000001,
        );
        let bounds = local_shadow_bounds(u32(max(bounds_preset, 0.0)));
        uv = clamp(transformed.xy / safe_z, bounds.xy, bounds.zw);
        depth = (transformed.z * 1.007874 - 1.007874) / safe_z;
        layer = local_shadow_layer(max(shadow_index, 0.0));
    }

    let shadow = local_shadow_bilinear(uv, depth, layer);
    return mix(shadow, 1.0, clamp(shadow_fade, 0.0, 1.0));
}

fn evaluate_local_light_shadowed(
    position_and_omni: vec4<f32>,
    color_and_omni: vec4<f32>,
    attenuation_and_shadow: vec4<f32>,
    spot_and_specular: vec4<f32>,
    shadow_row0: vec4<f32>,
    shadow_row1: vec4<f32>,
    shadow_row2: vec4<f32>,
    shadow_metadata: vec4<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    world_reflect: vec3<f32>,
    spec_power: f32,
    shadows_enabled: bool,
) -> LocalLightResult {
    var result = evaluate_local_light(
        position_and_omni,
        color_and_omni,
        attenuation_and_shadow,
        spot_and_specular,
        world_pos,
        world_normal,
        world_reflect,
        spec_power,
    );
    if shadows_enabled
        && attenuation_and_shadow.w != -1.0
        && any(result.diffuse > vec3<f32>(0.0))
    {
        let shadow = evaluate_local_shadow(
            world_pos,
            attenuation_and_shadow.w,
            shadow_row0,
            shadow_row1,
            shadow_row2,
            shadow_metadata.x,
            shadow_metadata.y,
        );
        result.diffuse *= shadow;
        result.specular *= shadow;
    }
    return result;
}

fn evaluate_light_volume(
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    world_reflect: vec3<f32>,
    spec_power: f32,
) -> LocalLightResult {
    let homogeneous_position = vec4<f32>(world_pos, 1.0);
    let uvw = vec3<f32>(
        dot(homogeneous_position, scene.light_volume_row0),
        dot(homogeneous_position, scene.light_volume_row1),
        dot(homogeneous_position, scene.light_volume_row2),
    );
    let color_sample = textureSample(light_volume_color, light_volume_sampler, uvw);
    let light_color = color_sample.rgb * 12.0;
    let encoded_direction = textureSample(
        light_volume_vector,
        light_volume_sampler,
        uvw,
    ).rgb - vec3<f32>(0.5);
    let light_direction = encoded_direction
        * inverseSqrt(dot(encoded_direction, encoded_direction) + 0.000001);
    let normal_response = max(dot(light_direction, world_normal), 0.0);
    let diffuse = light_color * normal_response;
    let reflection_response = max(dot(world_reflect, light_direction), 0.0);
    let specular = diffuse
        * reciprocal_specular(reflection_response, spec_power)
        * color_sample.a
        * 3.14;
    return LocalLightResult(diffuse, specular);
}

fn targeting_selection(world_y: f32) -> vec3<f32> {
    if scene.selection_params.w <= 0.5 {
        return vec3<f32>(0.0);
    }
    let uv = world_y * scene.selection_params.x + scene.selection_params.y;
    if uv < 0.0 || uv > 1.0 {
        return vec3<f32>(0.0);
    }
    let scan = sin(uv * 3.14159265);
    return scene.selection_color.rgb
        * scene.selection_params.z
        * scan
        * scan;
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var diffuse_sample = vec4<f32>(1.0);
    if has_flag(HAS_DIFFUSE) {
        diffuse_sample = textureSample(
            diffuse_map,
            material_sampler,
            animated_uv(material.channels0.x, input, material.uv_velocity0.xy),
        );
    }
    if has_flag(HAS_MODULATE) {
        diffuse_sample *= textureSample(
            modulate_map,
            material_sampler,
            animated_uv(material.channels2.w, input, material.uv_velocity5.zw),
        );
    }

    var tint_mask = diffuse_sample.a;
    if has_flag(HAS_XFORM) {
        tint_mask = textureSample(
            xform_map,
            material_sampler,
            animated_uv(material.channels1.x, input, material.uv_velocity3.zw),
        ).g;
    }
    let tint = mix(vec3<f32>(1.0), material.tint.rgb, tint_mask);
    let albedo = diffuse_sample.rgb * tint * input.color.rgb;

    var opacity = material.params.x * material.tint.a * input.color.a;
    if has_flag(HAS_OPACITY) {
        opacity *= textureSample(
            opacity_map,
            material_sampler,
            animated_uv(material.channels0.w, input, material.uv_velocity1.zw),
        ).r;
    }
    if opacity <= max(2.0 / 255.0, material.params.y) {
        discard;
    }
    opacity *= scene.frame_params.y;

    var vertex_normal = normalize(input.world_normal);
    if has_flag(TWO_SIDED) && !front_facing {
        vertex_normal = -vertex_normal;
    }
    var world_normal = vertex_normal;
    if has_flag(HAS_NORMAL) {
        let encoded = textureSample(
            normal_map,
            material_sampler,
            animated_uv(material.channels0.y, input, material.uv_velocity0.zw),
        ).rg * 2.0 - vec2<f32>(1.0);
        var tangent_normal = vec3<f32>(
            encoded,
            sqrt(max(1.0 - dot(encoded, encoded), 0.0)),
        );
        tangent_normal.x *= material.params.w;
        tangent_normal.y *= material.params.w;
        tangent_normal = normalize(tangent_normal);
        world_normal = normalize(
            input.world_tangent * tangent_normal.x
            + input.world_binormal * tangent_normal.y
            + vertex_normal * tangent_normal.z
        );
    }

    var ambient = sh_fill(world_normal);
    if has_flag(HAS_AO) {
        let ao = textureSample(
            ao_map,
            material_sampler,
            animated_uv(material.channels1.z, input, material.uv_velocity3.xy),
        ).r;
        ambient *= mix(1.0, ao, scene.ao_params.x);
    }
    let light_direction = normalize(scene.dir_light_vector.xyz);
    let directional_response = max(dot(world_normal, light_direction), 0.0);
    var directional_visibility = 1.0;
    if has_flag(RECEIVES_SHADOWS)
        && scene.shadow_params.z > 0.5
        && directional_response > 0.0
    {
        let shadow_view_projection = mat4x4<f32>(
            scene.shadow_vp_col0,
            scene.shadow_vp_col1,
            scene.shadow_vp_col2,
            scene.shadow_vp_col3,
        );
        let shadow_clip = shadow_view_projection * vec4<f32>(input.world_position, 1.0);
        directional_visibility = compute_dir_shadow(
            shadow_clip.xyz / shadow_clip.w,
            scene.dir_light_color.a,
            scene.shadow_params.x,
            scene.shadow_params.y,
        );
    }
    let directional = scene.dir_light_color.rgb
        * directional_response
        * directional_visibility;

    var specular_color = material.specular.rgb;
    var gloss_strength = 1.0;
    if has_flag(HAS_GLOSS) {
        let gloss = textureSample(
            gloss_map,
            material_sampler,
            animated_uv(material.channels0.z, input, material.uv_velocity1.xy),
        );
        if has_flag(COLOR_GLOSS) {
            specular_color *= gloss.rgb;
        } else {
            gloss_strength = gloss.r;
        }
    }
    let view_direction = normalize(scene.camera_position.xyz - input.world_position);
    let reflected_view = reflect(-view_direction, world_normal);
    let reflection_response = max(dot(reflected_view, light_direction), 0.0);
    let directional_specular = directional
        * reciprocal_specular(reflection_response, material.specular.w);

    var local_diffuse = vec3<f32>(0.0);
    var local_specular = vec3<f32>(0.0);
    let local_light_count = select(
        0u,
        min(u32(max(scene.local_light_params.x, 0.0)), 20u),
        scene.local_light_params.w > 0.5,
    );
    for (var light_index = 0u; light_index < local_light_count; light_index++) {
        let base = light_index * 8u;
        let result = evaluate_local_light_shadowed(
            local_lights[base],
            local_lights[base + 1u],
            local_lights[base + 2u],
            local_lights[base + 3u],
            local_lights[base + 4u],
            local_lights[base + 5u],
            local_lights[base + 6u],
            local_lights[base + 7u],
            input.world_position,
            world_normal,
            reflected_view,
            material.specular.w,
            has_flag(RECEIVES_SHADOWS) && scene.local_light_params.z > 0.5,
        );
        local_diffuse += result.diffuse;
        local_specular += result.specular;
    }
    if scene.light_volume_params.x > 0.5 {
        let volume = evaluate_light_volume(
            input.world_position,
            world_normal,
            reflected_view,
            material.specular.w,
        );
        local_diffuse += volume.diffuse;
        local_specular += volume.specular;
    }
    let specular = (directional_specular + local_specular)
        * specular_color
        * gloss_strength
        * 3.14;

    var emissive = vec3<f32>(0.0);
    if has_flag(HAS_EMISSIVE) {
        let self_sample = textureSample(
            emissive_map,
            material_sampler,
            animated_uv(material.channels1.y, input, material.uv_velocity2.xy),
        );
        emissive = pow(self_sample.rgb, vec3<f32>(2.2))
            * pow(self_sample.a, 2.2)
            * material.hdr_scales.x
            * 32.0;
        if has_flag(HAS_EMISSIVE_XFORM) {
            let emissive_mask = textureSample(
                emissive_xform_map,
                material_sampler,
                animated_uv(material.channels2.x, input, material.uv_velocity4.xy),
            ).g;
            emissive = mix(emissive * input.color.rgb, emissive, emissive_mask);
        }
    }

    var highlight = vec3<f32>(0.0);
    if has_flag(HAS_HIGHLIGHT) {
        let highlight_sample = textureSample(
            highlight_map,
            material_sampler,
            animated_uv(material.channels2.z, input, material.uv_velocity5.xy),
        );
        highlight = pow(highlight_sample.rgb, vec3<f32>(2.2))
            * pow(highlight_sample.a, 2.2)
            * material.hdr_scales.z
            * 32.0;
    }

    var environment = vec3<f32>(0.0);
    if has_flag(HAS_ENVIRONMENT) {
        let environment_sample = textureSampleBias(
            environment_map,
            environment_sampler,
            reflected_view,
            material.environment.y,
        );
        let environment_radiance = environment_sample.rgb
            * environment_sample.a
            * material.hdr_scales.y;
        let shadow_darkness = scene.dir_light_color.a;
        let light_visibility = (
            clamp(dot(vertex_normal, light_direction) * 16.0, 0.0, 1.0)
                * (1.0 - shadow_darkness)
                + shadow_darkness
        ) * 0.9 + 0.1;
        let fresnel = (1.0 - material.environment.z)
            * pow(
                clamp(1.0 - dot(world_normal, view_direction), 0.0, 1.0),
                material.environment.x,
            )
            + material.environment.z;
        var environment_mask = specular_color * gloss_strength;
        if has_flag(HAS_ENVIRONMENT_MASK) {
            environment_mask = textureSample(
                environment_mask_map,
                material_sampler,
                animated_uv(material.channels1.w, input, material.uv_velocity2.zw),
            ).rgb;
        }
        environment = environment_mask
            * environment_radiance
            * (light_visibility * material.environment.w * fresnel);
    }

    let lit = albedo * max(ambient + directional + local_diffuse, vec3<f32>(0.0))
        + specular
        + emissive
        + highlight
        + environment
        + targeting_selection(input.world_position.y);
    return vec4<f32>(fog_color(lit, input.fog_densities), opacity);
}
