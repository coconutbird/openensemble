//! Authoritative handling of animation-authored rigid-body impulses.

use super::helpers::{animation_anchor_world_transform, unit_world_transform};
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackAnimationAnchor, AttackAnimationEvent, AttackAnimationEventKind, PhysicsImpulseEvent,
};
use crate::world::World;
use glam::Vec3;

impl World {
    pub(super) fn apply_physics_impulse_event(
        &mut self,
        source_id: EntityId,
        event: &AttackAnimationEvent,
    ) -> bool {
        let AttackAnimationEventKind::PhysicsImpulse(impulse) = &event.kind else {
            return false;
        };
        self.apply_animation_physics_impulse(source_id, event.anchor.as_ref(), impulse);
        true
    }

    pub(super) fn apply_animation_physics_impulse(
        &mut self,
        source_id: EntityId,
        anchor: Option<&AttackAnimationAnchor>,
        event: &PhysicsImpulseEvent,
    ) {
        let Some((body_id, frame)) = self.units.get(source_id).and_then(|source| {
            let body_id = if event.attached_to_object {
                source.object_state.attached_to()?
            } else {
                source_id
            };
            let frame = anchor
                .and_then(|anchor| animation_anchor_world_transform(source, anchor))
                .or_else(|| unit_world_transform(source))?;
            Some((body_id, frame))
        }) else {
            return;
        };
        let Some(mass) = self.unit_body_mass(body_id) else {
            return;
        };
        let impulse = frame.transform_vector3(Vec3::from_array(event.force)) * mass;
        let _applied = match event.impulse_type {
            0 => self.apply_unit_angular_impulse(body_id, impulse),
            1 => self.apply_unit_impulse(body_id, impulse),
            2 => self.apply_unit_point_impulse(body_id, impulse, frame.w_axis.truncate()),
            _ => false,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Unit;
    use crate::physics::test_support::box_template;
    use glam::Mat4;

    #[test]
    fn local_impulse_uses_the_posed_bone_frame_and_body_mass() {
        let mut world = World::new();
        let unit_id = dynamic_unit(&mut world, Vec3::X);
        let anchor = AttackAnimationAnchor {
            links: Vec::new(),
            bone_to_component: Some(Mat4::from_translation(Vec3::Y)),
            single_bone_poses: Vec::new(),
        };

        world.apply_animation_physics_impulse(
            unit_id,
            Some(&anchor),
            &PhysicsImpulseEvent {
                to_bone: Some("recoil".to_owned()),
                impulse_type: 1,
                force: [0.0, 0.0, 2.0],
                attached_to_object: false,
            },
        );

        let velocity = world.unit_body_linear_velocity(unit_id).unwrap();
        assert!(velocity.abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1.0e-6));
    }

    #[test]
    fn attached_flag_redirects_the_impulse_to_the_parent_body() {
        let mut world = World::new();
        let parent_id = dynamic_unit(&mut world, Vec3::Z);
        let child_id = world.units.allocate_id();
        let mut child = Unit::new(child_id, 1);
        child.object_state.set_attached_to(Some(parent_id));
        world.units.insert(child_id, child);

        world.apply_animation_physics_impulse(
            child_id,
            None,
            &PhysicsImpulseEvent {
                to_bone: None,
                impulse_type: 1,
                force: [1.0, 0.0, 0.0],
                attached_to_object: true,
            },
        );

        assert_eq!(world.unit_body(child_id), None);
        let velocity = world.unit_body_linear_velocity(parent_id).unwrap();
        assert!(velocity.abs_diff_eq(Vec3::X, 1.0e-6));
    }

    fn dynamic_unit(world: &mut World, forward: Vec3) -> EntityId {
        let id = world.create_unit_at(1, Vec3::ZERO);
        world.units.get_mut(id).unwrap().base.set_forward(forward);
        let template = box_template("box", Vec3::ONE, 10.0, Vec3::ZERO);
        assert!(world.create_physics_object(id, &template, false));
        id
    }
}
