//! Stateless screen projection for simulation-owned objective pointers.

use super::projection::{inputs_are_valid, project_world_target};
use glam::Mat4;
use sim::{PlayerId, World};

/// One authoritative objective target projected into a local viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectedObjectivePointer {
    /// Retail UI widget receiving the pointer.
    pub widget_id: i32,
    /// Pixel position measured from the viewport's top-left corner.
    pub screen_position: [f32; 2],
    /// Whether the target itself lies inside the current camera frustum.
    pub target_on_screen: bool,
    /// Retail's authored target-use flag.
    pub use_target: bool,
    /// Retail's authored visibility override.
    pub force_target_visible: bool,
}

/// Project one player's current objective pointers without retaining game state.
#[must_use]
pub fn project_objective_pointers(
    world: &World,
    player_id: PlayerId,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Vec<ProjectedObjectivePointer> {
    if !inputs_are_valid(view_projection, viewport) {
        return Vec::new();
    }
    world
        .objective_pointers(player_id)
        .filter_map(|pointer| {
            let (screen_position, target_on_screen) =
                project_world_target(pointer.target_position(), view_projection, viewport)?;
            Some(ProjectedObjectivePointer {
                widget_id: pointer.widget_id(),
                screen_position,
                target_on_screen,
                use_target: pointer.use_target(),
                force_target_visible: pointer.force_target_visible(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn projection_clamps_offscreen_targets() {
        let viewport = [1_000.0, 500.0];
        assert_eq!(
            project_world_target(Vec3::ZERO, Mat4::IDENTITY, viewport),
            Some(([500.0, 250.0], true))
        );
        let (position, on_screen) =
            project_world_target(Vec3::new(4.0, 1.0, 0.0), Mat4::IDENTITY, viewport).unwrap();
        assert!((position[0] - 960.0).abs() < 0.001);
        assert!((position[1] - 192.5).abs() < 0.001);
        assert!(!on_screen);
    }

    #[test]
    fn invalid_viewports_do_not_project() {
        let world = World::new();
        assert!(project_objective_pointers(&world, 1, Mat4::IDENTITY, [0.0, 720.0]).is_empty());
    }
}
