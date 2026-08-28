//! Squad entities - the primary controllable groups.
//!
//! Based on `BSquad` from the original source.
//! A squad is a group of units that move and act together.

pub mod marine;
mod mode;
mod recovery;
mod shields;
pub mod warthog;

pub use mode::SquadMode;
pub use recovery::{RecoveryType, SquadRecovery};
pub use shields::SquadShields;

use super::BaseEntity;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

/// Squad state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadState {
    /// Idle, not doing anything.
    #[default]
    Idle,
    /// Moving to a target position.
    Moving,
    /// Attacking a target.
    Attacking,
    /// Dead/destroyed.
    Dead,
}

/// Gameplay implementation selected for a proto squad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadArchetype {
    /// Existing formation movement for an unimplemented squad type.
    #[default]
    Generic,
    /// Stock four-member Marine squad.
    Marine,
    /// Stock single-vehicle Warthog squad.
    Warthog,
}

/// Formation behavior selected by a proto squad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadFormation {
    /// No specialized formation behavior has been implemented.
    #[default]
    Generic,
    /// Per-member flock transforms and velocities.
    Flock,
}

/// Squad entity - the primary controllable unit in Halo Wars.
///
/// Squads contain unit-pool IDs and provide the controllable group transform.
#[derive(Debug, Clone)]
pub struct Squad {
    /// Base entity data.
    pub base: BaseEntity,
    /// Current state.
    pub state: SquadState,
    /// Gameplay implementation selected from the proto-squad name.
    pub archetype: SquadArchetype,
    /// Formation behavior selected from the proto-squad metadata.
    pub formation: SquadFormation,
    /// Movement target position (if moving).
    pub move_target: Option<Vec3>,
    /// Unit or squad currently targeted by an attack order.
    pub attack_target: Option<EntityId>,
    /// Command-authored attack range override; zero selects tactic range.
    pub attack_range: f32,
    /// Current retail squad mode used by tactic target rules.
    pub mode: SquadMode,
    /// Ability database index requested by the current attack order.
    pub attack_ability_id: Option<u8>,
    /// Active movement, attack, or command-ability recovery channel.
    pub recovery: SquadRecovery,
    /// Shared post-damage timer for member shield recharge.
    pub shields: SquadShields,
    /// Movement speed (units per second).
    pub speed: f32,
    /// Squad locomotion acceleration; zero means immediate.
    pub acceleration: f32,
    /// Squad yaw limit in degrees per second; zero means immediate.
    pub turn_rate_degrees: f32,
    /// Proto squad ID (type of squad).
    pub proto_squad_id: i32,
    /// Proto-squad name retained for diagnostics and deterministic checksums.
    pub proto_squad_name: String,
    /// Nominal pathing turn radius from the proto squad.
    pub turn_radius: f32,
    /// Minimum pathing turn radius from the proto squad.
    pub min_turn_radius: f32,
    /// Maximum pathing turn radius from the proto squad.
    pub max_turn_radius: f32,
    /// Units in this squad, sorted by entity ID for deterministic iteration.
    pub unit_ids: Vec<EntityId>,
    /// Members that completed the current command-ability attack cycle.
    ability_used_unit_ids: Vec<EntityId>,
}

impl Default for Squad {
    fn default() -> Self {
        Self {
            base: BaseEntity::default(),
            state: SquadState::Idle,
            archetype: SquadArchetype::Generic,
            formation: SquadFormation::Generic,
            move_target: None,
            attack_target: None,
            attack_range: 0.0,
            mode: SquadMode::Normal,
            attack_ability_id: None,
            recovery: SquadRecovery::default(),
            shields: SquadShields::default(),
            speed: 10.0, // Default speed
            acceleration: 0.0,
            turn_rate_degrees: 0.0,
            proto_squad_id: -1,
            proto_squad_name: String::new(),
            turn_radius: 0.0,
            min_turn_radius: 0.0,
            max_turn_radius: 0.0,
            unit_ids: Vec::new(),
            ability_used_unit_ids: Vec::new(),
        }
    }
}

impl Squad {
    /// Create a new squad with the given ID and player.
    #[must_use]
    pub fn new(id: EntityId, player_id: PlayerId) -> Self {
        Self {
            base: BaseEntity::new(id, player_id),
            ..Default::default()
        }
    }

    /// Set the squad's position.
    pub fn set_position(&mut self, pos: Vec3) {
        self.base.set_position(pos);
    }

    /// Get the squad's position.
    #[must_use]
    pub fn position(&self) -> Vec3 {
        self.base.position
    }

    /// Issue a move order to the given position.
    pub fn move_to(&mut self, target: Vec3) {
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.move_target = Some(target);
        self.state = SquadState::Moving;
    }

    /// Issue an attack order against a generational entity ID.
    pub fn attack(
        &mut self,
        target: EntityId,
        range: f32,
        mode: Option<SquadMode>,
        ability_id: Option<u8>,
    ) -> bool {
        if !self.is_alive() || target.is_invalid() {
            return false;
        }
        self.attack_target = Some(target);
        self.attack_range = valid_attack_range(range);
        if let Some(mode) = mode {
            self.mode = mode;
        }
        self.attack_ability_id = ability_id;
        self.ability_used_unit_ids.clear();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = SquadState::Attacking;
        true
    }

    /// Cancel the active attack order.
    pub fn clear_attack_order(&mut self) {
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == SquadState::Attacking {
            self.state = SquadState::Idle;
        }
    }

    pub(crate) fn chase_attack_target(&mut self, target: Vec3) {
        if self.state == SquadState::Attacking {
            self.move_target = Some(target);
        }
    }

    pub(crate) fn hold_attack_position(&mut self, target: Vec3) {
        if self.state == SquadState::Attacking {
            self.move_target = None;
            self.base.velocity = Vec3::ZERO;
            let direction = Vec3::new(
                target.x - self.base.position.x,
                0.0,
                target.z - self.base.position.z,
            )
            .normalize_or_zero();
            if direction != Vec3::ZERO {
                self.base.set_forward(direction);
            }
        }
    }

    /// Stop moving.
    pub fn stop(&mut self) {
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == SquadState::Moving {
            self.state = SquadState::Idle;
        }
    }

    /// Check if the squad is moving.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.state == SquadState::Moving
    }

    /// Add a unit ID while preserving deterministic sorted order.
    ///
    /// Returns `true` when the unit was newly added.
    pub fn add_unit(&mut self, unit_id: EntityId) -> bool {
        match self.unit_ids.binary_search(&unit_id) {
            Ok(_) => false,
            Err(index) => {
                self.unit_ids.insert(index, unit_id);
                true
            }
        }
    }

    /// Remove a unit ID from this squad.
    pub fn remove_unit(&mut self, unit_id: EntityId) -> bool {
        let Ok(index) = self.unit_ids.binary_search(&unit_id) else {
            return false;
        };
        self.unit_ids.remove(index);
        if let Ok(index) = self.ability_used_unit_ids.binary_search(&unit_id) {
            self.ability_used_unit_ids.remove(index);
        }
        true
    }

    /// Check whether this squad contains a unit.
    #[must_use]
    pub fn contains_unit(&self, unit_id: EntityId) -> bool {
        self.unit_ids.binary_search(&unit_id).is_ok()
    }

    pub(crate) fn unit_completed_ability(&self, unit_id: EntityId) -> bool {
        self.ability_used_unit_ids.binary_search(&unit_id).is_ok()
    }

    pub(crate) fn mark_unit_ability_complete(&mut self, unit_id: EntityId) {
        if let Err(index) = self.ability_used_unit_ids.binary_search(&unit_id) {
            self.ability_used_unit_ids.insert(index, unit_id);
        }
    }

    pub(crate) fn ability_complete_for(&self, participants: &[EntityId]) -> bool {
        !participants.is_empty()
            && participants
                .iter()
                .all(|unit_id| self.unit_completed_ability(*unit_id))
    }

    pub(crate) fn finish_ability_execution(
        &mut self,
        recovery_type: Option<RecoveryType>,
        recovery_time: f32,
        ability_id: Option<u8>,
    ) {
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        if let Some(recovery_type) = recovery_type {
            self.recovery
                .start(recovery_type, recovery_time, ability_id);
        }
    }

    pub(crate) fn update_recovery(&mut self, dt: f32) {
        self.recovery.advance(dt);
    }

    pub(crate) fn hash_ability_execution(&self, checksum: &mut crate::sync::SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.ability_used_unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.ability_used_unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }

    /// Update movement for one tick.
    ///
    /// Returns true if the squad reached its destination.
    pub fn update_movement(&mut self, dt: f32) -> bool {
        const ARRIVAL_THRESHOLD: f32 = 0.5;

        let Some(target) = self.move_target else {
            return false;
        };

        let to_target = target - self.base.position;
        let distance = to_target.length();

        if distance < ARRIVAL_THRESHOLD {
            // Arrived at destination
            self.base.position = target;
            self.stop();
            return true;
        }

        let direction = to_target / distance;
        self.base.forward = turn_toward(self.base.forward, direction, self.turn_rate_degrees, dt);
        let current_speed = self.base.velocity.length();
        let desired_speed = desired_speed(self.speed, self.acceleration, distance);
        let next_speed = approach_speed(current_speed, desired_speed, self.acceleration, dt);
        let move_distance = next_speed * dt;

        if move_distance >= distance && self.base.forward.dot(direction) > 0.999 {
            // Would overshoot, just arrive
            self.base.position = target;
            self.stop();
            return true;
        }

        // Update position and velocity
        self.base.velocity = self.base.forward * next_speed;
        self.base.position += self.base.velocity * dt;

        false
    }
}

fn desired_speed(max_speed: f32, acceleration: f32, distance: f32) -> f32 {
    if acceleration <= 0.0 {
        return max_speed;
    }
    let remaining = (distance - 0.5).max(0.0);
    (2.0 * acceleration * remaining).sqrt().min(max_speed)
}

fn approach_speed(current: f32, target: f32, acceleration: f32, dt: f32) -> f32 {
    if acceleration <= 0.0 {
        return target;
    }
    let delta = acceleration * dt;
    if current < target {
        (current + delta).min(target)
    } else {
        (current - delta).max(target)
    }
}

fn turn_toward(current: Vec3, desired: Vec3, degrees_per_second: f32, dt: f32) -> Vec3 {
    if degrees_per_second <= 0.0 {
        return desired;
    }
    let current = Vec3::new(current.x, 0.0, current.z).normalize_or_zero();
    let current = if current == Vec3::ZERO {
        Vec3::Z
    } else {
        current
    };
    let desired = Vec3::new(desired.x, 0.0, desired.z).normalize_or_zero();
    let dot = current.dot(desired).clamp(-1.0, 1.0);
    let cross_y = current.z.mul_add(desired.x, -current.x * desired.z);
    let angle = cross_y.atan2(dot);
    let limit = degrees_per_second.to_radians() * dt;
    let (sin, cos) = angle.clamp(-limit, limit).sin_cos();
    Vec3::new(
        current.x.mul_add(cos, current.z * sin),
        0.0,
        (-current.x).mul_add(sin, current.z * cos),
    )
}

pub(crate) fn formation_offset_to_world(forward: Vec3, offset: Vec3) -> Vec3 {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let forward = if forward == Vec3::ZERO {
        Vec3::Z
    } else {
        forward
    };
    let right = Vec3::Y.cross(forward);
    right * offset.x + Vec3::Y * offset.y + forward * offset.z
}

pub(crate) fn formation_offset_to_local(forward: Vec3, offset: Vec3) -> Vec3 {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let forward = if forward == Vec3::ZERO {
        Vec3::Z
    } else {
        forward
    };
    let right = Vec3::Y.cross(forward);
    Vec3::new(offset.dot(right), offset.y, offset.dot(forward))
}

impl Entity for Squad {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        if self.state == SquadState::Moving
            || (self.state == SquadState::Attacking && self.move_target.is_some())
        {
            self.update_movement(dt);
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive() && self.state != SquadState::Dead
    }
}

fn valid_attack_range(range: f32) -> f32 {
    if range.is_finite() && range > 0.0 {
        range
    } else {
        0.0
    }
}
