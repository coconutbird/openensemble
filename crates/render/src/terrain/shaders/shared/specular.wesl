// Shared specular lighting - ported from dirLighting.inc
//
// Implements the original Halo Wars specular model which uses a
// reciprocal approximation instead of pow() for performance.
// The original HLSL: specContrib = rDotL * recipFixedFunc(specPower - specPower * rDotL + rDotL)
// This is a Schlick-style approximation to Blinn-Phong specular.
//
// Also provides a standard Blinn-Phong path for comparison.
//
// All parameters are passed explicitly so the functions are independent
// of any particular uniform layout.

/// Ratio of specular to diffuse energy (PI, matching original).
const SPEC_TO_DIFFUSE_RATIO: f32 = 3.14159265;

/// Schlick-style specular approximation (matching original recipFixedFunc).
///
/// This approximates pow(rDotL, specPower) more cheaply:
///   result = rDotL / (specPower - specPower * rDotL + rDotL)
///
/// `r_dot_l`    - dot(reflect, light_dir), clamped to [0,1]
/// `spec_power` - specular exponent (higher = tighter highlight)
fn specular_schlick(r_dot_l: f32, spec_power: f32) -> f32 {
    let denom = spec_power - spec_power * r_dot_l + r_dot_l;
    if denom < 0.0001 {
        return 0.0;
    }
    return r_dot_l / denom;
}

/// Compute directional specular contribution.
///
/// Uses the Schlick approximation matching the original engine.
/// Returns the specular intensity (scalar) to be multiplied by
/// light_color and spec_map.
///
/// `world_normal`  - surface normal in world space
/// `light_dir`     - direction TO the light (normalized)
/// `view_dir`      - direction TO the camera (normalized)
/// `spec_power`    - specular exponent
fn compute_dir_specular(
    world_normal: vec3<f32>,
    light_dir: vec3<f32>,
    view_dir: vec3<f32>,
    spec_power: f32,
) -> f32 {
    let reflect_dir = reflect(-light_dir, world_normal);
    let r_dot_l = max(dot(reflect_dir, light_dir), 0.0);
    return specular_schlick(r_dot_l, spec_power);
}

/// Compute directional specular using Blinn-Phong half-vector model.
///
/// This is the standard Blinn-Phong model used in heightfield/roads shaders.
/// Slightly different from the Schlick approximation but visually similar.
///
/// `world_normal`  - surface normal in world space
/// `light_dir`     - direction TO the light (normalized)
/// `view_dir`      - direction TO the camera (normalized)
/// `spec_power`    - specular exponent
fn compute_blinn_phong_specular(
    world_normal: vec3<f32>,
    light_dir: vec3<f32>,
    view_dir: vec3<f32>,
    spec_power: f32,
) -> f32 {
    let half_vec = normalize(light_dir + view_dir);
    let n_dot_h = max(dot(world_normal, half_vec), 0.0);
    return pow(n_dot_h, spec_power);
}
