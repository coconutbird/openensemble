//! The retail ground orientation math of the ground tie (`BEntity_orientWithGround` and
//! `BEntity_filterOrientation`).

use glam::Vec3;

/// Half width of the ground footprint along the right axis (`BEntity_orientWithGround`).
const ORIENT_HALF_WIDTH: f32 = 1.25;
/// Half length of the ground footprint along the forward axis (`BEntity_orientWithGround`).
const ORIENT_HALF_LENGTH: f32 = 2.25;
/// Weight of the new frame in `BEntity_filterOrientation` @ `0x1402035F0`.
const ORIENT_FILTER_WEIGHT: f32 = 0.1;

/// Terrain height and orientation under an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroundSample {
    /// Mean terrain height of the four footprint samples.
    pub height: f32,
    /// Unit terrain normal with a positive Y component.
    pub normal: Vec3,
}

/// Sample the terrain under an entity like `BEntity_orientWithGround` @ `0x1402032A0`.
///
/// Retail samples the clamped terrain height at the four corners `±1.25 right ± 2.25 forward`.
/// The height is the mean of the four samples. The normal is the cross product of the diagonal
/// with the higher mean height and half of the other diagonal, turned to point up.
pub(crate) fn sample_ground(
    position: Vec3,
    heading: Vec3,
    mut height_at: impl FnMut(Vec3) -> Option<f32>,
) -> Option<GroundSample> {
    let forward = planar(heading).try_normalize().unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(forward);
    let mut corner = |side: f32, along: f32| {
        let point =
            position + right * (side * ORIENT_HALF_WIDTH) + forward * (along * ORIENT_HALF_LENGTH);
        height_at(point).map(|height| Vec3::new(point.x, height, point.z))
    };
    let front_right = corner(1.0, 1.0)?;
    let back_right = corner(1.0, -1.0)?;
    let front_left = corner(-1.0, 1.0)?;
    let back_left = corner(-1.0, -1.0)?;
    let first_mean = f32::midpoint(front_right.y, back_left.y);
    let second_mean = f32::midpoint(back_right.y, front_left.y);
    let (diagonal, other) = if first_mean >= second_mean {
        (front_right - back_left, (front_left - back_right) * 0.5)
    } else {
        (back_right - front_left, (back_left - front_right) * 0.5)
    };
    let mut normal = diagonal.cross(other);
    if normal.y < 0.0 {
        normal = -normal;
    }
    let normal = normal.try_normalize().unwrap_or(Vec3::Y);
    let height = (front_right.y + back_right.y + front_left.y + back_left.y) * 0.25;
    height
        .is_finite()
        .then_some(GroundSample { height, normal })
}

/// Return the right and forward axes of a frame with this up axis and planar heading.
///
/// This is the projection in `BEntity_orientWithGround`: `right = up × heading`,
/// `forward = right × up`.
pub(crate) fn ground_frame(up: Vec3, heading: Vec3) -> (Vec3, Vec3) {
    let heading = planar(heading).try_normalize().unwrap_or(Vec3::Z);
    let right = up.cross(heading).try_normalize().unwrap_or(Vec3::X);
    let forward = right.cross(up).try_normalize().unwrap_or(heading);
    (right, forward)
}

/// Blend the previous up axis toward a new terrain normal like `BEntity_filterOrientation`.
///
/// Retail blends the right and forward axes with weight 0.1 for the new frame. It then sets
/// `up = forward × right`. Retail skips the filter when the previous up axis is exactly world
/// up (`BEntity_tieToGround` @ `0x140202EB0`).
pub(crate) fn filter_up(previous_up: Vec3, normal: Vec3, heading: Vec3) -> Vec3 {
    if previous_up.y.to_bits() == 1.0_f32.to_bits() {
        return normal;
    }
    let (previous_right, previous_forward) = ground_frame(previous_up, heading);
    let (right, forward) = ground_frame(normal, heading);
    let right = right * ORIENT_FILTER_WEIGHT + previous_right * (1.0 - ORIENT_FILTER_WEIGHT);
    let forward = forward * ORIENT_FILTER_WEIGHT + previous_forward * (1.0 - ORIENT_FILTER_WEIGHT);
    forward.cross(right).try_normalize().unwrap_or(normal)
}

fn planar(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOPE: f32 = 0.25;

    fn ramp(point: Vec3) -> f32 {
        SLOPE * point.x
    }

    fn ramp_normal() -> Vec3 {
        Vec3::new(-SLOPE, 1.0, 0.0).normalize()
    }

    fn assert_vec_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < 0.000_1,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn ground_sample_reproduces_retail_footprint_on_a_plane() {
        let sample = sample_ground(
            Vec3::new(8.0, 50.0, 3.0),
            Vec3::new(1.0, 0.4, 1.0),
            |point| Some(ramp(point)),
        )
        .expect("the ramp has a height everywhere");

        assert!((sample.height - SLOPE * 8.0).abs() < 0.000_1);
        assert_vec_close(sample.normal, ramp_normal());
    }

    #[test]
    fn ground_sample_uses_the_higher_diagonal_on_a_ridge() {
        // A ridge along the heading: only the higher diagonal sets the normal.
        let ridge = |point: Vec3| Some(if point.x > 0.0 { 0.0 } else { 1.0 });
        let sample = sample_ground(Vec3::ZERO, Vec3::Z, ridge).expect("ridge height");

        assert!((sample.height - 0.5).abs() < 0.000_1);
        assert!(sample.normal.y > 0.0);
        assert!(sample.normal.is_normalized());
    }

    #[test]
    fn ground_frame_projects_the_heading_onto_the_terrain() {
        let (right, forward) = ground_frame(ramp_normal(), Vec3::X);

        assert!(forward.dot(ramp_normal()).abs() < 0.000_1);
        assert!(forward.x > 0.0 && forward.y > 0.0);
        assert_vec_close(right, Vec3::NEG_Z);
    }

    #[test]
    fn filter_snaps_from_world_up_and_blends_after() {
        assert_eq!(filter_up(Vec3::Y, ramp_normal(), Vec3::X), ramp_normal());

        let tilted = Vec3::new(0.0, 1.0, 0.25).normalize();
        let blended = filter_up(tilted, ramp_normal(), Vec3::X);
        assert!(blended.is_normalized());
        assert!(blended.angle_between(tilted) < blended.angle_between(ramp_normal()));
        assert!(blended.angle_between(ramp_normal()) < tilted.angle_between(ramp_normal()));
    }
}
