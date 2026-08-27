// Directional cascaded variance shadow filtering from dirLighting.inc.

const VSM_EPSILON: f32 = 0.000000155;

fn sample_shadow_depth(
    shadow_map: texture_2d_array<f32>,
    shadow_samp: sampler,
    uv: vec2<f32>,
    cascade: u32,
) -> f32 {
    return textureSampleLevel(shadow_map, shadow_samp, uv, cascade, 0.0).r;
}

// The original filter is not a uniform box kernel. It blends two adjacent
// 2x3 neighborhoods using the fractional texel position, then derives the
// first and second moments from the resulting tent weights.
fn oracle_vsm_filter(
    shadow_uv: vec2<f32>,
    fragment_z: f32,
    cascade: u32,
    shadow_darkness: f32,
    shadow_map: texture_2d_array<f32>,
    shadow_samp: sampler,
) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(shadow_map, 0));
    let texel = 1.0 / dimensions;
    let fractional = fract(dimensions * shadow_uv);

    let column0 = vec3<f32>(
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(-texel.x, -texel.y), cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(-texel.x, 0.0), cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(-texel.x, texel.y), cascade),
    );
    let column1 = vec3<f32>(
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(0.0, -texel.y), cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv, cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(0.0, texel.y), cascade),
    );
    let column2 = vec3<f32>(
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(texel.x, -texel.y), cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + vec2<f32>(texel.x, 0.0), cascade),
        sample_shadow_depth(shadow_map, shadow_samp, shadow_uv + texel, cascade),
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
    let probability = saturate((variance / (depth_delta * depth_delta + variance) - 0.43) * 1.754386);
    let smooth_probability = probability * probability * (3.0 - 2.0 * probability);
    return mix(shadow_darkness, 1.0, smooth_probability);
}

// `max_cascade` is the largest valid array index, not the number of slices.
fn compute_dir_shadow(
    shadow_coords: vec3<f32>,
    shadow_darkness: f32,
    csm_scale: f32,
    max_cascade: f32,
    shadow_map: texture_2d_array<f32>,
    shadow_samp: sampler,
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
        shadow_map,
        shadow_samp,
    );
}
