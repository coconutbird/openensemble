//! Individual units and buildings.
//!
//! Reverse engineering confirms that both mobile units and buildings occupy
//! vanilla's class-1 `BUnit` pool. [`UnitKind`] records the behavioral
//! distinction without inventing a separate entity class.

use super::{BaseEntity, BaseId};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

const ARRIVAL_THRESHOLD: f32 = 0.5;

/// Behavioral kind of a class-1 entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitKind {
    /// A mobile individual unit.
    #[default]
    Mobile,
    /// An immobile building, still stored in the unit pool.
    Building,
}

/// Minimal unit lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitState {
    /// No active order.
    #[default]
    Idle,
    /// Moving toward `move_target`.
    Moving,
    /// Dead and ready for removal.
    Dead,
}

/// An individual mobile unit or building.
#[derive(Debug, Clone)]
pub struct Unit {
    /// Common entity state.
    pub base: BaseEntity,
    /// Mobile unit versus building behavior.
    pub kind: UnitKind,
    /// Current lifecycle/order state.
    pub state: UnitState,
    /// Database proto-object ID, or `-1` when unresolved.
    pub proto_object_id: i32,
    /// Proto-object name retained for diagnostics and deterministic checksums.
    pub proto_object_name: String,
    /// Current hit points.
    pub hitpoints: f32,
    /// Maximum hit points.
    pub max_hitpoints: f32,
    /// Movement speed in world units per second.
    pub speed: f32,
    /// Standalone movement target.
    pub move_target: Option<Vec3>,
    /// Squad containing this unit, if any.
    pub squad_id: Option<EntityId>,
    /// Base containing this building, if any.
    pub base_id: Option<BaseId>,
    /// Position relative to the owning squad's origin.
    pub formation_offset: Vec3,
}

impl Default for Unit {
    fn default() -> Self {
        Self {
            base: BaseEntity::default(),
            kind: UnitKind::Mobile,
            state: UnitState::Idle,
            proto_object_id: -1,
            proto_object_name: String::new(),
            hitpoints: 100.0,
            max_hitpoints: 100.0,
            speed: 10.0,
            move_target: None,
            squad_id: None,
            base_id: None,
            formation_offset: Vec3::ZERO,
        }
    }
}

impl Unit {
    /// Create a mobile unit.
    #[must_use]
    pub fn new(id: EntityId, player_id: PlayerId) -> Self {
        Self {
            base: BaseEntity::new(id, player_id),
            ..Self::default()
        }
    }

    /// Create an immobile building in the unit pool.
    #[must_use]
    pub fn new_building(id: EntityId, player_id: PlayerId) -> Self {
        Self {
            base: BaseEntity::new(id, player_id),
            kind: UnitKind::Building,
            speed: 0.0,
            ..Self::default()
        }
    }

    /// Check whether this unit is a building.
    #[must_use]
    pub fn is_building(&self) -> bool {
        self.kind == UnitKind::Building
    }

    /// Set both current and maximum hit points.
    pub fn set_max_hitpoints(&mut self, max_hitpoints: f32) {
        if max_hitpoints.is_finite() && max_hitpoints > 0.0 {
            self.max_hitpoints = max_hitpoints;
            self.hitpoints = max_hitpoints;
        }
    }

    /// Apply positive finite damage, killing the unit at zero hit points.
    pub fn damage(&mut self, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 || !self.is_alive() {
            return;
        }
        self.hitpoints = (self.hitpoints - amount).max(0.0);
        if self.hitpoints == 0.0 {
            self.kill();
        }
    }

    /// Kill this unit or building.
    pub fn kill(&mut self) {
        self.hitpoints = 0.0;
        self.state = UnitState::Dead;
        self.base.kill();
        self.stop();
    }

    /// Issue a standalone move order.
    ///
    /// Buildings and units currently controlled through a squad reject it.
    pub fn move_to(&mut self, target: Vec3) -> bool {
        if self.is_building() || self.squad_id.is_some() || !self.is_alive() {
            return false;
        }
        self.move_target = Some(target);
        self.state = UnitState::Moving;
        true
    }

    /// Stop standalone movement.
    pub fn stop(&mut self) {
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == UnitState::Moving {
            self.state = UnitState::Idle;
        }
    }

    fn update_movement(&mut self, dt: f32) {
        let Some(target) = self.move_target else {
            return;
        };
        let to_target = target - self.base.position;
        let distance = to_target.length();
        if distance < ARRIVAL_THRESHOLD || self.speed * dt >= distance {
            self.base.position = target;
            self.stop();
            return;
        }
        let direction = to_target / distance;
        self.base.velocity = direction * self.speed;
        self.base.position += self.base.velocity * dt;
        self.base.set_forward(direction);
    }
}

impl Entity for Unit {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        if self.state == UnitState::Moving {
            self.update_movement(dt);
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive() && self.state != UnitState::Dead && self.hitpoints > 0.0
    }
}
