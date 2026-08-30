//! Authoritative movement-controller identity for flying units.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Retail movement action selected for a unit prototype.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlightControllerKind {
    /// Ordinary unit movement; no specialized flight controller was selected.
    #[default]
    Direct,
    /// Havok-backed aircraft using the Ghost movement and hover-flight actions.
    PhysicsHover,
    /// Non-physics aircraft using retail's `MoveAir` action.
    MoveAir,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct UnitFlight {
    controller: FlightControllerKind,
    move_air: MoveAirState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MoveAirState {
    pub(crate) action: MoveAirActionState,
    pub(crate) lifecycle: MoveAirLifecycleFlags,
    pub(crate) air_base: Option<EntityId>,
    pub(crate) base_position: Vec3,
    pub(crate) pad_position: Vec3,
    pub(crate) spot_forward: Vec3,
    pub(crate) height_displacement: f32,
    pub(crate) turn_rate: f32,
    pub(crate) goal_altitude_increment: f32,
    pub(crate) current_altitude_increment: f32,
    pub(crate) previous_altitude_change: f32,
    pub(crate) altitude_select_timer: f32,
    pub(crate) speed_select_timer: f32,
    pub(crate) goal_speed: f32,
    pub(crate) tactic: MoveAirTacticState,
    pub(crate) goal_position: Vec3,
    pub(crate) goal_position_valid: bool,
    pub(crate) hover_timer: f32,
    pub(crate) attack_blocked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MoveAirLifecycleFlags(u8);

impl MoveAirLifecycleFlags {
    const INITIALIZED: u8 = 1 << 0;
    const PAD_POSITION_VALID: u8 = 1 << 1;
    const LAUNCH_REQUESTED: u8 = 1 << 2;
    const RETURNING_TO_BASE: u8 = 1 << 3;

    pub(crate) const fn initialized(self) -> bool {
        self.0 & Self::INITIALIZED != 0
    }

    pub(crate) fn set_initialized(&mut self, value: bool) {
        self.set(Self::INITIALIZED, value);
    }

    pub(crate) const fn pad_position_valid(self) -> bool {
        self.0 & Self::PAD_POSITION_VALID != 0
    }

    pub(crate) fn set_pad_position_valid(&mut self, value: bool) {
        self.set(Self::PAD_POSITION_VALID, value);
    }

    pub(crate) const fn launch_requested(self) -> bool {
        self.0 & Self::LAUNCH_REQUESTED != 0
    }

    pub(crate) fn set_launch_requested(&mut self, value: bool) {
        self.set(Self::LAUNCH_REQUESTED, value);
    }

    pub(crate) fn set_returning_to_base(&mut self, value: bool) {
        self.set(Self::RETURNING_TO_BASE, value);
    }

    pub(crate) const fn returning_to_base(self) -> bool {
        self.0 & Self::RETURNING_TO_BASE != 0
    }

    fn set(&mut self, mask: u8, value: bool) {
        if value {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MoveAirActionState {
    #[default]
    None,
    Pathing,
    Working,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MoveAirTacticState {
    #[default]
    Navigate,
    ReturnToSquad,
    Strafe,
    LaunchHover,
}

impl UnitFlight {
    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.controller as u32);
        checksum.hash_u32(self.move_air.action as u32);
        checksum.hash_u32(u32::from(self.move_air.lifecycle.0));
        checksum.hash_u32(
            self.move_air
                .air_base
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_vec3(
            self.move_air.base_position.x,
            self.move_air.base_position.y,
            self.move_air.base_position.z,
        );
        checksum.hash_vec3(
            self.move_air.pad_position.x,
            self.move_air.pad_position.y,
            self.move_air.pad_position.z,
        );
        checksum.hash_vec3(
            self.move_air.spot_forward.x,
            self.move_air.spot_forward.y,
            self.move_air.spot_forward.z,
        );
        checksum.hash_f32(self.move_air.height_displacement);
        checksum.hash_f32(self.move_air.turn_rate);
        checksum.hash_f32(self.move_air.goal_altitude_increment);
        checksum.hash_f32(self.move_air.current_altitude_increment);
        checksum.hash_f32(self.move_air.previous_altitude_change);
        checksum.hash_f32(self.move_air.altitude_select_timer);
        checksum.hash_f32(self.move_air.speed_select_timer);
        checksum.hash_f32(self.move_air.goal_speed);
        checksum.hash_u32(self.move_air.tactic as u32);
        checksum.hash_vec3(
            self.move_air.goal_position.x,
            self.move_air.goal_position.y,
            self.move_air.goal_position.z,
        );
        checksum.hash_u32(u32::from(self.move_air.goal_position_valid));
        checksum.hash_f32(self.move_air.hover_timer);
        checksum.hash_u32(u32::from(self.move_air.attack_blocked));
    }
}

impl Unit {
    /// Return the movement action selected from this unit's layered prototype.
    #[must_use]
    pub const fn flight_controller_kind(&self) -> FlightControllerKind {
        self.flight.controller
    }

    pub(crate) fn configure_flight_controller(
        &mut self,
        controller: FlightControllerKind,
        height_displacement: f32,
    ) {
        let height_displacement = finite_or_zero(height_displacement);
        if self.flight.controller != controller
            || self.flight.move_air.height_displacement.to_bits() != height_displacement.to_bits()
        {
            self.flight = UnitFlight {
                controller,
                move_air: MoveAirState {
                    height_displacement,
                    ..MoveAirState::default()
                },
            };
        }
    }

    pub(crate) const fn uses_physics_hover(&self) -> bool {
        matches!(self.flight.controller, FlightControllerKind::PhysicsHover)
    }

    pub(crate) const fn uses_move_air(&self) -> bool {
        matches!(self.flight.controller, FlightControllerKind::MoveAir)
    }

    /// Return whether a unit movement action currently owns presentation.
    ///
    /// `MoveAir` is persistent and remains active while orbiting a stationary
    /// squad center, so its action cannot be inferred from `move_target`.
    #[must_use]
    pub fn has_active_move_action(&self) -> bool {
        self.move_target.is_some()
            || self.has_active_ground_move_action()
            || self.is_jumping()
            || self.is_move_air_working()
    }

    /// Return whether `MoveAir` currently prevents the ranged action firing.
    #[must_use]
    pub const fn is_move_air_attack_blocked(&self) -> bool {
        self.uses_move_air() && self.flight.move_air.attack_blocked
    }

    /// Return whether a train-limited `MoveAir` unit is waiting on its base pad.
    #[must_use]
    pub const fn is_move_air_parked(&self) -> bool {
        self.uses_move_air()
            && self.flight.move_air.lifecycle.initialized()
            && matches!(self.flight.move_air.action, MoveAirActionState::None)
            && self.flight.move_air.air_base.is_some()
            && !self.flight.move_air.lifecycle.launch_requested()
    }

    /// Return the train-limit air base captured by `MoveAir` initialization.
    #[must_use]
    pub const fn move_air_base_id(&self) -> Option<EntityId> {
        if self.uses_move_air() {
            self.flight.move_air.air_base
        } else {
            None
        }
    }

    /// Return the base position captured when `MoveAir` initialized.
    #[must_use]
    pub const fn move_air_base_position(&self) -> Option<Vec3> {
        if self.uses_move_air() && self.flight.move_air.lifecycle.initialized() {
            Some(self.flight.move_air.base_position)
        } else {
            None
        }
    }

    /// Return whether a carpet-bomb action ordered this aircraft back to base.
    #[must_use]
    pub const fn is_move_air_returning_to_base(&self) -> bool {
        self.uses_move_air() && self.flight.move_air.lifecycle.returning_to_base()
    }

    pub(crate) fn request_move_air_launch(&mut self) {
        if self.uses_move_air() {
            self.flight.move_air.lifecycle.set_launch_requested(true);
            self.flight.move_air.lifecycle.set_returning_to_base(false);
        }
    }

    pub(crate) fn request_move_air_return(&mut self) {
        if self.uses_move_air() {
            self.flight.move_air.lifecycle.set_returning_to_base(true);
        }
    }

    pub(crate) const fn move_air_state(&self) -> Option<MoveAirState> {
        if self.uses_move_air() {
            Some(self.flight.move_air)
        } else {
            None
        }
    }

    pub(crate) const fn is_move_air_working(&self) -> bool {
        self.uses_move_air() && matches!(self.flight.move_air.action, MoveAirActionState::Working)
    }

    pub(crate) fn set_move_air_state(&mut self, state: MoveAirState) {
        if self.uses_move_air() {
            self.flight.move_air = state;
        }
    }

    pub(crate) fn hash_flight_state(&self, checksum: &mut SyncChecksum) {
        self.flight.hash_state(checksum);
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_identity_is_checksummed() {
        let mut direct = Unit::default();
        let mut hover = direct.clone();
        hover.configure_flight_controller(FlightControllerKind::PhysicsHover, 10.0);

        assert_ne!(hash(&direct), hash(&hover));
        direct.configure_flight_controller(FlightControllerKind::PhysicsHover, 10.0);
        assert_eq!(hash(&direct), hash(&hover));
        assert!(!direct.has_active_move_action());

        direct.configure_flight_controller(FlightControllerKind::MoveAir, 10.0);
        assert!(!direct.has_active_move_action());
        let mut state = direct.move_air_state().unwrap();
        state.action = MoveAirActionState::Working;
        direct.set_move_air_state(state);
        assert!(direct.has_active_move_action());
    }

    #[test]
    fn move_air_tactic_state_is_checksummed() {
        let mut navigating = Unit::default();
        navigating.configure_flight_controller(FlightControllerKind::MoveAir, 10.0);
        let mut hovering = navigating.clone();
        let mut state = hovering.move_air_state().unwrap();
        state.tactic = MoveAirTacticState::LaunchHover;
        state.goal_position = Vec3::new(4.0, 5.0, 6.0);
        state.goal_position_valid = true;
        state.hover_timer = 1.25;
        state.attack_blocked = true;
        state.lifecycle.set_initialized(true);
        state.action = MoveAirActionState::Working;
        state.air_base = Some(EntityId::new(crate::entity_id::EntityClass::Unit, 9));
        state.base_position = Vec3::new(1.0, 2.0, 3.0);
        state.pad_position = Vec3::new(7.0, 8.0, 9.0);
        state.lifecycle.set_pad_position_valid(true);
        state.spot_forward = Vec3::X;
        state.lifecycle.set_launch_requested(true);
        state.lifecycle.set_returning_to_base(true);
        hovering.set_move_air_state(state);

        assert_ne!(hash(&navigating), hash(&hovering));
    }

    fn hash(unit: &Unit) -> u32 {
        let mut checksum = SyncChecksum::new();
        unit.hash_flight_state(&mut checksum);
        checksum.value()
    }
}
