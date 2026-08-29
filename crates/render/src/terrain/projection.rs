//! Shared world-to-viewport projection used by simulation UI adapters.

use glam::{Mat4, Vec2, Vec3};

pub(super) fn inputs_are_valid(view_projection: Mat4, viewport: [f32; 2]) -> bool {
    view_projection.is_finite()
        && viewport[0].is_finite()
        && viewport[1].is_finite()
        && viewport[0] > 0.0
        && viewport[1] > 0.0
}

pub(super) fn project_visible_world_point(
    position: Vec3,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Option<[f32; 2]> {
    let (position, target_on_screen) = project_world_target(position, view_projection, viewport)?;
    target_on_screen.then_some(position)
}

pub(super) fn project_world_target(
    target: Vec3,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Option<([f32; 2], bool)> {
    if !target.is_finite() || !inputs_are_valid(view_projection, viewport) {
        return None;
    }
    let clip = view_projection * target.extend(1.0);
    if !clip.is_finite() || clip.w.abs() <= f32::EPSILON {
        return None;
    }
    let in_front = clip.w > 0.0;
    let mut ndc = clip.truncate() / clip.w.abs();
    if !in_front {
        ndc.x = -ndc.x;
        ndc.y = -ndc.y;
    }
    let target_on_screen = in_front
        && (-1.0..=1.0).contains(&ndc.x)
        && (-1.0..=1.0).contains(&ndc.y)
        && (-1.0..=1.0).contains(&ndc.z);
    let position = if target_on_screen {
        ndc.truncate()
    } else {
        clamp_to_view_edge(ndc.truncate())
    };
    Some((
        [
            f32::midpoint(position.x, 1.0) * viewport[0],
            f32::midpoint(-position.y, 1.0) * viewport[1],
        ],
        target_on_screen,
    ))
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
}
