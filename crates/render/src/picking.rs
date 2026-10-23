//! Conversion between screen positions and world positions.
//!
//! All screen positions are physical pixels with the origin at the top-left corner of the
//! viewport. The projection has the wgpu clip-space depth range 0 to 1. This module holds the
//! only world-to-screen projection of the renderer.

use glam::{Mat4, Vec2, Vec3};
use num_traits::ToPrimitive;

use crate::world_view::WorldCamera;

/// The distance between two terrain samples of [`intersect_terrain`], in world units.
const TERRAIN_MARCH_STEP: f32 = 1.0;

/// The number of bisection steps of [`intersect_terrain`] after the march finds a crossing.
const TERRAIN_BISECTION_STEPS: u32 = 16;

/// A half-line in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Start point of the ray.
    pub origin: Vec3,
    /// Unit direction of the ray.
    pub direction: Vec3,
}

impl Ray {
    /// Returns the point at `distance` along the ray.
    #[must_use]
    pub fn at(&self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }
}

/// Returns the ray from the near plane through the screen position `screen`.
///
/// Returns `None` if the camera matrix cannot be inverted or the viewport is empty.
#[must_use]
pub fn screen_ray(camera: &WorldCamera, screen: Vec2) -> Option<Ray> {
    let viewport = viewport_size(camera)?;
    if !screen.is_finite() || !inputs_are_valid(camera.view_projection, viewport) {
        return None;
    }
    let inverse = camera.view_projection.inverse();
    if !inverse.is_finite() {
        return None;
    }
    let ndc = Vec2::new(
        screen.x / viewport[0] * 2.0 - 1.0,
        1.0 - screen.y / viewport[1] * 2.0,
    );
    let near = inverse.project_point3(ndc.extend(0.0));
    let far = inverse.project_point3(ndc.extend(1.0));
    let direction = (far - near).try_normalize()?;
    near.is_finite().then_some(Ray {
        origin: near,
        direction,
    })
}

/// Returns the screen position of the world point `position`.
///
/// The position can be outside the viewport. Returns `None` if the point is behind the camera
/// or the inputs are not finite.
#[must_use]
pub fn project_point(camera: &WorldCamera, position: Vec3) -> Option<Vec2> {
    let viewport = viewport_size(camera)?;
    let (ndc, in_front) = project_ndc(position, camera.view_projection, viewport)?;
    in_front.then(|| Vec2::from(ndc_to_pixels(ndc.truncate(), viewport)))
}

/// Returns the first point where `ray` goes below the terrain, within `max_distance`.
///
/// `height` returns the terrain height at the X and Z of a point, or `None` outside the
/// terrain. The function marches along the ray in steps of 1 world unit, then bisects the
/// step that crosses the terrain 16 times. Returns the ray origin if the origin
/// is already below the terrain, and `None` if the ray does not cross the terrain.
pub fn intersect_terrain(
    ray: &Ray,
    max_distance: f32,
    height: impl Fn(Vec3) -> Option<f32>,
) -> Option<Vec3> {
    let direction = ray.direction.try_normalize()?;
    if !ray.origin.is_finite() || !max_distance.is_finite() || max_distance < 0.0 {
        return None;
    }
    let ray = Ray {
        origin: ray.origin,
        direction,
    };
    let below = |distance: f32| {
        let point = ray.at(distance);
        height(point).map(|ground| point.y <= ground)
    };
    if below(0.0) == Some(true) {
        return Some(ray.origin);
    }
    let step_count = (max_distance / TERRAIN_MARCH_STEP).ceil().to_u32()?;
    let mut above_distance = 0.0;
    for step in 1..=step_count {
        let distance = (step.to_f32()? * TERRAIN_MARCH_STEP).min(max_distance);
        if below(distance) == Some(true) {
            return Some(ray.at(bisect(above_distance, distance, below)));
        }
        // A sample outside the terrain (`None`) also counts as above it, so that the bisection
        // starts from a point that is not below the terrain.
        above_distance = distance;
    }
    None
}

/// Narrows a crossing between `above` (above the terrain) and `under` (below the terrain).
fn bisect(mut above: f32, mut under: f32, below: impl Fn(f32) -> Option<bool>) -> f32 {
    for _ in 0..TERRAIN_BISECTION_STEPS {
        let middle = f32::midpoint(above, under);
        if below(middle) == Some(true) {
            under = middle;
        } else {
            above = middle;
        }
    }
    under
}

fn viewport_size(camera: &WorldCamera) -> Option<[f32; 2]> {
    Some([camera.viewport[0].to_f32()?, camera.viewport[1].to_f32()?])
}

pub(crate) fn inputs_are_valid(view_projection: Mat4, viewport: [f32; 2]) -> bool {
    view_projection.is_finite()
        && viewport[0].is_finite()
        && viewport[1].is_finite()
        && viewport[0] > 0.0
        && viewport[1] > 0.0
}

/// Returns the normalized device coordinates of `position` and if it is in front of the
/// camera. Behind the camera, the X and Y signs are flipped so that the direction to the point
/// stays correct on the screen.
fn project_ndc(position: Vec3, view_projection: Mat4, viewport: [f32; 2]) -> Option<(Vec3, bool)> {
    if !position.is_finite() || !inputs_are_valid(view_projection, viewport) {
        return None;
    }
    let clip = view_projection * position.extend(1.0);
    if !clip.is_finite() || clip.w.abs() <= f32::EPSILON {
        return None;
    }
    let in_front = clip.w > 0.0;
    let mut ndc = clip.truncate() / clip.w.abs();
    if !in_front {
        ndc.x = -ndc.x;
        ndc.y = -ndc.y;
    }
    Some((ndc, in_front))
}

fn ndc_to_pixels(ndc: Vec2, viewport: [f32; 2]) -> [f32; 2] {
    [
        f32::midpoint(ndc.x, 1.0) * viewport[0],
        f32::midpoint(-ndc.y, 1.0) * viewport[1],
    ]
}

pub(crate) fn project_visible_world_point(
    position: Vec3,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Option<[f32; 2]> {
    let (position, target_on_screen) = project_world_target(position, view_projection, viewport)?;
    target_on_screen.then_some(position)
}

/// Returns the screen position of `target` and if it is on the screen. An off-screen target
/// moves to the edge of the view in its screen direction.
pub(crate) fn project_world_target(
    target: Vec3,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Option<([f32; 2], bool)> {
    let (ndc, in_front) = project_ndc(target, view_projection, viewport)?;
    let target_on_screen = in_front
        && (-1.0..=1.0).contains(&ndc.x)
        && (-1.0..=1.0).contains(&ndc.y)
        && (-1.0..=1.0).contains(&ndc.z);
    let position = if target_on_screen {
        ndc.truncate()
    } else {
        clamp_to_view_edge(ndc.truncate())
    };
    Some((ndc_to_pixels(position, viewport), target_on_screen))
}

fn clamp_to_view_edge(mut direction: Vec2) -> Vec2 {
    if direction.length_squared() <= f32::EPSILON {
        direction = Vec2::Y;
    }
    let largest_axis = direction.x.abs().max(direction.y.abs());
    direction * (0.92 / largest_axis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perspective_camera() -> WorldCamera {
        let position = Vec3::new(10.0, 50.0, 30.0);
        let world_to_view = Mat4::look_at_lh(position, Vec3::new(10.0, 0.0, 0.0), Vec3::Y);
        let projection = Mat4::perspective_lh(1.0, 1.5, 0.5, 2_000.0);
        WorldCamera {
            view_projection: projection * world_to_view,
            world_to_view,
            position,
            depth_unproject: [0.0, 1.0],
            viewport: [1_200, 800],
        }
    }

    #[test]
    fn visible_and_clamped_projection_share_pixel_mapping() {
        let viewport = [1_000.0, 500.0];
        assert_eq!(
            project_visible_world_point(Vec3::ZERO, Mat4::IDENTITY, viewport),
            Some([500.0, 250.0])
        );
        let (position, on_screen) =
            project_world_target(Vec3::new(4.0, 1.0, 0.0), Mat4::IDENTITY, viewport).unwrap();
        assert!((position[0] - 960.0).abs() < 0.001);
        assert!((position[1] - 192.5).abs() < 0.001);
        assert!(!on_screen);
    }

    #[test]
    fn screen_ray_and_projection_are_inverse() {
        let camera = perspective_camera();
        let target = Vec3::new(14.0, 2.0, -6.0);
        let screen = project_point(&camera, target).expect("target is in front");
        let ray = screen_ray(&camera, screen).expect("camera is invertible");
        assert!((ray.direction.length() - 1.0).abs() < 1e-5);
        let to_target = (target - ray.origin).normalize();
        assert!(ray.direction.dot(to_target) > 0.999_999);

        let center = screen_ray(&camera, Vec2::new(600.0, 400.0)).unwrap();
        let forward = (Vec3::new(10.0, 0.0, 0.0) - camera.position).normalize();
        assert!(center.direction.dot(forward) > 0.999_999);
    }

    #[test]
    fn projection_maps_top_left_origin_and_rejects_points_behind() {
        let camera = WorldCamera {
            view_projection: Mat4::IDENTITY,
            world_to_view: Mat4::IDENTITY,
            position: Vec3::ZERO,
            depth_unproject: [0.0, 1.0],
            viewport: [200, 100],
        };
        assert_eq!(
            project_point(&camera, Vec3::new(-1.0, 1.0, 0.5)),
            Some(Vec2::new(0.0, 0.0))
        );
        // Off-screen points in front keep their unclamped position.
        assert_eq!(
            project_point(&camera, Vec3::new(3.0, 0.0, 0.5)),
            Some(Vec2::new(400.0, 50.0))
        );
        let behind = perspective_camera();
        assert_eq!(project_point(&behind, Vec3::new(10.0, 60.0, 80.0)), None);
        let empty = WorldCamera {
            viewport: [0, 100],
            ..camera
        };
        assert_eq!(project_point(&empty, Vec3::ZERO), None);
        assert_eq!(screen_ray(&empty, Vec2::ZERO), None);
    }

    #[test]
    fn terrain_intersection_marches_and_bisects() {
        // A plane that rises 0.5 units for each unit of X.
        let slope = |point: Vec3| Some(point.x * 0.5);
        let ray = Ray {
            origin: Vec3::new(0.0, 20.0, 0.0),
            direction: Vec3::new(1.0, -1.0, 0.0).normalize(),
        };
        let hit = intersect_terrain(&ray, 100.0, slope).expect("the ray crosses the plane");
        // y = 20 - x and y = x / 2 cross at x = 40 / 3.
        assert!((hit.x - 40.0 / 3.0).abs() < 1e-3);
        assert!((hit.y - hit.x * 0.5).abs() < 1e-3);

        assert_eq!(intersect_terrain(&ray, 5.0, slope), None);
        let upward = Ray {
            direction: Vec3::Y,
            ..ray
        };
        assert_eq!(intersect_terrain(&upward, 100.0, slope), None);
        let buried = Ray {
            origin: Vec3::new(10.0, 1.0, 0.0),
            ..ray
        };
        assert_eq!(
            intersect_terrain(&buried, 100.0, slope),
            Some(buried.origin)
        );
    }

    #[test]
    fn terrain_intersection_skips_samples_outside_the_terrain() {
        // The terrain exists only for X >= 10, at height 0.
        let bounded = |point: Vec3| (point.x >= 10.0).then_some(0.0);
        let ray = Ray {
            origin: Vec3::new(0.0, 10.0, 0.0),
            direction: Vec3::new(1.0, -0.5, 0.0).normalize(),
        };
        let hit = intersect_terrain(&ray, 100.0, bounded).expect("the ray reaches the terrain");
        assert!((hit.x - 20.0).abs() < 1e-3);
        assert!(hit.y.abs() < 1e-3);
    }
}
