// Shared shadow mapping - ported from dirLighting.inc and pcf.inc
//
// Implements Variance Shadow Mapping (VSM) with cascaded shadow maps (CSM).
// The original Halo Wars engine uses a 3D shadow map texture array with
// multiple cascades. Each cascade covers a progressively larger area.
//
// Shadow map layout:
//   - Array of 2D depth slices (one per cascade)
//   - Cascades selected by log2 of the max shadow-space coordinate
//   - VSM uses mean + variance for soft shadow edges
//
// All parameters are passed explicitly so the function is independent
// of any particular uniform layout.

const VSM_EPSILON: f32 = 0.000000155;

/// 2x2 PCF shadow filter (fast path from original pcf.inc).
///
/// Samples 4 texels around the sample point and bilinearly interpolates
/// the depth comparison results. Uses a large scale factor (20000) to
/// approximate a hard depth comparison with slight softening.
///
/// `shadow_uv`   - UV coordinates in shadow map space
/// `ndc_z`       - depth of the fragment in shadow space
/// `shadow_map`  - 2D shadow map texture
/// `shadow_samp` - comparison or linear sampler
fn pcf_2x2(
    shadow_uv: vec2<f32>,
    ndc_z: f32,
    shadow_map: texture_2d<f32>,
    shadow_samp: sampler,
) -> f32 {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(shadow_map, 0));

    // Sample 4 neighboring texels (matching original OffsetX/Y = ±0.5)
    let d00 = textureSampleLevel(shadow_map, shadow_samp, shadow_uv + vec2<f32>(-0.5, -0.5) * texel_size, 0.0).r;
    let d10 = textureSampleLevel(shadow_map, shadow_samp, shadow_uv + vec2<f32>(0.5, -0.5) * texel_size, 0.0).r;
    let d01 = textureSampleLevel(shadow_map, shadow_samp, shadow_uv + vec2<f32>(-0.5, 0.5) * texel_size, 0.0).r;
    let d11 = textureSampleLevel(shadow_map, shadow_samp, shadow_uv + vec2<f32>(0.5, 0.5) * texel_size, 0.0).r;

    // Bilinear weights from fractional texel position
    let f = fract(shadow_uv * vec2<f32>(textureDimensions(shadow_map, 0)) - 0.5);
    let weights = vec4<f32>(
        (1.0 - f.y) * (1.0 - f.x),
        (1.0 - f.y) * f.x,
        f.y * (1.0 - f.x),
        f.y * f.x,
    );

    // Depth comparison with soft edge (original: saturate((depth - ndc_z) * 20000))
    let depths = vec4<f32>(d00, d10, d01, d11);
    let cmp = clamp((depths - vec4<f32>(ndc_z)) * 20000.0, vec4<f32>(0.0), vec4<f32>(1.0));

    return dot(cmp, weights);
}

/// VSM (Variance Shadow Mapping) filter - 3x3 kernel from dirLighting.inc.
///
/// Computes mean depth and mean squared depth over a 3x3 neighborhood,
/// then uses Chebyshev's inequality to estimate the shadow percentage.
/// Produces softer shadows than PCF alone.
///
/// `shadow_uv`       - UV in shadow map space [0,1]
/// `fragment_z`      - fragment depth in shadow space [0,1]
/// `shadow_darkness`  - minimum shadow brightness (0 = fully dark)
/// `shadow_map`      - 2D shadow map texture
/// `shadow_samp`     - linear sampler
fn vsm_shadow(
    shadow_uv: vec2<f32>,
    fragment_z: f32,
    shadow_darkness: f32,
    shadow_map: texture_2d<f32>,
    shadow_samp: sampler,
) -> f32 {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(shadow_map, 0));

    // 3x3 sample grid (matching original dirLighting.inc)
    var avg_z = 0.0;
    var avg_z2 = 0.0;
    for (var dy = -1i; dy <= 1i; dy++) {
        for (var dx = -1i; dx <= 1i; dx++) {
            let offset = vec2<f32>(f32(dx), f32(dy)) * texel_size;
            let d = textureSampleLevel(shadow_map, shadow_samp, shadow_uv + offset, 0.0).r;
            avg_z += d;
            avg_z2 += d * d;
        }
    }
    avg_z /= 9.0;
    avg_z2 /= 9.0;

    // Fragment is fully lit if closer than mean depth
    if fragment_z <= avg_z {
        return 1.0;
    }

    // Chebyshev's inequality
    let variance = clamp(avg_z2 - avg_z * avg_z + VSM_EPSILON, 0.0, 1.0);
    let d = fragment_z - avg_z;
    let p_max = variance / (variance + d * d);

    // Smooth step to reduce light bleeding (matching original smoothstep(0.43, 1.0, p_max))
    return mix(shadow_darkness, 1.0, smoothstep(0.43, 1.0, p_max));
}

/// Compute directional shadow factor using cascaded shadow maps.
///
/// Selects the appropriate cascade based on the shadow-space coordinates,
/// then applies VSM filtering.
///
/// `shadow_coords`    - fragment position in shadow space (from light's view-proj)
/// `shadow_darkness`  - minimum brightness in shadow (from dir_light_color.a)
/// `csm_scale`        - cascade selection scale factor
/// `num_passes`       - number of shadow cascade passes
/// `shadow_map`       - shadow map texture
/// `shadow_samp`      - linear sampler
fn compute_dir_shadow(
    shadow_coords: vec3<f32>,
    shadow_darkness: f32,
    csm_scale: f32,
    num_passes: f32,
    shadow_map: texture_2d<f32>,
    shadow_samp: sampler,
) -> f32 {
    // Cascade selection (matching original dirLighting.inc)
    let max_coord = max(abs(shadow_coords.x), abs(shadow_coords.y));
    var cascade = floor(max_coord * csm_scale);
    if cascade < 1.0 {
        cascade = 0.0;
    } else {
        cascade = min(floor(log2(cascade) + 1.0), num_passes - 1.0);
    }

    // Scale coordinates for selected cascade
    let xy_scale = max(pow(2.0, num_passes - 1.0 - cascade), 1.0);
    var scaled = shadow_coords * vec3<f32>(xy_scale, xy_scale, 1.0);

    // Check if outside last cascade
    let abs_scaled = abs(scaled.xy);
    if cascade >= num_passes - 1.0 && max(abs_scaled.x, abs_scaled.y) > 0.999 {
        return 1.0; // Outside shadow range
    }

    // Transform to shadow map UV space
    let uv = vec2<f32>(
        scaled.x * 0.5 + 0.5,
        scaled.y * -0.5 + 0.5,
    );
    let z = clamp(shadow_coords.z, 0.0, 1.0);

    return vsm_shadow(uv, z, shadow_darkness, shadow_map, shadow_samp);
}
