//! Squad entity - the primary controllable unit.
//!
//! Based on `BSquad` from the original source.
//! A squad is a group of units that move and act together.

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

/// Squad entity - the primary controllable unit in Halo Wars.
///
/// Squads contain unit-pool IDs and provide the controllable group transform.
#[derive(Debug, Clone)]
pub struct Squad {
    /// Base entity data.
    pub base: BaseEntity,
    /// Current state.
    pub state: SquadState,
    /// Movement target position (if moving).
    pub move_target: Option<Vec3>,
    /// Movement speed (units per second).
    pub speed: f32,
    /// Proto squad ID (type of squad).
    pub proto_squad_id: i32,
    /// Units in this squad, sorted by entity ID for deterministic iteration.
    pub unit_ids: Vec<EntityId>,
}

impl Default for Squad {
    fn default() -> Self {
        Self {
            base: BaseEntity::default(),
            state: SquadState::Idle,
            move_target: None,
            speed: 10.0, // Default speed
            proto_squad_id: -1,
            unit_ids: Vec::new(),
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
        self.move_target = Some(target);
        self.state = SquadState::Moving;
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
        true
    }

    /// Check whether this squad contains a unit.
    #[must_use]
    pub fn contains_unit(&self, unit_id: EntityId) -> bool {
        self.unit_ids.binary_search(&unit_id).is_ok()
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

        // Move toward target
        let direction = to_target / distance;
        let move_distance = self.speed * dt;

        if move_distance >= distance {
            // Would overshoot, just arrive
            self.base.position = target;
            self.stop();
            return true;
        }

        // Update position and velocity
        self.base.velocity = direction * self.speed;
        self.base.position += direction * move_distance;
        self.base.set_forward(direction);

        false
    }
}

impl Entity for Squad {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        if self.state == SquadState::Moving {
            self.update_movement(dt);
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive() && self.state != SquadState::Dead
    }
}
