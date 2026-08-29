//! Renderer projection for authoritative simulation hint callouts.

use super::projection::{inputs_are_valid, project_visible_world_point};
use glam::Mat4;
use sim::{HintCalloutAnchor, World};

/// One active sim callout projected into the current UI viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectedHintCallout {
    /// Trigger-visible callout identifier.
    pub id: i32,
    /// Retail's reusable UI widget slot.
    pub widget_slot: u8,
    /// Localized string-table identifier for UI text lookup.
    pub string_id: i32,
    /// Pixel position measured from the viewport's top-left corner.
    pub screen_position: [f32; 2],
}

/// Project every visible sim-owned callout without duplicating its lifetime.
#[must_use]
pub fn project_hint_callouts(
    world: &World,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Vec<ProjectedHintCallout> {
    if !inputs_are_valid(view_projection, viewport) {
        return Vec::new();
    }
    world
        .hint_callouts()
        .filter_map(|callout| {
            let position = match callout.anchor() {
                HintCalloutAnchor::Location(position) => position,
                HintCalloutAnchor::Entity(entity_id) => world.entity_position(entity_id)?,
            };
            let screen_position = project_visible_world_point(position, view_projection, viewport)?;
            Some(ProjectedHintCallout {
                id: callout.id(),
                widget_slot: callout.widget_slot(),
                string_id: callout.string_id(),
                screen_position,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn clip_projection_maps_center_and_rejects_invisible_points() {
        let viewport = [1280.0, 720.0];
        assert_eq!(
            project_visible_world_point(Vec3::ZERO, Mat4::IDENTITY, viewport),
            Some([640.0, 360.0])
        );
        assert!(
            project_visible_world_point(Vec3::new(2.0, 0.0, 0.0), Mat4::IDENTITY, viewport)
                .is_none()
        );
        let behind = Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]);
        assert!(project_visible_world_point(Vec3::ZERO, behind, viewport).is_none());
    }
}
