struct VolumeParams {
    world_low: vec4<f32>,
    world_extent: vec4<f32>,
    counts: vec4<u32>,
};

struct BufferedLight {
    position_and_radius: vec4<f32>,
    color_and_omni_mul: vec4<f32>,
    attenuation_and_specular: vec4<f32>,
    spot_at_and_add: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: VolumeParams;
@group(0) @binding(1) var<storage, read> lights: array<BufferedLight>;
@group(0) @binding(2) var color_volume: texture_storage_3d<rgba16float, write>;
@group(0) @binding(3) var vector_volume: texture_storage_3d<rgba16float, write>;

fn smooth_attenuation(value: f32) -> f32 {
    let saturated = clamp(value, 0.0, 1.0);
    return saturated * saturated * (3.0 - 2.0 * saturated);
}

@compute @workgroup_size(8, 8, 1)
fn fill_light_volume(@builtin(global_invocation_id) id: vec3<u32>) {
    let dimensions = textureDimensions(color_volume);
    if any(id >= dimensions) {
        return;
    }

    let coordinate = (vec3<f32>(id) + vec3<f32>(0.5)) / vec3<f32>(dimensions);
    let world_position = params.world_low.xyz + vec3<f32>(
        coordinate.x * params.world_extent.x,
        coordinate.z * params.world_extent.y,
        coordinate.y * params.world_extent.z,
    );
    var color = vec3<f32>(0.0);
    var direction_sum = vec3<f32>(0.0);
    var has_specular = 0.0;

    for (var index = 0u; index < params.counts.x; index += 1u) {
        let light = lights[index];
        let light_vector = light.position_and_radius.xyz - world_position;
        let distance_squared = dot(light_vector, light_vector);
        let inverse_distance = inverseSqrt(max(distance_squared, 0.000001));
        let distance = distance_squared * inverse_distance;
        let light_direction = light_vector * inverse_distance;
        let radial = smooth_attenuation(
            distance * light.color_and_omni_mul.w + light.spot_at_and_add.w,
        );
        let spot = smooth_attenuation(
            -dot(light.spot_at_and_add.xyz, light_direction)
                * light.attenuation_and_specular.y
                + light.attenuation_and_specular.z,
        );
        let decay = clamp(
            inverse_distance * light.attenuation_and_specular.x,
            0.0,
            1.0,
        );
        let attenuation = radial * spot * decay;
        color += light.color_and_omni_mul.xyz * attenuation;
        direction_sum += light_direction * attenuation * 4.0;
        if distance <= light.position_and_radius.w
            && light.attenuation_and_specular.w > 0.0
        {
            has_specular = 1.0;
        }
    }

    let direction_length_squared = dot(direction_sum, direction_sum);
    let direction = direction_sum
        * inverseSqrt(max(direction_length_squared, 0.000001));
    textureStore(color_volume, id, vec4<f32>(color, has_specular));
    textureStore(vector_volume, id, vec4<f32>(direction * 0.5 + 0.5, 1.0));
}
