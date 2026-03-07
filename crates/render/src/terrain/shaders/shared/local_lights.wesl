// Shared local (point/omni) lighting - ported from localLighting.inc
//
// The original engine supports up to 20 pixel-shader lights per draw call,
// each with 8 float4 constants (position, color, attenuation, spot params,
// shadow matrix columns, etc.).
//
// Our implementation uses a storage buffer of packed light data.
// Each light occupies 8 × vec4<f32> = 128 bytes.
//
// Light data layout per light (8 vec4s):
//   [0] xyz = position,       w = omni_mul
//   [1] xyz = color,          w = omni_add
//   [2] x = decay_dist,  y = spot_mul,  z = spot_add,  w = shadow_index
//   [3] xyz = spot_at,        w = spec_intensity
//   [4-6] shadow matrix columns (unused for now)
//   [7] x = shadow_uv_bounds_index, y = shadow_fade_factor
//
// All parameters are passed explicitly so the functions are independent
// of any particular uniform layout.

/// Maximum number of local lights per draw call.
const MAX_LOCAL_LIGHTS: u32 = 20u;

/// Stride in vec4 units per light in the data buffer.
const LIGHT_STRIDE: u32 = 8u;

/// Smoothstep-based attenuation (matching original's x*x*(-2x+3) hermite).
fn smooth_atten(x: f32) -> f32 {
    let c = clamp(x, 0.0, 1.0);
    return c * c * (3.0 - 2.0 * c);
}

/// Evaluate a single omni/point light's diffuse contribution.
///
/// `light_pos`    - light world position
/// `light_color`  - light RGB color
/// `omni_mul`     - distance attenuation multiplier
/// `omni_add`     - distance attenuation additive bias
/// `decay_dist`   - inverse decay distance for near-field boost
/// `world_pos`    - fragment world position
/// `world_normal` - fragment world normal
fn evaluate_omni_diffuse(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    omni_mul: f32,
    omni_add: f32,
    decay_dist: f32,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
) -> vec3<f32> {
    let light_vec = light_pos - world_pos;
    let light_vec_len2 = dot(light_vec, light_vec);
    let oo_light_dist = inverseSqrt(max(light_vec_len2, 0.0001));
    let dist = oo_light_dist * light_vec_len2;
    let light_norm = light_vec * oo_light_dist;

    // N·L attenuation
    let n_dot_l = smooth_atten(clamp(dot(light_norm, world_normal), 0.0, 1.0));

    // Distance attenuation (omni falloff)
    let dist_atten = smooth_atten(clamp(dist * omni_mul + omni_add, 0.0, 1.0));

    // Near-field decay boost
    let decay = clamp(decay_dist * oo_light_dist, 0.0, 1.0);

    return n_dot_l * dist_atten * decay * light_color;
}

/// Evaluate a single omni/point light with specular.
///
/// Returns vec2 packed as: diffuse contribution in .xyz via out params,
/// but for simplicity we return a struct-like pair.
///
/// `light_pos`      - light world position
/// `light_color`    - light RGB color
/// `omni_mul`       - distance attenuation multiplier
/// `omni_add`       - distance attenuation additive bias
/// `decay_dist`     - inverse decay distance
/// `spec_intensity` - per-light specular intensity multiplier
/// `world_pos`      - fragment world position
/// `world_normal`   - fragment world normal
/// `world_reflect`  - reflected view vector
/// `spec_power`     - specular exponent
///
/// Returns vec4: xyz = diffuse contribution, w = specular contribution
fn evaluate_omni_specular(
    light_pos: vec3<f32>,
    light_color: vec3<f32>,
    omni_mul: f32,
    omni_add: f32,
    decay_dist: f32,
    spec_intensity: f32,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    world_reflect: vec3<f32>,
    spec_power: f32,
) -> vec4<f32> {
    let light_vec = light_pos - world_pos;
    let light_vec_len2 = dot(light_vec, light_vec);
    let oo_light_dist = inverseSqrt(max(light_vec_len2, 0.0001));
    let dist = oo_light_dist * light_vec_len2;
    let light_norm = light_vec * oo_light_dist;

    let n_dot_l = smooth_atten(clamp(dot(light_norm, world_normal), 0.0, 1.0));
    let dist_atten = smooth_atten(clamp(dist * omni_mul + omni_add, 0.0, 1.0));
    let decay = clamp(decay_dist * oo_light_dist, 0.0, 1.0);

    let atten = n_dot_l * dist_atten * decay;

    // Specular (Schlick approximation matching original)
    let r_dot_l = max(dot(world_reflect, light_norm), 0.0);
    let denom = spec_power - (spec_power - 1.0) * r_dot_l;
    var spec_contrib = 0.0;
    if (denom > 0.0001) {
        spec_contrib = r_dot_l / denom * spec_intensity;
    }

    let diffuse = atten * light_color;
    return vec4<f32>(diffuse, dot(diffuse, vec3<f32>(0.299, 0.587, 0.114)) * spec_contrib);
}
