//! The readback of the bodies into their units (`BUnitActionPhysics_update`, `0x140547880`,
//! 2008 `unitactionphysics.cpp:187-625`) and the move rules of the physics vehicle squads
//! (2008 `squadactionmove.cpp`).

use glam::{Quat, Vec3};

use crate::entities::UnitState;
use crate::entity_id::EntityId;
use crate::world::World;

/// The pose of a unit that the readback takes from its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BodyPose {
    /// The unit position: the body origin minus the rotated `CenterOffset`.
    pub(crate) position: Vec3,
    /// The body Z axis.
    pub(crate) forward: Vec3,
    /// The body Y axis.
    pub(crate) up: Vec3,
    /// The linear velocity of the body.
    pub(crate) velocity: Vec3,
}

/// The unit pose of a body: `position = body position − R·centerOffset`, the forward and up
/// axes of the body rotation and the body velocity.
#[must_use]
pub(crate) fn unit_pose(
    body_position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    center_offset: Vec3,
) -> BodyPose {
    BodyPose {
        position: body_position - rotation * center_offset,
        forward: rotation * Vec3::Z,
        up: rotation * Vec3::Y,
        velocity,
    }
}

impl World {
    /// Runs the `BUnitActionPhysics` update of each unit with a dynamic body, in entity id
    /// order. Retail runs it in the unit updates, before the physics step, so it reads the body
    /// state of the last step.
    ///
    /// - An active body moves its unit (`setPosition(pos, false)`) and gives the unit its axes
    ///   and velocity. An early terminate request then stops and deactivates the body.
    /// - An inactive body clears the early terminate flags, ends the move of the unit if
    ///   `EndMoveOnInactivePhysics` is set, and moves a `CompleteOnInactivePhysics` action to
    ///   its wait state.
    pub(crate) fn read_back_physics(&mut self) {
        let world = self.physics.world_mut();
        for (_, unit) in self.units.iter_mut() {
            let Some(state) = unit.physics_object.as_mut() else {
                continue;
            };
            if state.fixed || state.flags.waiting {
                continue;
            }
            let body = state.body;
            if world.is_active(body) == Some(true) {
                let (Some(position), Some(rotation), Some(velocity)) = (
                    world.position(body),
                    world.rotation(body),
                    world.linear_velocity(body),
                ) else {
                    continue;
                };
                let pose = unit_pose(position, rotation, velocity, state.center_offset);
                unit.base.position = pose.position;
                unit.base.forward = pose.forward;
                unit.base.up = pose.up;
                unit.base.velocity = pose.velocity;
                if state.flags.early_terminate {
                    world.set_linear_velocity(body, Vec3::ZERO);
                    world.set_angular_velocity(body, Vec3::ZERO);
                    world.deactivate(body);
                    state.flags.early_terminate = false;
                    state.flags.allow_early_terminate = false;
                }
            } else {
                state.flags.early_terminate = false;
                state.flags.allow_early_terminate = false;
                if state.flags.end_move_on_inactive {
                    state.flags.end_move_on_inactive = false;
                    if unit.state == UnitState::Moving {
                        unit.state = UnitState::Idle;
                    }
                }
                if state.flags.complete_on_inactive {
                    state.flags.waiting = true;
                }
            }
        }
    }

    /// The move rules of the physics vehicle squads (`BSquad::isSquadAPhysicsVehicle`: one
    /// unit, and it has a vehicle action).
    ///
    /// The squad moves itself; its unit gets no move opportunity, only the moving flag
    /// (`BSquadActionMove::setupOpp`). At the move start the body loses its deactivation and
    /// wakes, and the unit keeps its move when the body sleeps. At the end of the move the
    /// deactivation comes back and the move of the unit ends when the body sleeps (2008
    /// `squadactionmove.cpp:967-981`, `:1027-1040`).
    pub(crate) fn update_physics_vehicle_squads(&mut self) {
        let squads: Vec<(EntityId, bool)> = self
            .squads
            .iter()
            .filter_map(|(_, squad)| match squad.unit_ids.as_slice() {
                [unit_id] => Some((*unit_id, squad.move_target.is_some())),
                _ => None,
            })
            .collect();
        for (unit_id, moving) in squads {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let Some(state) = unit.physics_object.as_mut() else {
                continue;
            };
            if state.action.is_none() {
                continue;
            }
            let body = state.body;
            let world = self.physics.world_mut();
            if moving && !state.flags.squad_moving {
                world.enable_deactivation(body, false);
                world.activate(body);
                state.flags.end_move_on_inactive = false;
            } else if !moving && state.flags.squad_moving {
                world.enable_deactivation(body, true);
                state.flags.end_move_on_inactive = true;
            }
            state.flags.squad_moving = moving;
            if moving {
                unit.move_target = None;
                unit.state = UnitState::Moving;
            }
        }
    }

    /// Runs the `BUnitActionPhysics` update of each class-0 object with a dynamic body, in
    /// entity id order. Retail gives the action to any entity with a dynamic body
    /// (`BEntity_createPhysicsObject`) and runs it in `BObject::update` after the unit updates,
    /// before the physics step.
    ///
    /// - An active body moves its object and gives it its axes and velocity.
    /// - An inactive body moves a `CompleteOnInactivePhysics` action to its wait state.
    pub(crate) fn read_back_object_physics(&mut self) {
        let world = self.physics.world();
        for (_, object) in self.objects.iter_mut() {
            let Some(state) = object.physics_object.as_mut() else {
                continue;
            };
            if state.fixed || state.flags.waiting {
                continue;
            }
            let body = state.body;
            if world.is_active(body) != Some(true) {
                state.flags.waiting |= state.flags.complete_on_inactive;
                continue;
            }
            let (Some(position), Some(rotation), Some(velocity)) = (
                world.position(body),
                world.rotation(body),
                world.linear_velocity(body),
            ) else {
                continue;
            };
            let pose = unit_pose(position, rotation, velocity, state.center_offset);
            object.base.position = pose.position;
            object.base.forward = pose.forward;
            object.base.up = pose.up;
            object.base.velocity = pose.velocity;
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec3};

    use super::unit_pose;

    #[test]
    fn the_unit_sits_at_the_body_origin_minus_the_rotated_center_offset() {
        let rotation = Quat::from_mat3(&glam::Mat3::from_cols(Vec3::NEG_Z, Vec3::Y, Vec3::X));
        let pose = unit_pose(
            Vec3::new(10.0, 5.0, 3.0),
            rotation,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 2.28, 1.0),
        );
        // The offset (0, 2.28, 1) turns to (1, 2.28, 0).
        assert!((pose.position - Vec3::new(9.0, 2.72, 3.0)).length() < 1e-5);
        assert!((pose.forward - Vec3::X).length() < 1e-6);
        assert!((pose.up - Vec3::Y).length() < 1e-6);
        assert_eq!(pose.velocity, Vec3::X);
    }
}
