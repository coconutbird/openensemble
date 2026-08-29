//! Renderer projection for authoritative simulation hint callouts.

use glam::{Mat4, Vec3};
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
    if !view_projection.is_finite()
        || !viewport[0].is_finite()
        || !viewport[1].is_finite()
        || viewport[0] <= 0.0
        || viewport[1] <= 0.0
    {
        return Vec::new();
    }
    world
        .hint_callouts()
        .filter_map(|callout| {
            let position = match callout.anchor() {
                HintCalloutAnchor::Location(position) => position,
                HintCalloutAnchor::Entity(entity_id) => world.entity_position(entity_id)?,
            };
            let screen_position = project_world_point(position, view_projection, viewport)?;
            Some(ProjectedHintCallout {
                id: callout.id(),
                widget_slot: callout.widget_slot(),
                string_id: callout.string_id(),
                screen_position,
            })
        })
        .collect()
}

fn project_world_point(
    position: Vec3,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Option<[f32; 2]> {
    if !position.is_finite() {
        return None;
    }
    let clip = view_projection * position.extend(1.0);
    if !clip.is_finite() || clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if !(-1.0..=1.0).contains(&ndc.x)
        || !(-1.0..=1.0).contains(&ndc.y)
        || !(-1.0..=1.0).contains(&ndc.z)
    {
        return None;
    }
    Some([
        f32::midpoint(ndc.x, 1.0) * viewport[0],
        f32::midpoint(-ndc.y, 1.0) * viewport[1],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_projection_maps_center_and_rejects_invisible_points() {
        let viewport = [1280.0, 720.0];
        assert_eq!(
            project_world_point(Vec3::ZERO, Mat4::IDENTITY, viewport),
            Some([640.0, 360.0])
        );
        assert!(project_world_point(Vec3::new(2.0, 0.0, 0.0), Mat4::IDENTITY, viewport).is_none());
        let behind = Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]);
        assert!(project_world_point(Vec3::ZERO, behind, viewport).is_none());
    }
}
