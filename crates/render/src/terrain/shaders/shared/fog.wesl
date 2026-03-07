// Shared fog helpers - matches fogHelpers.inc
//
// Dual-mode fog: radial (distance-based) and planar (height-based).
// All parameters are passed explicitly so the functions are independent
// of any particular uniform layout.

/// Compute radial (distance-based) fog density.
/// Returns 1.0 = fully visible, 0.0 = fully fogged.
fn compute_radial_fog_density(
    world_pos: vec3<f32>,
    camera_pos: vec3<f32>,
    fog_density2: f32,
    fog_start2: f32,
) -> f32 {
    let camera_vec = camera_pos - world_pos;
    let dist2 = dot(camera_vec, camera_vec);
    return clamp(exp2(-(fog_density2 * max(0.0, dist2 - fog_start2))), 0.0, 1.0);
}

/// Apply radial + planar fog to a lit color.
/// z_fog = radial fog density (1 = no fog), p_fog = planar fog density (1 = no fog).
fn compute_fog(
    result: vec3<f32>,
    z_fog: f32,
    p_fog: f32,
    fog_color: vec3<f32>,
    planar_fog_color: vec3<f32>,
) -> vec3<f32> {
    var color = mix(planar_fog_color, result, p_fog);
    return mix(fog_color, color, z_fog);
}

/// Compute planar (height-based) fog density.
/// Returns 1.0 = fully visible, 0.0 = fully fogged.
fn compute_planar_fog_density(
    world_y: f32,
    planar_start: f32,
    planar_density2: f32,
) -> f32 {
    let pd = max(0.0, planar_start - world_y);
    return clamp(exp2(-(planar_density2 * pd * pd)), 0.0, 1.0);
}
