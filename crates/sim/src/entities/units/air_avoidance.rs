//! Authoritative per-aircraft avoidance and lethal-crash state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::gameplay::AirAvoidanceActionProfile;
use crate::player::{PlayerId, TeamId};
use crate::sync::SyncChecksum;
use glam::Vec3;

const HOVER_LOOK_AHEAD_SAMPLES: usize = 5;
const EMPTY_HOVER_LOOK_AHEAD_HEIGHT: f32 = -10_000.0;

/// Retail aircraft crash lifecycle exposed to renderer and UI clients.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AircraftCrashPhase {
    /// Ordinary flight and collision avoidance.
    #[default]
    Inactive,
    /// Lethal damage was intercepted; crash targeting begins next action update.
    PendingTarget,
    /// The aircraft is descending toward a target or random crash point.
    Crashing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AirObstructionState {
    #[default]
    Passable,
    Blocking,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AirAvoidanceFlags(u8);

impl AirAvoidanceFlags {
    const SPEED_LIMITED: u8 = 1 << 0;
    const CAN_KAMIKAZE: u8 = 1 << 1;
    const DETONATE_ON_DEATH: u8 = 1 << 2;
    const AVOIDING: u8 = 1 << 3;
    const AVOIDING_FRIENDLY: u8 = 1 << 4;
    const CRASH_UNTARGETABLE: u8 = 1 << 5;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

/// Persistent state owned by one retail `AvoidCollisionAir` action.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnitAirAvoidance {
    pub(crate) action_name: Option<String>,
    pub(crate) phase: AircraftCrashPhase,
    pub(crate) avoidance_vector: Vec3,
    pub(crate) previous_position: Vec3,
    pub(crate) crash_position: Vec3,
    pub(crate) kamikaze_target: Option<EntityId>,
    pub(crate) killer_unit: Option<EntityId>,
    pub(crate) killer_player: Option<PlayerId>,
    pub(crate) killer_team: Option<TeamId>,
    pub(crate) detonate_at_ms: Option<u32>,
    birth_seconds: f32,
    pub(crate) nearest_obstacle_altitude: f32,
    pub(crate) vertical_avoidance_offset: f32,
    pub(crate) hover_vertical_velocity: f32,
    hover_look_ahead_heights: [f32; HOVER_LOOK_AHEAD_SAMPLES],
    hover_look_ahead_index: u8,
    flags: AirAvoidanceFlags,
}

impl Default for UnitAirAvoidance {
    fn default() -> Self {
        let mut flags = AirAvoidanceFlags::default();
        flags.set(AirAvoidanceFlags::CAN_KAMIKAZE, true);
        Self {
            action_name: None,
            phase: AircraftCrashPhase::Inactive,
            avoidance_vector: Vec3::ZERO,
            previous_position: Vec3::ZERO,
            crash_position: Vec3::ZERO,
            kamikaze_target: None,
            killer_unit: None,
            killer_player: None,
            killer_team: None,
            detonate_at_ms: None,
            birth_seconds: 0.0,
            nearest_obstacle_altitude: 0.0,
            vertical_avoidance_offset: 0.0,
            hover_vertical_velocity: 0.0,
            hover_look_ahead_heights: [EMPTY_HOVER_LOOK_AHEAD_HEIGHT; HOVER_LOOK_AHEAD_SAMPLES],
            hover_look_ahead_index: 0,
            flags,
        }
    }
}

impl UnitAirAvoidance {
    pub(crate) fn reconcile(
        &mut self,
        profile: Option<&AirAvoidanceActionProfile>,
        position: Vec3,
    ) {
        if self.phase != AircraftCrashPhase::Inactive {
            return;
        }
        let action_name = profile.map(AirAvoidanceActionProfile::action_name);
        if self.action_name.as_deref() == action_name {
            return;
        }
        let transitioning_between_actions = self.action_name.is_some() && action_name.is_some();
        self.action_name = action_name.map(str::to_owned);
        self.previous_position = position;
        self.birth_seconds = 0.0;
        self.avoidance_vector = Vec3::ZERO;
        self.nearest_obstacle_altitude = 0.0;
        if !transitioning_between_actions {
            self.vertical_avoidance_offset = 0.0;
            self.hover_vertical_velocity = 0.0;
            self.hover_look_ahead_heights =
                [EMPTY_HOVER_LOOK_AHEAD_HEIGHT; HOVER_LOOK_AHEAD_SAMPLES];
            self.hover_look_ahead_index = 0;
        }
        self.flags
            .set(AirAvoidanceFlags::SPEED_LIMITED, profile.is_some());
        self.flags.set(
            AirAvoidanceFlags::DETONATE_ON_DEATH,
            profile.is_some_and(AirAvoidanceActionProfile::detonate_on_death),
        );
        self.flags.set(AirAvoidanceFlags::AVOIDING, false);
        self.flags.set(AirAvoidanceFlags::AVOIDING_FRIENDLY, false);
        self.flags.set(AirAvoidanceFlags::CRASH_UNTARGETABLE, false);
    }

    pub(crate) fn advance_birth(&mut self, dt: f32) {
        if !self.flags.contains(AirAvoidanceFlags::SPEED_LIMITED) {
            return;
        }
        if self.birth_seconds > 1.5 {
            self.flags.set(AirAvoidanceFlags::SPEED_LIMITED, false);
        }
        self.birth_seconds += dt;
    }

    pub(crate) fn intercept_lethal_damage(&mut self) -> bool {
        if self.phase != AircraftCrashPhase::Inactive
            || self.action_name.is_none()
            || self.flags.contains(AirAvoidanceFlags::DETONATE_ON_DEATH)
            || !self.flags.contains(AirAvoidanceFlags::CAN_KAMIKAZE)
        {
            return false;
        }
        self.phase = AircraftCrashPhase::PendingTarget;
        true
    }

    pub(crate) fn set_killer(
        &mut self,
        unit: Option<EntityId>,
        player: Option<PlayerId>,
        team: Option<TeamId>,
    ) {
        if self.phase == AircraftCrashPhase::PendingTarget {
            self.killer_unit = unit;
            self.killer_player = player;
            self.killer_team = team;
        }
    }

    pub(crate) fn start_crashing(
        &mut self,
        target: Option<EntityId>,
        crash_position: Vec3,
    ) -> bool {
        if self.phase != AircraftCrashPhase::PendingTarget || !crash_position.is_finite() {
            return false;
        }
        self.phase = AircraftCrashPhase::Crashing;
        self.kamikaze_target = target;
        self.crash_position = crash_position;
        true
    }

    pub(crate) fn begin_crash_update(&mut self, detonate_at_ms: u32) {
        if self.phase == AircraftCrashPhase::Crashing && self.detonate_at_ms.is_none() {
            self.detonate_at_ms = Some(detonate_at_ms);
            self.flags.set(AirAvoidanceFlags::CRASH_UNTARGETABLE, true);
        }
    }

    pub(crate) fn clear_dead_kamikaze_target(&mut self) {
        self.kamikaze_target = None;
    }

    pub(crate) fn set_can_kamikaze(&mut self, enabled: bool) {
        if self.phase == AircraftCrashPhase::Inactive {
            self.flags.set(AirAvoidanceFlags::CAN_KAMIKAZE, enabled);
        }
    }

    pub(crate) fn set_avoidance(
        &mut self,
        vector: Vec3,
        nearest_altitude: f32,
        avoiding: bool,
        friendly: bool,
    ) {
        self.avoidance_vector = vector;
        self.nearest_obstacle_altitude = nearest_altitude;
        self.flags.set(AirAvoidanceFlags::AVOIDING, avoiding);
        self.flags
            .set(AirAvoidanceFlags::AVOIDING_FRIENDLY, friendly);
    }

    pub(crate) fn update_vertical_avoidance(&mut self, altitude: f32) {
        if self.flags.contains(AirAvoidanceFlags::AVOIDING)
            && altitude >= self.nearest_obstacle_altitude
            && altitude <= self.nearest_obstacle_altitude + 30.0
        {
            self.vertical_avoidance_offset = 20.0 - (altitude - self.nearest_obstacle_altitude);
        } else if self.vertical_avoidance_offset > 0.1 {
            self.vertical_avoidance_offset -= 0.05;
        } else if self.vertical_avoidance_offset < -0.1 {
            self.vertical_avoidance_offset += 0.05;
        } else {
            self.vertical_avoidance_offset = 0.0;
        }
    }

    pub(crate) fn hover_look_ahead_index(&self) -> usize {
        usize::from(self.hover_look_ahead_index)
    }

    pub(crate) fn update_hover_look_ahead(&mut self, height: Option<f32>) -> f32 {
        let index = self.hover_look_ahead_index();
        if let Some(height) = height.filter(|height| height.is_finite()) {
            self.hover_look_ahead_heights[index] = height;
        }
        let maximum = self
            .hover_look_ahead_heights
            .iter()
            .copied()
            .fold(EMPTY_HOVER_LOOK_AHEAD_HEIGHT, f32::max);
        self.hover_look_ahead_index = u8::try_from((index + 1) % HOVER_LOOK_AHEAD_SAMPLES)
            .expect("the retail hover look-ahead ring has five entries");
        maximum
    }

    pub(crate) fn finish(&mut self) {
        self.phase = AircraftCrashPhase::Inactive;
        self.avoidance_vector = Vec3::ZERO;
        self.kamikaze_target = None;
        self.detonate_at_ms = None;
        self.flags.set(AirAvoidanceFlags::CRASH_UNTARGETABLE, false);
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(name) = &self.action_name {
            checksum.hash_u32(u32::try_from(name.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(name.as_bytes());
        } else {
            checksum.hash_u32(u32::MAX);
        }
        checksum.hash_u32(self.phase as u32);
        hash_vec3(checksum, self.avoidance_vector);
        hash_vec3(checksum, self.previous_position);
        hash_vec3(checksum, self.crash_position);
        checksum.hash_u32(self.kamikaze_target.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.killer_unit.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.killer_player.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.killer_team.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.detonate_at_ms.unwrap_or(u32::MAX));
        checksum.hash_f32(self.birth_seconds);
        checksum.hash_f32(self.nearest_obstacle_altitude);
        checksum.hash_f32(self.vertical_avoidance_offset);
        checksum.hash_f32(self.hover_vertical_velocity);
        for height in self.hover_look_ahead_heights {
            checksum.hash_f32(height);
        }
        checksum.hash_u32(u32::from(self.hover_look_ahead_index));
        checksum.hash_u32(u32::from(self.flags.0));
    }
}

impl Unit {
    /// Return the authored reverse speed used by aircraft attack positioning.
    ///
    /// Retail uses `-1` when the field is absent, meaning maximum forward speed.
    #[must_use]
    pub const fn reverse_speed(&self) -> f32 {
        match self.reverse_speed {
            Some(speed) => speed,
            None => -1.0,
        }
    }

    pub(crate) fn configure_air_navigation(
        &mut self,
        reverse_speed: Option<f32>,
        obstructs_air: bool,
    ) {
        self.reverse_speed = reverse_speed;
        self.air_obstruction = if obstructs_air {
            AirObstructionState::Blocking
        } else {
            AirObstructionState::Passable
        };
    }

    pub(crate) fn can_reverse_for_air_avoidance(&self) -> bool {
        self.reverse_speed.is_none_or(|speed| speed > 0.1)
    }

    pub(crate) const fn obstructs_air(&self) -> bool {
        matches!(self.air_obstruction, AirObstructionState::Blocking)
    }

    /// Return the authoritative aircraft crash phase.
    #[must_use]
    pub const fn aircraft_crash_phase(&self) -> AircraftCrashPhase {
        self.air_avoidance.phase
    }

    /// Return whether lethal damage has put this aircraft into its crash action.
    #[must_use]
    pub const fn is_crashing(&self) -> bool {
        !matches!(self.air_avoidance.phase, AircraftCrashPhase::Inactive)
    }

    /// Horizontal avoidance displacement computed by the simulation.
    #[must_use]
    pub const fn air_avoidance_vector(&self) -> Vec3 {
        self.air_avoidance.avoidance_vector
    }

    /// Whether retail's initial 8-unit-per-second aircraft cap is active.
    #[must_use]
    pub const fn is_air_speed_limited(&self) -> bool {
        self.air_avoidance
            .flags
            .contains(AirAvoidanceFlags::SPEED_LIMITED)
    }

    /// Current unit selected as this aircraft's kamikaze target.
    #[must_use]
    pub const fn kamikaze_target(&self) -> Option<EntityId> {
        self.air_avoidance.kamikaze_target
    }

    pub(crate) const fn is_air_crash_untargetable(&self) -> bool {
        self.air_avoidance
            .flags
            .contains(AirAvoidanceFlags::CRASH_UNTARGETABLE)
    }

    pub(crate) fn intercept_lethal_aircraft_damage(&mut self) -> bool {
        if !self.air_avoidance.intercept_lethal_damage() {
            return false;
        }
        self.base.set_selectable(false);
        true
    }

    pub(crate) fn set_aircraft_can_kamikaze(&mut self, enabled: bool) {
        self.air_avoidance.set_can_kamikaze(enabled);
    }

    pub(crate) fn hash_air_avoidance_state(&self, checksum: &mut SyncChecksum) {
        self.air_avoidance.hash_state(checksum);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}
