//! The physics state of one unit with a body.
//!
//! Retail keeps it in `BEntity::mpPhysicsObject` (DE entity `+0xA8`) and in the persistent
//! `BUnitActionPhysics` (DE action type 21) that every non-fixed body gets.

use glam::Vec3;

use ::physics::{BodyId, ConstraintId};

use super::death::{ClamshellAnimation, DeathPhysics};
use super::gravity_ball::GravityBallPull;
use super::vehicle_actions::VehicleAction;
use crate::sync::SyncChecksum;

/// The physics state of a unit with a body.
#[derive(Clone, Debug)]
pub struct UnitPhysics {
    /// The Havok body (the pelvis of a clamshell).
    pub(crate) body: BodyId,
    /// `BPhysicsObject+0x10`: the `CenterOffset` of the physics info. The body origin is the unit
    /// position plus the rotated offset.
    pub(crate) center_offset: Vec3,
    /// The name of the physics info (`PhysicsInfo` or `PhysicsReplacementInfo`).
    pub(crate) info: String,
    /// `true` for a `MotionType fixed` body: it has the building filter and no readback.
    pub(crate) fixed: bool,
    /// The restitution of the body material, for the terrain penetration fix.
    pub(crate) restitution: f32,
    /// Half the size of the box around the main body shape of the blueprint (before a vehicle
    /// hull replaces it).
    pub(crate) half_extents: Vec3,
    /// The upper and lower bodies of a clamshell and their hinges.
    pub(crate) clamshell: Option<ClamshellBodies>,
    /// The vehicle action (`hkpUnaryAction`) of a vehicle body.
    pub(crate) action: Option<VehicleAction>,
    /// The creation index of the action: the actions run in this order.
    pub(crate) action_order: u32,
    /// The gravity ball pull action of the Wave power on the body.
    pub(crate) gravity_ball: Option<GravityBallPull>,
    /// The state of the `BUnitActionPhysics` and of the vehicle action that the sim reads.
    pub(crate) flags: UnitPhysicsFlags,
    /// What the contact listener saw since the last readback.
    pub(crate) contacts: UnitPhysicsContacts,
    /// The clamshell, rocket and thrown part state of `BUnitActionPhysics`.
    pub(crate) death: DeathPhysics,
}

/// The extra bodies of a clamshell (`BClamshellPhysicsObject`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClamshellBodies {
    /// The upper body (torso, arms and head).
    pub(crate) upper: BodyId,
    /// The lower body (legs and feet).
    pub(crate) lower: BodyId,
    /// The upper and lower limited hinges.
    pub(crate) hinges: [Option<ConstraintId>; 2],
}

/// The flags of `BUnitActionPhysics` and of the vehicle action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one retail action flag bit"
)]
pub(crate) struct UnitPhysicsFlags {
    /// `CompleteOnInactivePhysics`: the action completes when the body sleeps.
    pub(crate) complete_on_inactive: bool,
    /// `EndMoveOnInactivePhysics`: the move of the unit ends when the body sleeps.
    pub(crate) end_move_on_inactive: bool,
    /// `AllowEarlyTerminate`: the vehicle action may ask for an early stop.
    pub(crate) allow_early_terminate: bool,
    /// The vehicle action asks the readback to stop and deactivate the body.
    pub(crate) early_terminate: bool,
    /// The action is in its wait state (`cStateWait`): the readback stopped.
    pub(crate) waiting: bool,
    /// The squad of a physics vehicle was moving at the last update.
    pub(crate) squad_moving: bool,
}

/// The contacts of a body that the listener reports, cleared by each readback.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct UnitPhysicsContacts {
    /// The first contact point of the body (`BUnitActionPhysics::collision` records only the
    /// first one). It stays after the readback.
    pub(crate) first_collision: Option<CollisionRecord>,
    /// The number of new contact points with other bodies than the terrain.
    pub(crate) body_contacts: u16,
    /// The largest approach speed of a new contact point with another body.
    pub(crate) body_impact_speed: f32,
    /// The largest approach speed of a new terrain contact point.
    pub(crate) terrain_impact_speed: f32,
    /// A new terrain contact point while the body moved down (`projectedVelocity < 0`, the
    /// landing test of `BUnitActionThrown`).
    pub(crate) landed: bool,
    /// The number of new contact points of the body (any other body or the terrain).
    pub(crate) new_points: u16,
}

/// One recorded contact (`BPhysicsCollisionData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollisionRecord {
    /// The contact position.
    pub(crate) position: Vec3,
    /// The contact normal, toward this body.
    pub(crate) normal: Vec3,
    /// The approach speed along the normal (negative while the bodies approach).
    pub(crate) projected_velocity: f32,
    /// The other body, or `None` for the terrain.
    pub(crate) other: Option<BodyId>,
}

impl UnitPhysics {
    /// Makes the state of a new body without an action.
    #[must_use]
    pub(crate) fn new(body: BodyId, center_offset: Vec3, info: String, fixed: bool) -> Self {
        Self {
            body,
            center_offset,
            info,
            fixed,
            restitution: 0.0,
            half_extents: Vec3::ZERO,
            clamshell: None,
            action: None,
            action_order: 0,
            gravity_ball: None,
            flags: UnitPhysicsFlags::default(),
            contacts: UnitPhysicsContacts::default(),
            death: DeathPhysics::default(),
        }
    }

    /// The Havok body.
    #[must_use]
    pub const fn body(&self) -> BodyId {
        self.body
    }

    /// The `CenterOffset` of the physics info.
    #[must_use]
    pub const fn center_offset(&self) -> Vec3 {
        self.center_offset
    }

    /// Half the size of the box around the blueprint shape of the main body.
    #[must_use]
    pub const fn half_extents(&self) -> Vec3 {
        self.half_extents
    }

    /// The name of the physics info.
    #[must_use]
    pub fn info(&self) -> &str {
        &self.info
    }

    /// `true` for a fixed body.
    #[must_use]
    pub const fn is_fixed(&self) -> bool {
        self.fixed
    }

    /// `true` if the body has a vehicle action.
    #[must_use]
    pub const fn has_vehicle_action(&self) -> bool {
        self.action.is_some()
    }

    /// `mFlagInAir` of the vehicle action: `true` if no wheel touched the terrain in the last
    /// substep. `None` without a vehicle action; aircraft always give `false`.
    #[must_use]
    pub fn vehicle_in_air(&self) -> Option<bool> {
        self.action.as_ref().map(VehicleAction::in_air)
    }

    /// The upper and lower bodies of a clamshell.
    #[must_use]
    pub fn clamshell_bodies(&self) -> Option<[BodyId; 2]> {
        self.clamshell
            .map(|clamshell| [clamshell.upper, clamshell.lower])
    }

    /// The animation of a clamshell body (`None` for other bodies).
    #[must_use]
    pub const fn clamshell_animation(&self) -> Option<ClamshellAnimation> {
        self.death.clamshell_animation
    }

    /// The animation position (0 to 1) that the last clamshell readback took from the hinge
    /// value.
    #[must_use]
    pub const fn clamshell_animation_position(&self) -> f32 {
        self.death.clamshell_animation_position
    }

    /// Adds the state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.body.raw());
        checksum.hash_vec3(
            self.center_offset.x,
            self.center_offset.y,
            self.center_offset.z,
        );
        checksum.hash_u32(u32::from(self.fixed));
        if let Some(clamshell) = &self.clamshell {
            checksum.hash_u32(clamshell.upper.raw());
            checksum.hash_u32(clamshell.lower.raw());
        }
        if let Some(action) = &self.action {
            checksum.hash_u32(self.action_order);
            action.hash_state(checksum);
        }
        if let Some(pull) = &self.gravity_ball {
            pull.hash_state(checksum);
        }
        let flags = self.flags;
        for flag in [
            flags.complete_on_inactive,
            flags.end_move_on_inactive,
            flags.allow_early_terminate,
            flags.early_terminate,
            flags.waiting,
            flags.squad_moving,
        ] {
            checksum.hash_u32(u32::from(flag));
        }
        self.death.hash_state(checksum);
    }
}
