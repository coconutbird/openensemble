import package::shared::specular::specular_schlick;

// Local light equations shared by terrain and foliage. The packed fields map
// directly to the first sixteen uint values in the oracle's gLightData entry.

struct LocalLightResult {
    diffuse: vec3<f32>,
    specular: vec3<f32>,
};

const MAX_LOCAL_LIGHTS: u32 = 20u;
const LIGHT_STRIDE: u32 = 8u;

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

fn evaluate_spot_diffuse(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    omni_mul: f32,
    omni_add: f32,
    decay_dist: f32,
    spot_mul: f32,
    spot_add: f32,
    spot_at: vec3<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
) -> vec3<f32> {
    let light = local_light_attenuation(
        light_pos, omni_mul, omni_add, decay_dist, spot_mul, spot_add, spot_at,
        world_pos, world_normal,
    );
    return light_color * light.w;
}

fn evaluate_spot_specular(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    omni_mul: f32,
    omni_add: f32,
    decay_dist: f32,
    spot_mul: f32,
    spot_add: f32,
    spot_at: vec3<f32>,
    spec_intensity: f32,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    world_reflect: vec3<f32>,
    spec_power: f32,
) -> LocalLightResult {
    let light = local_light_attenuation(
        light_pos, omni_mul, omni_add, decay_dist, spot_mul, spot_add, spot_at,
        world_pos, world_normal,
    );
    let diffuse = light_color * light.w;
    let reflect_dot_light = max(dot(world_reflect, light.xyz), 0.0);
    let denominator = spec_power - spec_power * reflect_dot_light + reflect_dot_light;
    let specular_response = select(
        0.0,
        reflect_dot_light / denominator,
        abs(denominator) > 0.000001,
    );
    return LocalLightResult(
        diffuse,
        diffuse * (specular_response * spec_intensity),
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

fn load_local_shadow(
    shadow_map: texture_depth_2d_array,
    uv: vec2<f32>,
    layer: u32,
) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(shadow_map, 0));
    let texel = clamp(
        vec2<i32>(floor(uv * vec2<f32>(dimensions))),
        vec2<i32>(0),
        dimensions - vec2<i32>(1),
    );
    return textureLoad(shadow_map, texel, i32(layer), 0);
}

fn local_shadow_bilinear(
    shadow_map: texture_depth_2d_array,
    shadow_sampler: sampler,
    uv: vec2<f32>,
    depth: f32,
    layer: u32,
) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(shadow_map, 0));
    let half_texel = vec2<f32>(0.5) / dimensions;
    let fractional = fract(dimensions * uv);
    let inverse = vec2<f32>(1.0) - fractional;

    let comparisons = clamp(
        vec4<f32>(
            load_local_shadow(shadow_map, uv + vec2<f32>(-half_texel.x, -half_texel.y), layer),
            load_local_shadow(shadow_map, uv + vec2<f32>( half_texel.x, -half_texel.y), layer),
            load_local_shadow(shadow_map, uv + vec2<f32>(-half_texel.x,  half_texel.y), layer),
            load_local_shadow(shadow_map, uv + vec2<f32>( half_texel.x,  half_texel.y), layer),
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
    shadow_map: texture_depth_2d_array,
    shadow_sampler: sampler,
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

    let shadow = local_shadow_bilinear(
        shadow_map,
        shadow_sampler,
        uv,
        depth,
        layer,
    );
    return mix(shadow, 1.0, clamp(shadow_fade, 0.0, 1.0));
}

fn evaluate_spot_specular_shadowed(
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
    shadow_map: texture_depth_2d_array,
    shadow_sampler: sampler,
) -> LocalLightResult {
    var result = evaluate_spot_specular(
        position_and_omni.xyz,
        color_and_omni.xyz,
        position_and_omni.w,
        color_and_omni.w,
        attenuation_and_shadow.x,
        attenuation_and_shadow.y,
        attenuation_and_shadow.z,
        spot_and_specular.xyz,
        spot_and_specular.w,
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
            shadow_map,
            shadow_sampler,
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
    transform_row0: vec4<f32>,
    transform_row1: vec4<f32>,
    transform_row2: vec4<f32>,
    color_texture: texture_3d<f32>,
    vector_texture: texture_3d<f32>,
    volume_sampler: sampler,
) -> LocalLightResult {
    let homogeneous_position = vec4<f32>(world_pos, 1.0);
    let uvw = vec3<f32>(
        dot(homogeneous_position, transform_row0),
        dot(homogeneous_position, transform_row1),
        dot(homogeneous_position, transform_row2),
    );
    let color_sample = textureSample(color_texture, volume_sampler, uvw);
    let light_color = color_sample.rgb * 12.0;
    let encoded_direction = textureSample(vector_texture, volume_sampler, uvw).rgb
        - vec3<f32>(0.5);
    let light_direction = encoded_direction
        * inverseSqrt(dot(encoded_direction, encoded_direction) + 0.000001);
    let normal_response = max(dot(light_direction, world_normal), 0.0);
    let diffuse = light_color * normal_response;
    let reflection_response = max(dot(world_reflect, light_direction), 0.0);
    let specular = diffuse
        * (specular_schlick(reflection_response, spec_power) * color_sample.a * 3.14);
    return LocalLightResult(diffuse, specular);
}
