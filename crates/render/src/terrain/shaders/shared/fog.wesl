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

/// Compute planar (height-based) volumetric fog density.
/// Traces the view ray through the fog volume below planar_start height.
/// Returns 1.0 = fully visible, 0.0 = fully fogged.
///
/// HWDE implementation (VS lines 180-199):
/// 1. Computes the direction from vertex to camera
/// 2. Finds the effective distance of the view ray through the fog region
/// 3. Applies squared exponential decay: 2^(-(eff_dist² × density²))
fn compute_planar_fog_density(
    world_pos: vec3<f32>,
    camera_pos: vec3<f32>,
    planar_start: f32,
    planar_density2: f32,
) -> f32 {
    let camera_vec = camera_pos - world_pos;
    let dist = length(camera_vec);
    if dist < 0.001 {
        return 1.0;
    }

    // Vertical component of normalized camera direction
    let camera_y_dir = camera_vec.y / dist;

    // Height of vertex relative to fog plane
    let fog_height = world_pos.y - planar_start;

    // Compute effective fog distance along the view ray
    // Handles camera above/below vertex differently (ray-plane intersection)
    var inv_y_dir = 1.0 / (-camera_y_dir);
    // Clamp to avoid infinity when looking horizontally
    inv_y_dir = clamp(inv_y_dir, -3.4e+38, 3.4e+38);

    var eff_dist: f32;
    if camera_y_dir < 0.0 {
        // Camera is below vertex: compute partial intersection
        eff_dist = dist - fog_height * inv_y_dir;
    } else {
        // Camera is above or level with vertex
        eff_dist = fog_height * inv_y_dir;
    }

    eff_dist = min(dist, max(0.0, eff_dist));

    return clamp(exp2(-(planar_density2 * eff_dist * eff_dist)), 0.0, 1.0);
}
