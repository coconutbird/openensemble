#![expect(
    dead_code,
    reason = "the plumbing commit adds the state before the body creation uses it"
)]
//! The physics state of one unit with a body.
//!
//! Retail keeps it in `BEntity::mpPhysicsObject` (DE entity `+0xA8`) and in the persistent
//! `BUnitActionPhysics` (DE action type 21) that every non-fixed body gets.

use glam::Vec3;

use ::physics::BodyId;

use super::vehicle_actions::VehicleAction;

/// The physics state of a unit with a body.
#[derive(Clone, Debug)]
pub struct UnitPhysics {
    /// The Havok body.
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
    /// The vehicle action (`hkpUnaryAction`) of a vehicle body.
    pub(crate) action: Option<VehicleAction>,
    /// The creation index of the action: the actions run in this order.
    pub(crate) action_order: u32,
    /// The state of the `BUnitActionPhysics` and of the vehicle action that the sim reads.
    pub(crate) flags: UnitPhysicsFlags,
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
    /// `EndMoveOnInactivePhysics`: the squad move ends when the body sleeps.
    pub(crate) end_move_on_inactive: bool,
    /// `AllowEarlyTerminate`: the vehicle action may ask for an early stop.
    pub(crate) allow_early_terminate: bool,
    /// The vehicle action asks the readback to stop and deactivate the body.
    pub(crate) early_terminate: bool,
    /// The first collision of the body was recorded (`BUnitActionPhysics::collision`).
    pub(crate) collided: bool,
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
            action: None,
            action_order: 0,
            flags: UnitPhysicsFlags::default(),
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
}
