//! The sim state that the vehicle actions read during a physics substep.
//!
//! The retail actions read their unit, its squad and the squad move action
//! (`BSquadActionMove`) directly. The sim builds this snapshot from its `World` before each
//! substep, so that the action code needs no access to the sim entities.

use glam::Vec3;

/// The state of the unit that owns the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnitInput {
    /// The unit position (`getPosition`), as the last readback wrote it.
    pub(crate) position: Vec3,
    /// The unit forward axis (`getForward`).
    pub(crate) forward: Vec3,
    /// The unit right axis (`getRight`).
    pub(crate) right: Vec3,
    /// The obstruction radius (`getObstructionRadius`).
    pub(crate) obstruction_radius: f32,
    /// The desired velocity of the proto object (`getProtoObject()->getDesiredVelocity()`).
    pub(crate) proto_desired_velocity: f32,
    /// The maximum velocity of the unit (`BUnit::getMaxVelocity`).
    pub(crate) max_velocity: f32,
    /// The reverse speed (`BUnit::getReverseSpeed`): −1 if the proto object sets none.
    pub(crate) reverse_speed: f32,
    /// The turn rate of the proto object in degrees per second (`getTurnRate`).
    pub(crate) turn_rate_degrees: f32,
    /// `true` if the unit has its `BUnitActionPhysics`.
    pub(crate) has_physics_action: bool,
    /// `getFlagDoingFatality`.
    pub(crate) doing_fatality: bool,
    /// The target of the ranged attack action, if the unit has one with a target.
    pub(crate) ranged_target: Option<RangedTarget>,
}

/// The target of a `BUnitActionRangedAttack`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RangedTarget {
    /// The position of the target entity, or `None` if the entity does not exist.
    pub(crate) entity_position: Option<Vec3>,
    /// The position of the sim target (`BSimTarget::getPosition`).
    pub(crate) position: Vec3,
}

impl RangedTarget {
    /// The entity position, or the sim target position if the entity does not exist (the hover
    /// rule).
    pub(crate) fn entity_or_target_position(&self) -> Vec3 {
        self.entity_position.unwrap_or(self.position)
    }
}

/// The squad move action (`BSquadActionMove`) of the parent squad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MoveActionInput {
    /// The position of the move target (`getTarget()->getPosition()`).
    pub(crate) target: Vec3,
    /// `getState() == cStateWorking`.
    pub(crate) working: bool,
    /// `getState() == cStateWait`.
    pub(crate) waiting: bool,
    /// `getVelocity()`.
    pub(crate) velocity: f32,
    /// `getFuturePosition(0.2 · (1 + i), true)` for the five look-ahead slots of the hover
    /// action.
    pub(crate) future_positions: [Vec3; 5],
}

/// The parent squad of the unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SquadInput {
    /// The squad position (`getPosition`).
    pub(crate) position: Vec3,
    /// The turn-radius position (`getTurnRadiusPos`).
    pub(crate) turn_radius_position: Vec3,
    /// The turn-radius forward axis (`getTurnRadiusFwd`).
    pub(crate) turn_radius_forward: Vec3,
    /// `getDesiredVelocity`: the desired velocity of the leader unit.
    pub(crate) desired_velocity: f32,
    /// `getStasisSpeedMult`.
    pub(crate) stasis_speed_multiplier: f32,
    /// The squad move action, if the squad has one.
    pub(crate) move_action: Option<MoveActionInput>,
}

/// The terrain height and the flight height at one point (the hover action samples).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HeightSample {
    /// `gTerrainSimRep.getHeight`.
    pub(crate) terrain: f32,
    /// The flight height: `getFlightHeightRaycast` from 100 above, or the fallback that retail
    /// uses without flight heights. A transport with a hover offset uses the terrain height.
    pub(crate) flight: f32,
}

/// A crash of an aircraft (`BUnitActionAvoidCollisionAir::Crashing`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CrashInput {
    /// `getCrashPos`.
    pub(crate) position: Vec3,
    /// The position of the live kamikaze target, if the aircraft has one.
    pub(crate) kamikaze_target: Option<Vec3>,
}

/// The air state that the hover action reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AirInput {
    /// The `hoverAltitudeOffset` of the avoid-air proto action, or 0.
    pub(crate) hover_altitude_offset: f32,
    /// The `maxPitch` and `maxRoll` of the avoid-air proto action, if the unit has one.
    pub(crate) max_pitch_roll: Option<(f32, f32)>,
    /// The crash, if the avoid-air action crashes the aircraft.
    pub(crate) crash: Option<CrashInput>,
    /// The nearest obstacle altitude while the avoid-air action avoids (`Avoiding`).
    pub(crate) avoiding_obstacle_altitude: Option<f32>,
    /// `speedLimited`: the birth speed limit.
    pub(crate) speed_limited: bool,
    /// The hover offset of the transport action, if the unit is a transporter that has one.
    pub(crate) transport_hover_offset: Option<f32>,
    /// The samples at the body points `(0, 0, 4)`, `(−4, 0, −4)` and `(4, 0, −4)` of the unit
    /// matrix.
    pub(crate) body_samples: [HeightSample; 3],
    /// The samples at the five future positions of the move action.
    pub(crate) future_samples: [HeightSample; 5],
}

/// Everything that one vehicle action reads in one substep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VehicleActionInput {
    /// The unit.
    pub(crate) unit: UnitInput,
    /// The parent squad, if the unit has one.
    pub(crate) squad: Option<SquadInput>,
    /// The air state, for the hover action.
    pub(crate) air: Option<AirInput>,
}

/// The flags of `BUnitActionPhysics` that the actions write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EarlyTerminateFlags {
    /// `AllowEarlyTerminate`.
    pub(crate) allow: bool,
    /// `EarlyTerminate`: the readback zeroes the velocities and deactivates the body.
    pub(crate) terminate: bool,
}
