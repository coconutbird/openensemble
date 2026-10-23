//! The gravity ball pull action of the Wave power and the order of the Havok actions.
//!
//! `BPowerWave_addObjectToBall` (`0x140331D40`) and `BPowerWave_grabFakeObject` (`0x1403324B0`)
//! give a body a `BPhysicsGravityBallPullAction` (`hkpWorld_addAction`, `0x1408796B0`). Havok
//! runs `applyAction` (`0x1402CE1A0`, 2008 `physicsgravityballpullaction.cpp:33-111`) before
//! each substep, together with the vehicle actions, in action creation order.

use glam::Vec3;

use ::physics::{BodyId, World as PhysicsWorld};

use super::vehicle_actions::apply_vehicle_action;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use crate::world::World;

/// `dword_1411F3B90`: below this distance to the ball the spring has no direction.
const MINIMUM_SPRING_LENGTH: f32 = 0.001;

/// The spring values that the Wave gives the action (`setRestLength`, `setStrength`,
/// `setDamping`, `setMinLateralSpeed`; action `+0x60`..`+0x6C`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GravityBallSpring {
    /// The length at which the spring starts to pull.
    pub(crate) rest_length: f32,
    /// The force per unit of extension.
    pub(crate) strength: f32,
    /// The force per unit of radial speed.
    pub(crate) damping: f32,
    /// Below this lateral speed the action adds the lateral velocity again.
    pub(crate) minimum_lateral_speed: f32,
}

/// The motion of the ball object that the action reads (`getPosition`, `getVelocity`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BallMotion {
    /// The ball position.
    pub(crate) position: Vec3,
    /// The ball velocity.
    pub(crate) velocity: Vec3,
}

/// A `BPhysicsGravityBallPullAction` on the body of a unit.
///
/// The Wave sets `mOnCompression = false` and `mOnExtension = true` (action `+0x70` = `0x0100`),
/// so the spring only pulls when the body is farther than the rest length.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GravityBallPull {
    /// The creation index of the action: the actions run in this order.
    order: u32,
    /// The ball object.
    ball: EntityId,
    /// `mExtraEntities`: the upper and lower bodies of a clamshell.
    extra_bodies: Vec<BodyId>,
    /// The spring values.
    spring: GravityBallSpring,
}

impl GravityBallPull {
    /// The ball object of the action.
    #[cfg(test)]
    pub(crate) const fn ball(&self) -> EntityId {
        self.ball
    }

    /// The spring values of the action.
    #[cfg(test)]
    pub(crate) const fn spring(&self) -> GravityBallSpring {
        self.spring
    }

    /// `applyAction` for one substep of `dt` seconds on `body`.
    ///
    /// 1. Anti-gravity `applyForce(dt, −m·g)` on the body and on each extra body (with the mass
    ///    of the main body).
    /// 2. The spring along ball → centre of mass: `F = (len − rest)·k + relVel·damping`,
    ///    `applyForce(dt, −F·dir, com)`, also on each extra body at its own centre of mass.
    /// 3. If the lateral speed of the body is below the minimum, a point impulse of the lateral
    ///    velocity times the mass at the centre of mass (and on each extra body).
    ///
    /// DE replaces a NaN or infinite ball position, ball velocity or lateral velocity with zero
    /// (`sub_14086BAF0`).
    pub(crate) fn apply(
        &self,
        physics: &mut PhysicsWorld,
        body: BodyId,
        ball: BallMotion,
        dt: f32,
    ) {
        let Some(mass) = physics.mass(body) else {
            return;
        };
        let anti_gravity = physics.gravity() * -mass;
        physics.apply_force(body, dt, anti_gravity);
        for &extra in &self.extra_bodies {
            physics.apply_force(extra, dt, anti_gravity);
        }
        let Some(center) = physics.center_of_mass_world(body) else {
            return;
        };
        let offset = center - finite_or_zero(ball.position);
        let length = offset.length();
        if MINIMUM_SPRING_LENGTH > length {
            return;
        }
        let direction = offset * (1.0 / length);
        if self.spring.rest_length > length {
            return;
        }
        let Some(body_velocity) = physics.point_velocity(body, center) else {
            return;
        };
        let radial_speed = (body_velocity - finite_or_zero(ball.velocity)).dot(direction);
        let force = (length - self.spring.rest_length) * self.spring.strength
            + radial_speed * self.spring.damping;
        let spring_force = direction * -force;
        physics.apply_force_at_point(body, dt, spring_force, center);
        for &extra in &self.extra_bodies {
            if let Some(extra_center) = physics.center_of_mass_world(extra) {
                physics.apply_force_at_point(extra, dt, spring_force, extra_center);
            }
        }
        let lateral = body_velocity - direction * body_velocity.dot(direction);
        if self.spring.minimum_lateral_speed > lateral.length() {
            let impulse = finite_or_zero(lateral) * mass;
            physics.apply_point_impulse(body, impulse, center);
            for &extra in &self.extra_bodies {
                if let Some(extra_center) = physics.center_of_mass_world(extra) {
                    physics.apply_point_impulse(extra, impulse, extra_center);
                }
            }
        }
    }

    /// Adds the action to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.order);
        checksum.hash_u32(self.ball.as_u32());
        checksum.hash_u32(u32::try_from(self.extra_bodies.len()).unwrap_or(u32::MAX));
        for body in &self.extra_bodies {
            checksum.hash_u32(body.raw());
        }
        for value in [
            self.spring.rest_length,
            self.spring.strength,
            self.spring.damping,
            self.spring.minimum_lateral_speed,
        ] {
            checksum.hash_f32(value);
        }
    }
}

/// One Havok action that runs in a substep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PhysicsAction {
    /// The vehicle action of the unit.
    Vehicle(EntityId),
    /// The gravity ball pull action of the unit.
    GravityBall(EntityId),
}

impl World {
    /// The actions that run this substep, in action creation order: the vehicle actions and the
    /// gravity ball actions of the active bodies. Havok runs only the actions of the active
    /// islands.
    pub(crate) fn active_physics_actions(&self) -> Vec<PhysicsAction> {
        let world = self.physics.world();
        let mut actions: Vec<(u32, PhysicsAction)> = Vec::new();
        for (id, unit) in self.units.iter() {
            let Some(state) = unit.physics_object.as_ref() else {
                continue;
            };
            if world.is_active(state.body) != Some(true) {
                continue;
            }
            if state.action.is_some() {
                actions.push((state.action_order, PhysicsAction::Vehicle(id)));
            }
            if let Some(pull) = &state.gravity_ball {
                actions.push((pull.order, PhysicsAction::GravityBall(id)));
            }
        }
        actions.sort_unstable();
        actions.into_iter().map(|(_, action)| action).collect()
    }

    /// `forceActivate`, `new BPhysicsGravityBallPullAction` and `hkpWorld::addAction` on the
    /// body of a unit. The extra bodies are the upper and lower bodies of a clamshell. A unit
    /// that has an action already gets a new one. Returns `false` if the unit has no dynamic
    /// body.
    pub(crate) fn add_gravity_ball_pull(
        &mut self,
        unit_id: EntityId,
        ball: EntityId,
        spring: GravityBallSpring,
    ) -> bool {
        let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
            .filter(|state| !state.fixed)
        else {
            return false;
        };
        self.physics.world_mut().activate(state.body);
        let extra_bodies = state
            .clamshell_bodies()
            .map_or_else(Vec::new, |bodies| bodies.to_vec());
        state.gravity_ball = Some(GravityBallPull {
            order: self.physics.next_action_order(),
            ball,
            extra_bodies,
            spring,
        });
        true
    }

    /// `hkpWorld::removeAction` of the gravity ball action of a unit. Returns `false` if the
    /// unit had none.
    pub(crate) fn remove_gravity_ball_pull(&mut self, unit_id: EntityId) -> bool {
        self.units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
            .and_then(|state| state.gravity_ball.take())
            .is_some()
    }

    /// The gravity ball action of a unit.
    #[cfg(test)]
    pub(crate) fn unit_gravity_ball_pull(&self, unit_id: EntityId) -> Option<&GravityBallPull> {
        self.units
            .get(unit_id)?
            .physics_object
            .as_ref()?
            .gravity_ball
            .as_ref()
    }

    /// `BUnitActionPhysics::setState(cStateNone)` and `setFlagCompleteOnInactivePhysics`: the
    /// readback runs again, and the action completes when the body sleeps only if
    /// `complete_on_inactive` is set. Returns `false` if the unit has no body.
    pub(crate) fn restart_unit_physics_action(
        &mut self,
        unit_id: EntityId,
        complete_on_inactive: bool,
    ) -> bool {
        let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        else {
            return false;
        };
        state.flags.waiting = false;
        state.flags.complete_on_inactive = complete_on_inactive;
        true
    }

    /// `setFlagCompleteOnInactivePhysics` on the physics action of a unit. Returns `false` if
    /// the unit has no body.
    pub(crate) fn set_unit_physics_complete_on_inactive(
        &mut self,
        unit_id: EntityId,
        complete_on_inactive: bool,
    ) -> bool {
        let Some(state) = self
            .units
            .get_mut(unit_id)
            .and_then(|unit| unit.physics_object.as_mut())
        else {
            return false;
        };
        state.flags.complete_on_inactive = complete_on_inactive;
        true
    }
}

/// Applies the Havok actions of the active bodies for one substep of `dt` seconds, in action
/// creation order.
pub(crate) fn apply_physics_actions(world: &mut World, dt: f32) {
    for action in world.active_physics_actions() {
        match action {
            PhysicsAction::Vehicle(unit_id) => apply_vehicle_action(world, unit_id, dt),
            PhysicsAction::GravityBall(unit_id) => apply_gravity_ball_pull(world, unit_id, dt),
        }
    }
}

/// Applies the gravity ball action of one unit. The action does nothing while its ball object
/// is missing.
fn apply_gravity_ball_pull(world: &mut World, unit_id: EntityId, dt: f32) {
    let units = &world.units;
    let Some(state) = units
        .get(unit_id)
        .and_then(|unit| unit.physics_object.as_ref())
    else {
        return;
    };
    let Some(pull) = state.gravity_ball.as_ref() else {
        return;
    };
    let Some(ball) = units.get(pull.ball).map(|ball| BallMotion {
        position: ball.base.position,
        velocity: ball.base.velocity,
    }) else {
        return;
    };
    pull.apply(world.physics.world_mut(), state.body, ball, dt);
}

/// Zero for a vector with a NaN or infinite component (the DE `isOk3` check).
fn finite_or_zero(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}

#[cfg(test)]
mod tests;
