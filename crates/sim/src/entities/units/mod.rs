//! Individual unit and building entities.
//!
//! Reverse engineering confirms that both mobile units and buildings occupy
//! vanilla's class-1 `BUnit` pool. [`UnitKind`] records the behavioral
//! distinction without inventing a separate entity class.

mod actions;
mod building;
mod combat;
mod garrison;
pub mod marine;
pub(crate) mod rally_points;
mod scalars;
mod shields;
mod tower_wall;
pub mod warthog;

pub use actions::UnitActions;
pub use building::{
    BuildingProduction, ConstructionKind, ConstructionProgress, ConstructionTask, ResearchProgress,
    ResearchTask, TrainingKind, TrainingProgress, TrainingTask,
};
pub(crate) use building::{ProductionTask, TriggerCommandStateRef};
pub use combat::UnitCombat;
pub use garrison::UnitGarrison;
pub use rally_points::RallyPoint;
pub use scalars::UnitDataScalar;
pub use shields::{ShieldCoverage, UnitShields};
pub use tower_wall::TowerWallAction;

use super::{BaseEntity, BaseId, EntityIdle, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::physics::PhysicsBody;
use crate::player::{PlayerId, PopulationCost};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MovementFacing {
    #[default]
    Forward,
    Reverse,
}

/// An individual mobile unit or building.
#[derive(Debug, Clone)]
pub struct Unit {
    /// Common entity state.
    pub base: BaseEntity,
    /// Runtime state inherited from retail `BObject`.
    pub object_state: ObjectState,
    /// Mobile unit versus building behavior.
    pub kind: UnitKind,
    /// Gameplay implementation selected from proto metadata.
    pub archetype: UnitArchetype,
    /// Current lifecycle/order state.
    pub state: UnitState,
    /// Retail `EntityIdle` action presence and elapsed duration.
    pub(crate) idle: EntityIdle,
    /// Database proto-object ID, or `-1` when unresolved.
    pub proto_object_id: i32,
    /// Proto-object name retained for diagnostics and deterministic checksums.
    pub proto_object_name: String,
    /// Authored object-type memberships used by containment and targeting rules.
    pub object_types: Vec<String>,
    /// Retail flying flag derived from the prototype movement type.
    pub flying: bool,
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
    /// Live ranged-attack accuracy multiplier.
    pub accuracy_scalar: f32,
    /// Live research, training, and construction work-rate multiplier.
    pub work_rate_scalar: f32,
    /// Live line-of-sight radius multiplier.
    pub line_of_sight_scalar: f32,
    /// Live movement speed and acceleration multiplier.
    pub velocity_scalar: f32,
    /// Live authored weapon-range multiplier.
    pub weapon_range_scalar: f32,
    /// Whether automatic target acquisition may choose this object.
    auto_attackable: bool,
    /// Whether movement keeps this unit facing opposite its travel direction.
    movement_facing: MovementFacing,
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
    /// Containment state and immutable container capabilities.
    pub garrison: UnitGarrison,
    /// Per-unit authored attack animation/cooldown state.
    pub combat: UnitCombat,
    /// Persistent retail tower-wall action after a destination is assigned.
    pub tower_wall: Option<TowerWallAction>,
    /// Research/production work owned by building units.
    pub production: BuildingProduction,
    /// Primary and co-op rally destinations retained by this unit.
    rally_points: rally_points::UnitRallyPoints,
    /// Whether construction has completed and built-state effects are active.
    pub built: bool,
    /// Unit whose command created this building.
    pub built_by: Option<EntityId>,
    /// Concrete socket entity supplied by a direct build command.
    pub build_socket_id: Option<EntityId>,
    /// Virtual child-socket index selected by `BuildOther`.
    pub build_socket_index: Option<u16>,
    /// Building currently plugged into this socket unit.
    pub(crate) socket_plug_id: Option<EntityId>,
    /// Unit whose authored child-object list created this socket.
    pub(crate) socket_parent_id: Option<EntityId>,
    /// Authored socket units associated with this unit, in entity-ref order.
    pub(crate) associated_socket_ids: Vec<EntityId>,
    /// Socket position in its parent's right/up/forward coordinate frame.
    pub(crate) socket_local_offset: Vec3,
    /// Socket yaw relative to its parent's facing, in degrees.
    pub(crate) socket_local_yaw_degrees: f32,
    /// Live population charged to this standalone object.
    pub population_costs: Vec<PopulationCost>,
    /// Population-cap additions supplied while this object remains alive.
    pub population_cap_additions: Vec<PopulationCost>,
    /// Building whose production queue created this standalone object.
    pub trained_by: Option<EntityId>,
    /// Shared authored train-limit bucket, when one linked this object.
    pub train_limit_bucket: Option<u8>,
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
            object_state: ObjectState::default(),
            kind: UnitKind::Mobile,
            archetype: UnitArchetype::Generic,
            state: UnitState::Idle,
            idle: EntityIdle::default(),
            proto_object_id: -1,
            proto_object_name: String::new(),
            object_types: Vec::new(),
            flying: false,
            hitpoints: 100.0,
            max_hitpoints: 100.0,
            shields: UnitShields::default(),
            damage_multiplier: 1.0,
            damage_taken_multiplier: 1.0,
            accuracy_scalar: 1.0,
            work_rate_scalar: 1.0,
            line_of_sight_scalar: 1.0,
            velocity_scalar: 1.0,
            weapon_range_scalar: 1.0,
            auto_attackable: true,
            movement_facing: MovementFacing::Forward,
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
            garrison: UnitGarrison::default(),
            combat: UnitCombat::default(),
            tower_wall: None,
            production: BuildingProduction::default(),
            rally_points: rally_points::UnitRallyPoints::default(),
            built: true,
            built_by: None,
            build_socket_id: None,
            build_socket_index: None,
            socket_plug_id: None,
            socket_parent_id: None,
            associated_socket_ids: Vec::new(),
            socket_local_offset: Vec3::ZERO,
            socket_local_yaw_degrees: 0.0,
            population_costs: Vec::new(),
            population_cap_additions: Vec::new(),
            trained_by: None,
            train_limit_bucket: None,
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
        let mut building = Self {
            base: BaseEntity::new(id, player_id),
            kind: UnitKind::Building,
            speed: 0.0,
            ..Self::default()
        };
        building.base.configure_prototype_mobility(true);
        building
    }

    /// Check whether this unit is a building.
    #[must_use]
    pub fn is_building(&self) -> bool {
        self.kind == UnitKind::Building
    }

    /// Check whether this unit can perform completed-unit gameplay actions.
    #[must_use]
    pub fn is_operational(&self) -> bool {
        self.is_alive() && !self.is_garrisoned() && (!self.is_building() || self.built)
    }

    /// Return whether automatic combat acquisition may target this object.
    #[must_use]
    pub const fn is_auto_attackable(&self) -> bool {
        self.auto_attackable
    }

    pub(crate) fn set_auto_attackable(&mut self, auto_attackable: bool) {
        self.auto_attackable = auto_attackable;
    }

    /// Return whether retail reverse movement is enabled for this unit.
    #[must_use]
    pub const fn is_reverse_moving(&self) -> bool {
        matches!(self.movement_facing, MovementFacing::Reverse)
    }

    pub(crate) fn set_reverse_move(&mut self, reverse_move: bool) {
        self.movement_facing = if reverse_move {
            MovementFacing::Reverse
        } else {
            MovementFacing::Forward
        };
    }

    /// Return whether the retail idle action currently exists.
    #[must_use]
    pub fn has_idle_action(&self) -> bool {
        self.idle.is_active()
    }

    /// Return the elapsed duration of the current idle action in milliseconds.
    #[must_use]
    pub fn idle_duration(&self) -> u32 {
        self.idle.duration_ms()
    }

    pub(crate) fn reconcile_idle_action(&mut self, elapsed_ms: u32, parent_is_idle: bool) {
        let should_be_idle = self.is_alive() && self.state == UnitState::Idle && parent_is_idle;
        self.idle.reconcile(should_be_idle, elapsed_ms);
    }

    pub(crate) fn cancel_idle_action(&mut self) {
        self.idle.cancel();
    }

    /// Check whether another unit currently contains this unit.
    #[must_use]
    pub const fn is_garrisoned(&self) -> bool {
        self.garrison.is_contained()
    }

    /// Check retail object-type identity without case sensitivity.
    ///
    /// Every proto-object is itself a base object type, followed by its
    /// authored abstract object-type memberships.
    #[must_use]
    pub fn is_object_type(&self, object_type: &str) -> bool {
        self.proto_object_name.eq_ignore_ascii_case(object_type)
            || self
                .object_types
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(object_type))
    }

    /// Return whether this unit is an authored socket child of another unit.
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        self.socket_parent_id.is_some()
    }

    /// Return the unit that owns this authored socket child.
    #[must_use]
    pub const fn socket_parent(&self) -> Option<EntityId> {
        self.socket_parent_id
    }

    /// Return associated socket units in retail entity-reference order.
    #[must_use]
    pub fn associated_sockets(&self) -> &[EntityId] {
        &self.associated_socket_ids
    }

    /// Return the live building ID recorded as this socket's plug.
    #[must_use]
    pub const fn socket_plug(&self) -> Option<EntityId> {
        self.socket_plug_id
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

    pub(crate) fn set_prototype_movement_speed(&mut self, speed: f32) {
        if !speed.is_finite() || speed < 0.0 {
            return;
        }
        self.speed = speed;
        if let Some(body) = self.physics.as_mut() {
            body.set_max_speed(speed);
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
        self.cancel_idle_action();
        self.stop();
    }

    /// Issue a standalone move order.
    ///
    /// Buildings and units currently controlled through a squad reject it.
    pub fn move_to(&mut self, target: Vec3) -> bool {
        if self.is_building()
            || !self.base.is_mobile()
            || self.squad_id.is_some()
            || !self.is_alive()
            || self.is_garrisoned()
        {
            return false;
        }
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.cancel_idle_action();
        self.move_target = Some(target);
        self.state = UnitState::Moving;
        true
    }

    /// Issue a standalone attack order.
    pub fn attack(&mut self, target: EntityId, range: f32, ability_id: Option<u8>) -> bool {
        if self.squad_id.is_some() || !self.is_operational() || target.is_invalid() {
            return false;
        }
        self.attack_target = Some(target);
        self.attack_range = valid_attack_range(range);
        self.attack_ability_id = ability_id;
        self.combat.reset();
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = UnitState::Attacking;
        true
    }

    /// Cancel the current standalone attack order.
    pub fn clear_attack_order(&mut self) {
        self.cancel_idle_action();
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
        if self.state == UnitState::Attacking && !self.is_building() && self.base.is_mobile() {
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
        let interrupted_movement = self.state == UnitState::Moving || self.move_target.is_some();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == UnitState::Moving {
            self.state = UnitState::Idle;
        }
        if interrupted_movement {
            self.cancel_idle_action();
        }
    }

    fn update_movement(&mut self, dt: f32) {
        let reverse_move = self.is_reverse_moving();
        if let Some(body) = &mut self.physics {
            if body.update(
                &mut self.base,
                self.move_target,
                dt,
                self.velocity_scalar,
                reverse_move,
            ) {
                self.stop();
            }
            return;
        }
        let Some(target) = self.move_target else {
            return;
        };
        let to_target = target - self.base.position;
        let distance = to_target.length();
        let speed = self.speed * self.velocity_scalar;
        if distance < ARRIVAL_THRESHOLD || speed * dt >= distance {
            self.base.position = target;
            self.stop();
            return;
        }
        let direction = to_target / distance;
        self.base.velocity = direction * speed;
        self.base.position += self.base.velocity * dt;
        self.base
            .set_forward(if reverse_move { -direction } else { direction });
    }

    pub(crate) fn move_as_squad_member(&mut self, target: Vec3) {
        if !self.is_building() && self.base.is_mobile() && self.is_alive() && !self.is_garrisoned()
        {
            self.cancel_idle_action();
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
        if self.base.is_mobile()
            && !self.is_garrisoned()
            && (self.state == UnitState::Moving
                || (self.state == UnitState::Attacking && self.move_target.is_some()))
        {
            self.update_movement(dt);
        }
    }

    fn is_alive(&self) -> bool {
        // Retail's BUnit::isAlive reads its explicit alive flag. Direct
        // trigger HP edits can therefore leave an otherwise-live unit at
        // zero hit points; normal combat damage still calls `kill` at zero.
        self.base.is_alive() && self.state != UnitState::Dead
    }
}

fn valid_attack_range(range: f32) -> f32 {
    if range.is_finite() && range > 0.0 {
        range
    } else {
        0.0
    }
}
