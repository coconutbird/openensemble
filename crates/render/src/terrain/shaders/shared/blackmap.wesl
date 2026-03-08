// Shared blackmap (fog-of-war) - ported from blackmap.inc
//
// The blackmap system controls terrain visibility based on exploration state:
//   - Layer 0: Fully visible (explored + currently visible)
//   - Layer 1: Explored but not currently visible (darkened by fog_scalar)
//   - Layer 2: Unexplored (darkened further, with unexplored mask texture)
//
// The visibility factor is stored in the alpha channel of the blackmap texture.
// Values 0.0-0.5 interpolate between layer 0 and layer 1.
// Values 0.5-1.0 interpolate between layer 1 and layer 2.
//
// A background lerp factor clips the effect at world bounds edges,
// fading to a solid background color outside the playable area.
//
// All parameters are passed explicitly so the functions are independent
// of any particular uniform layout.

/// Compute blackmap brightness and background lerp scalars.
///
/// `world_pos`   - fragment world position
/// `vis_factor`  - visibility texture alpha sample (0 = visible, 1 = unexplored)
/// `fog_scalar`  - brightness multiplier for fog-of-war layer (blackmap_params0.w)
/// `unexplored_scalar` - brightness multiplier for unexplored layer (blackmap_params1.x)
/// `bounds_lo`   - world XZ lower bounds (blackmap_params1.yz)
/// `bounds_hi`   - world XZ upper bounds (blackmap_params2.yz)
/// `bounds_falloff` - inverse falloff distance at bounds edges (blackmap_params2.w)
///
/// Returns vec2: x = brightness scalar, y = background lerp factor
fn compute_blackmap_scalar(
    world_pos: vec3<f32>,
    vis_factor: f32,
    fog_scalar: f32,
    unexplored_scalar: f32,
    bounds_lo: vec2<f32>,
    bounds_hi: vec2<f32>,
    bounds_falloff: f32,
) -> vec2<f32> {
    // Distance from bounds edges (clamped to 0..1)
    let lo_dist = bounds_lo - vec2<f32>(world_pos.x, world_pos.z);
    let hi_dist = vec2<f32>(world_pos.x, world_pos.z) - bounds_hi;
    let dist = vec2<f32>(1.0) - clamp(max(lo_dist, hi_dist) * bounds_falloff, vec2<f32>(0.0), vec2<f32>(1.0));
    let background_lerp = dist.x * dist.y;

    // Three-layer brightness interpolation
    var brightness: f32;
    if vis_factor <= 0.5 {
        brightness = mix(1.0, fog_scalar, vis_factor * 2.0);
    } else {
        brightness = mix(fog_scalar, unexplored_scalar, (vis_factor - 0.5) * 2.0);
    }

    return vec2<f32>(brightness, background_lerp);
}

/// Apply blackmap color modification including unexplored mask texture.
///
/// `color`            - input lit color
/// `world_pos`        - fragment world position
/// `vis_factor`       - visibility texture alpha sample
/// `unexplored_color` - sampled unexplored mask texture (RGBA)
/// `bg_color`         - background color outside playable area (blackmap_params0.xyz)
/// `fog_scalar`       - brightness for fog-of-war layer
/// `unexplored_scalar`- brightness for unexplored layer
/// `bounds_lo`        - world XZ lower bounds
/// `bounds_hi`        - world XZ upper bounds
/// `bounds_falloff`   - inverse falloff distance
fn apply_blackmap(
    color: vec3<f32>,
    world_pos: vec3<f32>,
    vis_factor: f32,
    unexplored_color: vec4<f32>,
    bg_color: vec3<f32>,
    fog_scalar: f32,
    unexplored_scalar: f32,
    bounds_lo: vec2<f32>,
    bounds_hi: vec2<f32>,
    bounds_falloff: f32,
) -> vec3<f32> {
    // Background lerp from bounds
    let lo_dist = bounds_lo - vec2<f32>(world_pos.x, world_pos.z);
    let hi_dist = vec2<f32>(world_pos.x, world_pos.z) - bounds_hi;
    let dist = vec2<f32>(1.0) - clamp(max(lo_dist, hi_dist) * bounds_falloff, vec2<f32>(0.0), vec2<f32>(1.0));
    let background_lerp = dist.x * dist.y;

    // Three-layer color blending
    let layer0 = color;
    let layer1 = color * fog_scalar;
    // Unexplored: blend with unexplored mask texture, then darken
    let layer2 = mix(color, unexplored_color.rgb, unexplored_color.a) * unexplored_scalar;

    var result: vec3<f32>;
    if vis_factor <= 0.5 {
        result = mix(layer0, layer1, vis_factor * 2.0);
    } else {
        result = mix(layer1, layer2, (vis_factor - 0.5) * 2.0);
    }

    // Fade to background color outside playable bounds
    return mix(bg_color, result, background_lerp);
}
