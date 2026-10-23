//! The death and debris parts of `BUnitActionPhysics_update` (`0x140547880`, 2008
//! `unitactionphysics.cpp:187-625`): the clamshell readback and its collision listener, the
//! rocket on death, and the collision filter of a thrown part that changes once the part is
//! outside its parent.

use ::physics::{BodyId, FilterInfo};
use glam::{Quat, Vec3};

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use crate::world::World;

/// `cRocketForce`: the impulse along the unit up axis per second of the rocket.
const ROCKET_FORCE: f32 = 3000.0;
/// `cInitialRocketUpForceMin`: the smallest first upward impulse.
const INITIAL_ROCKET_UP_FORCE_MIN: f32 = 1200.0;
/// `cInitialRocketUpForceMax`: the largest first upward impulse. DE draws
/// `1200 + 2300·rand` (`0x14054816B`).
const INITIAL_ROCKET_UP_FORCE_MAX: f32 = 3500.0;
/// `cRocketUpForce`: the upward impulse per second after the first update.
const ROCKET_UP_FORCE: f32 = 250.0;
/// `cSpinForce`: the impulse against the unit forward axis per second at the head bone.
const ROCKET_SPIN_FORCE: f32 = 800.0;
/// `cDefaultRocketAngularDamping`: the angular damping of the first rocket update.
const ROCKET_ANGULAR_DAMPING: f32 = 10.0;
/// `cMinRocketTime` in milliseconds. DE draws `750 + (rand >> 19 & 0xFFF)`
/// (`BUnit_createAndThrowPhysicsReplacement`, `0x1404DEA30`).
pub(crate) const MIN_ROCKET_TIME_MS: u32 = 750;
/// `cAnimZeroPos`: the hinge value that the clamshell animation adds before the clamp.
const CLAMSHELL_ANIMATION_ZERO: f32 = 0.2;

/// The animation of a clamshell replacement (`setAnimationState(cAnimationStateMisc, …)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClamshellAnimation {
    /// `cAnimTypeClamshell` (39): the hinge angle drives the animation clock and the readback
    /// takes the unit pose from `getClamshellData`.
    #[default]
    Clamshell,
    /// `cAnimTypeFlail` (53): the unit flails until its first contact.
    Flail,
    /// `cAnimTypeClamshellRocket` (40): the rocket on death animation.
    ClamshellRocket,
}

/// `BClamshellCollisionListener` (2008 `unitactionphysics.h:17-33`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClamshellListener {
    /// `mCollided`: a contact point was added to one of the three bodies.
    pub(crate) collided: bool,
    /// `mCollisionProcessed`: the animation changed to the clamshell animation.
    pub(crate) processed: bool,
}

/// `setupRocketOnDeath`: the rocket timer of a thrown replacement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RocketOnDeath {
    /// `mStartRocketTime`: the game time of the throw in milliseconds.
    pub(crate) start_ms: u32,
    /// `mRocketTime`: the rocket duration in milliseconds.
    pub(crate) duration_ms: u32,
    /// `mFlagRocketExplode`: explode and die when the timer ends.
    pub(crate) explode: bool,
    /// The model space position of `Bip01 Head` (or `BoneBubble02`). `None` skips the spin
    /// impulse, as retail does when `getBone` fails.
    pub(crate) head_bone: Option<Vec3>,
}

/// `enableDynamicCollisionFilter`: the filter that a thrown part gets once its body is outside
/// the body of its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DynamicFilter {
    /// `mCollisionFilterInfo`: the new Objects filter.
    pub(crate) filter: FilterInfo,
    /// `mParentEntityID`: the unit that threw the part.
    pub(crate) parent: EntityId,
}

/// The death and debris state of `BUnitActionPhysics`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DeathPhysics {
    /// `!mFlagFirstUpdate`: the action ran its first update.
    pub(crate) updated: bool,
    /// The animation of a clamshell body.
    pub(crate) clamshell_animation: Option<ClamshellAnimation>,
    /// The listener that a flailing clamshell gets in its first update.
    pub(crate) clamshell_listener: Option<ClamshellListener>,
    /// The animation position of the last clamshell readback (0 to 1).
    pub(crate) clamshell_animation_position: f32,
    /// The rocket on death.
    pub(crate) rocket: Option<RocketOnDeath>,
    /// The rocket timer ended and the unit explodes.
    pub(crate) rocket_finished: bool,
    /// The collision filter that waits until the part is outside its parent.
    pub(crate) dynamic_filter: Option<DynamicFilter>,
}

impl DeathPhysics {
    /// Adds the state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.updated));
        if let Some(animation) = self.clamshell_animation {
            checksum.hash_u32(animation as u32);
        }
        if let Some(listener) = self.clamshell_listener {
            checksum.hash_u32(u32::from(listener.collided));
            checksum.hash_u32(u32::from(listener.processed));
        }
        checksum.hash_f32(self.clamshell_animation_position);
        if let Some(rocket) = self.rocket {
            checksum.hash_u32(rocket.start_ms);
            checksum.hash_u32(rocket.duration_ms);
            checksum.hash_u32(u32::from(rocket.explode));
        }
        checksum.hash_u32(u32::from(self.rocket_finished));
        if let Some(dynamic) = self.dynamic_filter {
            checksum.hash_u32(dynamic.parent.as_u32());
        }
    }
}

/// The unit pose of a clamshell (`BClamshellPhysicsObject::getClamshellData`, DE
/// `0x1407B8520`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClamshellPose {
    /// The pelvis origin minus the `CenterOffset` in the right, up and forward axes.
    pub(crate) position: Vec3,
    /// The normalized sum of the upper and lower forward axes.
    pub(crate) forward: Vec3,
    /// `forward × right`, normalized.
    pub(crate) up: Vec3,
    /// The pelvis right axis.
    pub(crate) right: Vec3,
    /// The hinge value: `(cos + 1) / 2` of the angle between the upper and lower directions
    /// from the pelvis, negative when `upper × lower` points against `right`.
    pub(crate) angle: f32,
}

/// The pose of one clamshell body: origin and rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BodyFrame {
    /// The body origin.
    pub(crate) position: Vec3,
    /// The body rotation.
    pub(crate) rotation: Quat,
}

/// `getClamshellData`. Each normalize follows `XMVector3Normalize`: a zero vector stays zero.
#[must_use]
pub(crate) fn clamshell_pose(
    pelvis: BodyFrame,
    upper: BodyFrame,
    lower: BodyFrame,
    center_offset: Vec3,
) -> ClamshellPose {
    let forward = normalized((upper.rotation * Vec3::Z) + (lower.rotation * Vec3::Z));
    let right = pelvis.rotation * Vec3::X;
    let up = normalized(forward.cross(right));
    let offset = right * center_offset.x + up * center_offset.y + forward * center_offset.z;
    let to_upper = normalized(upper.position - pelvis.position);
    let to_lower = normalized(lower.position - pelvis.position);
    let cosine = to_upper.dot(to_lower);
    let half = f32::midpoint(cosine, 1.0);
    let angle = if to_upper.cross(to_lower).dot(right) > 0.0 {
        half
    } else {
        -half
    };
    ClamshellPose {
        position: pelvis.position - offset,
        forward,
        up,
        right,
        angle,
    }
}

/// The animation position of a clamshell hinge value: `clamp(angle + 0.2, 0, 1)`.
#[must_use]
pub(crate) fn clamshell_animation_position(angle: f32) -> f32 {
    (angle + CLAMSHELL_ANIMATION_ZERO).clamp(0.0, 1.0)
}

fn normalized(value: Vec3) -> Vec3 {
    let length = value.length();
    if length == 0.0 {
        Vec3::ZERO
    } else {
        value / length
    }
}

/// The bodies of a unit for the clamshell overrides of `BClamshellPhysicsObject` (vtable
/// `0x1411E4120`): the main body, then the upper and lower bodies of a clamshell.
pub(crate) fn body_and_parts(
    main: BodyId,
    parts: Option<[BodyId; 2]>,
) -> impl Iterator<Item = BodyId> {
    std::iter::once(main).chain(parts.into_iter().flatten())
}

impl World {
    /// The death and debris part of the `BUnitActionPhysics` update of each unit with a dynamic
    /// body, in entity id order, after the readback of the same update.
    ///
    /// - A clamshell that flails gets its collision listener in the first update. A contact
    ///   point then switches the animation to the clamshell animation.
    /// - While the body is active, a clamshell with the clamshell animation takes the unit pose
    ///   from `getClamshellData`.
    /// - While the body is active, the rocket on death pushes the body up until its timer ends.
    ///   Then the unit explodes and dies (see [`World::finished_rocket_deaths`]).
    /// - While the body is active, a part with a dynamic filter gets its Objects filter once it
    ///   is outside the body of its parent, or when the parent or its body is gone.
    pub(crate) fn update_death_physics(&mut self, elapsed: f32) {
        let unit_ids: Vec<EntityId> = self
            .units
            .iter()
            .filter(|(_, unit)| {
                unit.physics_object
                    .as_ref()
                    .is_some_and(|state| !state.fixed && !state.flags.waiting)
            })
            .map(|(unit_id, _)| unit_id)
            .collect();
        for unit_id in unit_ids {
            self.update_unit_death_physics(unit_id, elapsed);
        }
    }

    fn update_unit_death_physics(&mut self, unit_id: EntityId, elapsed: f32) {
        let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        else {
            return;
        };
        let first_update = !state.death.updated;
        state.death.updated = true;
        let new_points = state.contacts.new_points;
        if first_update && state.death.clamshell_animation == Some(ClamshellAnimation::Flail) {
            state.death.clamshell_listener = Some(ClamshellListener::default());
        }
        if let Some(listener) = state.death.clamshell_listener.as_mut() {
            // The listener sees the contact points of the steps after the first update.
            listener.collided |= !first_update && new_points > 0;
            if listener.collided && !listener.processed {
                state.death.clamshell_animation = Some(ClamshellAnimation::Clamshell);
                listener.processed = true;
            }
        }
        let body = state.body;
        if self.physics.world().is_active(body) != Some(true) {
            return;
        }
        self.read_back_clamshell(unit_id);
        self.update_rocket_on_death(unit_id, elapsed, first_update);
        self.update_dynamic_filter(unit_id);
    }

    /// The clamshell animation processing of the readback: the unit pose from the three
    /// bodies and the animation position from the hinge value.
    fn read_back_clamshell(&mut self, unit_id: EntityId) {
        let world = self.physics.world();
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let Some(state) = unit.physics_object.as_mut() else {
            return;
        };
        let (Some(clamshell), Some(ClamshellAnimation::Clamshell)) =
            (state.clamshell, state.death.clamshell_animation)
        else {
            return;
        };
        let frame = |body| {
            Some(BodyFrame {
                position: world.position(body)?,
                rotation: world.rotation(body)?,
            })
        };
        let (Some(pelvis), Some(upper), Some(lower)) = (
            frame(state.body),
            frame(clamshell.upper),
            frame(clamshell.lower),
        ) else {
            return;
        };
        let pose = clamshell_pose(pelvis, upper, lower, state.center_offset);
        state.death.clamshell_animation_position = clamshell_animation_position(pose.angle);
        unit.base.position = pose.position;
        unit.base.forward = pose.forward;
        unit.base.up = pose.up;
    }

    /// The rocket on death branch of the readback (DE `0x1405480A8`-`0x1405482B0`). The
    /// damping and the linear impulses go through the clamshell overrides
    /// (`setAngularDamping`, `applyImpulse`); the spin impulse is a point impulse on the main
    /// body only.
    fn update_rocket_on_death(&mut self, unit_id: EntityId, elapsed: f32, first_update: bool) {
        let Some(unit) = self.units.get(unit_id) else {
            return;
        };
        let Some(state) = unit.physics_object.as_ref() else {
            return;
        };
        let Some(rocket) = state.death.rocket else {
            return;
        };
        let (up, forward) = (unit.base.up, unit.base.forward);
        let main = state.body;
        let rocket_on = self.game_time_ms.wrapping_sub(rocket.start_ms) < rocket.duration_ms;
        if first_update {
            let _damped = self.set_unit_body_angular_damping(unit_id, ROCKET_ANGULAR_DAMPING);
        }
        if rocket_on {
            let lift = if first_update {
                Vec3::Y
                    * self
                        .sim_random_mut()
                        .range_float(INITIAL_ROCKET_UP_FORCE_MIN, INITIAL_ROCKET_UP_FORCE_MAX)
            } else {
                Vec3::Y * (ROCKET_UP_FORCE * elapsed)
            };
            let _pushed = self.apply_unit_impulse(unit_id, up * (ROCKET_FORCE * elapsed));
            let _lifted = self.apply_unit_impulse(unit_id, lift);
            if let Some(head) = rocket.head_bone {
                let spin = -forward * (ROCKET_SPIN_FORCE * elapsed);
                let world = self.physics.world_mut();
                let origin = world.position(main).unwrap_or(Vec3::ZERO);
                world.apply_point_impulse(main, spin, head + origin);
            }
        } else if rocket.explode
            && let Some(state) = self
                .units
                .get_mut(unit_id)
                .and_then(|unit| unit.physics_object.as_mut())
        {
            state.death.rocket_finished = true;
        }
    }

    /// The dynamic collision filter of a thrown part (2008 `unitactionphysics.cpp:282-337`).
    fn update_dynamic_filter(&mut self, unit_id: EntityId) {
        let Some(state) = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.physics_object.as_ref())
        else {
            return;
        };
        let Some(dynamic) = state.death.dynamic_filter else {
            return;
        };
        let body = state.body;
        let bodies = body_and_parts(body, state.clamshell_bodies());
        let outside = self.unit_body(dynamic.parent).is_none_or(|parent| {
            !self
                .physics
                .world()
                .bodies_penetrate(body, parent)
                .unwrap_or(false)
        });
        if !outside {
            return;
        }
        let world = self.physics.world_mut();
        for body in bodies {
            world.set_filter_info(body, dynamic.filter);
        }
        if let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        {
            state.death.dynamic_filter = None;
        }
    }

    /// The units whose rocket timer ended in the last readback.
    pub(crate) fn finished_rocket_deaths(&self) -> Vec<EntityId> {
        self.units
            .iter()
            .filter(|(_, unit)| {
                unit.physics_object
                    .as_ref()
                    .is_some_and(|state| state.death.rocket_finished)
            })
            .map(|(unit_id, _)| unit_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{BodyFrame, clamshell_animation_position, clamshell_pose};
    use glam::{Quat, Vec3};

    fn frame(position: Vec3) -> BodyFrame {
        BodyFrame {
            position,
            rotation: Quat::IDENTITY,
        }
    }

    #[test]
    fn a_straight_clamshell_gives_the_pelvis_pose_and_hinge_value_zero() {
        let pose = clamshell_pose(
            frame(Vec3::new(0.0, 2.0, 0.0)),
            frame(Vec3::new(0.0, 3.0, 0.0)),
            frame(Vec3::new(0.0, 1.0, 0.0)),
            Vec3::new(0.0, 1.0, 0.0),
        );
        assert!(pose.forward.abs_diff_eq(Vec3::Z, 1.0e-6));
        assert!(pose.right.abs_diff_eq(Vec3::X, 1.0e-6));
        assert!(pose.up.abs_diff_eq(Vec3::Y, 1.0e-6));
        assert!(pose.position.abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1.0e-6));
        // Opposite directions: cos = -1, so the value is 0 on either side.
        assert!(pose.angle.abs() < 1.0e-6);
        assert!((clamshell_animation_position(pose.angle) - 0.2).abs() < 1.0e-6);
    }

    #[test]
    fn a_folded_clamshell_gives_a_signed_hinge_value() {
        // Upper bent forward (+Z): upper × lower = Z × -Y points along +X, with `right`.
        let folded = clamshell_pose(
            frame(Vec3::ZERO),
            frame(Vec3::Z),
            frame(-Vec3::Y),
            Vec3::ZERO,
        );
        assert!((folded.angle - 0.5).abs() < 1.0e-6);
        // Upper bent backward: the cross product points along -X.
        let back = clamshell_pose(
            frame(Vec3::ZERO),
            frame(-Vec3::Z),
            frame(-Vec3::Y),
            Vec3::ZERO,
        );
        assert!((back.angle + 0.5).abs() < 1.0e-6);
        assert!((clamshell_animation_position(folded.angle) - 0.7).abs() < 1.0e-6);
        assert!(clamshell_animation_position(back.angle).abs() < 1.0e-6);
    }
}
