//! Stateless viewport projection for simulation-owned icon objects.

use super::projection::{inputs_are_valid, project_world_target};
use glam::Mat4;
use sim::{EntityId, PlayerId, World};

/// One authoritative class-zero icon projected into the local viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedIconObject {
    /// Trigger-addressable class-zero object identity.
    pub entity_id: EntityId,
    /// Proto-object name used by a complete minimap UI to resolve its artwork.
    pub prototype_name: String,
    /// Pixel position measured from the viewport's top-left corner.
    pub screen_position: [f32; 2],
    /// Whether the icon's world position lies inside the current camera frustum.
    pub target_on_screen: bool,
    /// Runtime RGB override, or `None` when prototype/player color applies.
    pub color_override: Option<[u8; 3]>,
    /// Parent object for icons created as retail attachments.
    pub attached_to: Option<EntityId>,
}

/// Project every icon visible to `viewer_id` without retaining game state.
#[must_use]
pub fn project_icon_objects(
    world: &World,
    viewer_id: PlayerId,
    view_projection: Mat4,
    viewport: [f32; 2],
) -> Vec<ProjectedIconObject> {
    if !inputs_are_valid(view_projection, viewport) {
        return Vec::new();
    }
    world
        .icon_objects()
        .filter(|(icon_id, _)| world.is_icon_visible_to_player(*icon_id, viewer_id))
        .filter_map(|(icon_id, icon)| {
            let object = world.get_object(icon_id)?;
            let (screen_position, target_on_screen) =
                project_world_target(object.base.position, view_projection, viewport)?;
            Some(ProjectedIconObject {
                entity_id: icon_id,
                prototype_name: object.proto_object_name.clone(),
                screen_position,
                target_on_screen,
                color_override: icon.color_override(),
                attached_to: object.object_state.attached_to(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use pipeline::database::hw1::{Database, ProtoObject};

    fn database() -> Database {
        Database {
            objects: vec![ProtoObject {
                name: "sys_icon_test".to_owned(),
                dbid: Some(42),
                object_class: Some("Object".to_owned()),
                object_types: vec!["Icon".to_owned()],
                flags: vec!["VisibleForOwnerOnly".to_owned()],
                ..ProtoObject::default()
            }],
            ..Database::default()
        }
    }

    #[test]
    fn projection_consumes_sim_visibility_color_and_position() {
        let database = database();
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        let icon_id = world
            .create_icon_object(&database, 1, 42, Vec3::ZERO, Some([255, 128, 0]), false)
            .unwrap();

        let projected = project_icon_objects(&world, 1, Mat4::IDENTITY, [1_000.0, 500.0]);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].entity_id, icon_id);
        for (actual, expected) in projected[0].screen_position.into_iter().zip([500.0, 250.0]) {
            assert!((actual - expected).abs() < 0.001);
        }
        assert_eq!(projected[0].color_override, Some([255, 128, 0]));
        assert!(projected[0].target_on_screen);
        assert!(project_icon_objects(&world, 2, Mat4::IDENTITY, [1_000.0, 500.0]).is_empty());
    }
}
