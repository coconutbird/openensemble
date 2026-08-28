//! Individual unit and building entities.
//!
//! Reverse engineering confirms that both mobile units and buildings occupy
//! vanilla's class-1 `BUnit` pool. [`UnitKind`] records the behavioral
//! distinction without inventing a separate entity class.

mod actions;
mod building;
mod combat;
pub mod marine;
mod shields;
pub mod warthog;

pub use actions::UnitActions;
pub use building::{BuildingProduction, ResearchProgress, ResearchTask};
pub use combat::UnitCombat;
pub use shields::{ShieldCoverage, UnitShields};

use super::{BaseEntity, BaseId};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::physics::PhysicsBody;
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

/// Gameplay implementation selected for a unit proto object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitArchetype {
    /// Existing direct-movement behavior for an unimplemented unit type.
    #[default]
    Generic,
    /// Stock `unsc_inf_marine_01` infantry behavior.
    Marine,
    /// Stock `unsc_veh_warthog_01` vehicle behavior.
    Warthog,
}

/// Minimal unit lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitState {
    /// No active order.
    #[default]
    Idle,
    /// Moving toward `move_target`.
    Moving,
    /// Pursuing or engaging `attack_target`.
    Attacking,
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
    /// Gameplay implementation selected from proto metadata.
    pub archetype: UnitArchetype,
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
    /// Integral energy-shield state.
    pub shields: UnitShields,
    /// Live outgoing damage multiplier (veterancy and tech effects layer here).
    pub damage_multiplier: f32,
    /// Live incoming damage multiplier.
    pub damage_taken_multiplier: f32,
    /// Movement speed in world units per second.
    pub speed: f32,
    /// Acceleration in world units per second squared; zero means immediate.
    pub acceleration: f32,
    /// Maximum yaw rate in degrees per second; zero means immediate.
    pub turn_rate_degrees: f32,
    /// Gameplay obstruction radii even when no live rigid body is active.
    pub obstruction_half_extents: Vec3,
    /// Deterministic rigid body, when this object participates in physics.
    pub physics: Option<PhysicsBody>,
    /// Standalone movement target.
    pub move_target: Option<Vec3>,
    /// Standalone attack target, retained as a generational entity ID.
    pub attack_target: Option<EntityId>,
    /// Command-authored attack range override; zero selects tactic range.
    pub attack_range: f32,
    /// Ability database index requested by a standalone attack order.
    pub attack_ability_id: Option<u8>,
    /// Live enablement state for authored tactic actions.
    pub actions: UnitActions,
    /// Per-unit authored attack animation/cooldown state.
    pub combat: UnitCombat,
    /// Research/production work owned by building units.
    pub production: BuildingProduction,
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
            archetype: UnitArchetype::Generic,
            state: UnitState::Idle,
            proto_object_id: -1,
            proto_object_name: String::new(),
            hitpoints: 100.0,
            max_hitpoints: 100.0,
            shields: UnitShields::default(),
            damage_multiplier: 1.0,
            damage_taken_multiplier: 1.0,
            speed: 10.0,
            acceleration: 0.0,
            turn_rate_degrees: 0.0,
            obstruction_half_extents: Vec3::ZERO,
            physics: None,
            move_target: None,
            attack_target: None,
            attack_range: 0.0,
            attack_ability_id: None,
            actions: UnitActions::default(),
            combat: UnitCombat::default(),
            production: BuildingProduction::default(),
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

    /// Check whether movement is controlled by a dynamic physics body.
    #[must_use]
    pub fn is_physics_driven(&self) -> bool {
        self.physics
            .as_ref()
            .is_some_and(|body| body.motion_type() == crate::physics::MotionType::Dynamic)
    }

    /// Set both current and maximum hit points.
    pub fn set_max_hitpoints(&mut self, max_hitpoints: f32) {
        if max_hitpoints.is_finite() && max_hitpoints > 0.0 {
            self.max_hitpoints = max_hitpoints;
            self.hitpoints = max_hitpoints;
        }
    }

    /// Apply positive finite damage through shields, then hit points.
    ///
    /// Returns whether the live unit accepted the damage event.
    pub fn damage(&mut self, amount: f32) -> bool {
        if !amount.is_finite() || amount <= 0.0 || !self.is_alive() {
            return false;
        }
        let hitpoint_damage = self.shields.absorb_damage(amount);
        self.hitpoints = (self.hitpoints - hitpoint_damage).max(0.0);
        if self.hitpoints == 0.0 {
            self.kill();
        }
        true
    }

    /// Kill this unit or building.
    pub fn kill(&mut self) {
        self.hitpoints = 0.0;
        self.state = UnitState::Dead;
        self.base.kill();
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.stop();
    }

    /// Issue a standalone move order.
    ///
    /// Buildings and units currently controlled through a squad reject it.
    pub fn move_to(&mut self, target: Vec3) -> bool {
        if self.is_building() || self.squad_id.is_some() || !self.is_alive() {
            return false;
        }
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.move_target = Some(target);
        self.state = UnitState::Moving;
        true
    }

    /// Issue a standalone attack order.
    pub fn attack(&mut self, target: EntityId, range: f32, ability_id: Option<u8>) -> bool {
        if self.squad_id.is_some() || !self.is_alive() || target.is_invalid() {
            return false;
        }
        self.attack_target = Some(target);
        self.attack_range = valid_attack_range(range);
        self.attack_ability_id = ability_id;
        self.combat.reset();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = UnitState::Attacking;
        true
    }

    /// Cancel the current standalone attack order.
    pub fn clear_attack_order(&mut self) {
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == UnitState::Attacking {
            self.state = UnitState::Idle;
        }
    }

    pub(crate) fn chase_attack_target(&mut self, target: Vec3) {
        if self.state == UnitState::Attacking && !self.is_building() {
            self.move_target = Some(target);
        }
    }

    pub(crate) fn hold_attack_position(&mut self, target: Vec3) {
        if self.state == UnitState::Attacking {
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

    /// Accumulate a force on this unit's physics body.
    pub fn apply_force(&mut self, force: Vec3) -> bool {
        let Some(body) = &mut self.physics else {
            return false;
        };
        body.apply_force(force);
        true
    }

    /// Apply an immediate impulse to this unit's physics body.
    pub fn apply_impulse(&mut self, impulse: Vec3) -> bool {
        let Some(body) = &mut self.physics else {
            return false;
        };
        body.apply_impulse(&mut self.base, impulse);
        true
    }

    /// Apply an immediate impulse at a world-space point.
    pub fn apply_impulse_at_point(&mut self, impulse: Vec3, point: Vec3) -> bool {
        let Some(body) = &mut self.physics else {
            return false;
        };
        body.apply_impulse_at_point(&mut self.base, impulse, point);
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
        if let Some(body) = &mut self.physics {
            if body.update(&mut self.base, self.move_target, dt) {
                self.stop();
            }
            return;
        }
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

    pub(crate) fn move_as_squad_member(&mut self, target: Vec3) {
        if !self.is_building() && self.is_alive() {
            self.move_target = Some(target);
            self.state = UnitState::Moving;
        }
    }
}

impl Entity for Unit {
    fn id(&self) -> EntityId {
        self.base.id
    }

    fn update(&mut self, dt: f32) {
        if self.state == UnitState::Moving
            || (self.state == UnitState::Attacking && self.move_target.is_some())
        {
            self.update_movement(dt);
        }
    }

    fn is_alive(&self) -> bool {
        self.base.is_alive() && self.state != UnitState::Dead && self.hitpoints > 0.0
    }
}

fn valid_attack_range(range: f32) -> f32 {
    if range.is_finite() && range > 0.0 {
        range
    } else {
        0.0
    }
}
