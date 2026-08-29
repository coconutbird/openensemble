//! Individual unit and building entities.
//!
//! Reverse engineering confirms that both mobile units and buildings occupy
//! vanilla's class-1 `BUnit` pool. [`UnitKind`] records the behavioral
//! distinction without inventing a separate entity class.

mod actions;
mod ammunition;
mod building;
mod collision_attack;
mod combat;
mod cryo;
mod death_replacement;
mod detonate;
mod garrison;
pub mod marine;
mod physics_replacement;
pub(crate) mod rally_points;
mod revival;
mod scalars;
mod shields;
mod tactic_state;
mod tower_wall;
mod vehicle;
pub mod warthog;

pub use actions::UnitActions;
pub use ammunition::UnitAmmunition;
pub use building::{
    BuildingProduction, ConstructionKind, ConstructionProgress, ConstructionTask, ResearchProgress,
    ResearchTask, TrainingKind, TrainingProgress, TrainingTask,
};
pub(crate) use building::{ProductionTask, TriggerCommandStateRef};
pub(crate) use collision_attack::UnitCollisionAttack;
pub use combat::UnitCombat;
pub(crate) use cryo::UnitCryo;
pub(crate) use death_replacement::UnitStaticDeathReplacement;
pub use detonate::UnitDetonatePhase;
pub(crate) use detonate::{UnitDetonateTriggerConfig, UnitDetonation};
pub use garrison::UnitGarrison;
pub(crate) use physics_replacement::UnitPhysicsReplacement;
pub use rally_points::RallyPoint;
pub use scalars::UnitDataScalar;
pub(crate) use scalars::UnitScalarModifiers;
pub use shields::{ShieldCoverage, UnitShields};
pub use tower_wall::TowerWallAction;
pub(crate) use vehicle::configure_ground_vehicle_physics;

use super::{BaseEntity, BaseId, EntityIdle, ObjectState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::UnitRevivalProfile;
use crate::physics::PhysicsBody;
use crate::player::{PlayerId, PopulationCost};
use glam::Vec3;
use revival::{DamageDisposition, UnitRevival};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ExternalShieldState {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum InvulnerabilityState {
    #[default]
    Vulnerable,
    Invulnerable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BoardingState {
    #[default]
    Free,
    BeingBoarded,
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
    /// Effective player-prototype name used by gameplay and presentation.
    pub proto_object_name: String,
    /// Logical database prototype whose player-owned definition was transformed.
    pub(crate) logical_proto_object_name: String,
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
    /// Independent retail hero-down or tactic hibernation state.
    revival: UnitRevival,
    /// Live outgoing damage multiplier (veterancy and tech effects layer here).
    pub damage_multiplier: f32,
    /// Live incoming damage multiplier.
    pub damage_taken_multiplier: f32,
    /// Outgoing modifier contributed by an active squad Join relationship.
    join_damage_multiplier: f32,
    /// Incoming modifier contributed by an active squad Join relationship.
    join_damage_taken_multiplier: f32,
    /// Live ranged-attack accuracy multiplier.
    pub accuracy_scalar: f32,
    /// Live ranged-attack dodge modifier applied alongside accuracy.
    pub dodge_scalar: f32,
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
    /// Whether retail action state currently rejects all incoming damage.
    invulnerability: InvulnerabilityState,
    /// Whether a hostile Board action temporarily reserves this target.
    boarding_state: BoardingState,
    /// Whether projectiles and AOE use retail's external-shield volume rules.
    external_shield: ExternalShieldState,
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
    /// Current authored tactic state and its deterministic transition revision.
    tactic_state: tactic_state::UnitTacticState,
    /// Containment state and immutable container capabilities.
    pub garrison: UnitGarrison,
    /// Per-unit authored attack animation/cooldown state.
    pub combat: UnitCombat,
    /// Persistent retail ammunition amount and regeneration action.
    pub ammunition: UnitAmmunition,
    /// Per-unit effects projected from the owning squad's cryo action.
    pub(crate) cryo: UnitCryo,
    /// Persistent collision-attack lifecycle and targets hit by this action.
    pub(crate) collision_attack: UnitCollisionAttack,
    /// Persistent arming and immediate Detonate child-action state.
    pub(crate) detonate: detonate::UnitDetonate,
    /// Whether this entity is a physics death replacement awaiting cleanup.
    pub(crate) physics_replacement: UnitPhysicsReplacement,
    /// In-place static replacement retained after the source unit dies.
    pub(crate) static_death_replacement: UnitStaticDeathReplacement,
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
            logical_proto_object_name: String::new(),
            object_types: Vec::new(),
            flying: false,
            hitpoints: 100.0,
            max_hitpoints: 100.0,
            shields: UnitShields::default(),
            revival: UnitRevival::default(),
            damage_multiplier: 1.0,
            damage_taken_multiplier: 1.0,
            join_damage_multiplier: 1.0,
            join_damage_taken_multiplier: 1.0,
            accuracy_scalar: 1.0,
            dodge_scalar: 1.0,
            work_rate_scalar: 1.0,
            line_of_sight_scalar: 1.0,
            velocity_scalar: 1.0,
            weapon_range_scalar: 1.0,
            auto_attackable: true,
            invulnerability: InvulnerabilityState::Vulnerable,
            boarding_state: BoardingState::Free,
            external_shield: ExternalShieldState::Disabled,
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
            tactic_state: tactic_state::UnitTacticState::default(),
            garrison: UnitGarrison::default(),
            combat: UnitCombat::default(),
            ammunition: UnitAmmunition::default(),
            cryo: UnitCryo::default(),
            collision_attack: UnitCollisionAttack::default(),
            detonate: detonate::UnitDetonate::default(),
            physics_replacement: UnitPhysicsReplacement::default(),
            static_death_replacement: UnitStaticDeathReplacement::default(),
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

    /// Return the stable logical prototype name retained across technology transforms.
    #[must_use]
    pub fn logical_proto_object_name(&self) -> &str {
        &self.logical_proto_object_name
    }

    /// Check whether this unit is a building.
    #[must_use]
    pub fn is_building(&self) -> bool {
        self.kind == UnitKind::Building
    }

    /// Check whether this unit can perform completed-unit gameplay actions.
    #[must_use]
    pub fn is_operational(&self) -> bool {
        self.is_alive()
            && !self.is_incapacitated()
            && !self.is_garrisoned()
            && (!self.is_building() || self.built)
    }

    /// Return whether combat may currently target and damage this unit.
    #[must_use]
    pub fn is_attackable(&self) -> bool {
        self.is_alive()
            && !self.is_incapacitated()
            && !self.is_garrisoned()
            && !self.is_invulnerable()
            && !self.is_being_boarded()
    }

    /// Return whether automatic combat acquisition may target this object.
    #[must_use]
    pub fn is_auto_attackable(&self) -> bool {
        self.auto_attackable && self.is_attackable()
    }

    /// Return whether this prototype owns a retail external-shield volume.
    #[must_use]
    pub const fn is_external_shield(&self) -> bool {
        matches!(self.external_shield, ExternalShieldState::Enabled)
    }

    /// Return retail's synchronized simulation-bounding-box center.
    #[must_use]
    pub(crate) fn simulation_center(&self) -> Vec3 {
        if self.flying {
            self.base.position
        } else {
            self.base.position + Vec3::Y * self.obstruction_half_extents.y.abs()
        }
    }

    /// Return the horizontal obstruction radius used by retail range math.
    #[must_use]
    pub(crate) fn obstruction_radius(&self) -> f32 {
        self.obstruction_half_extents
            .x
            .abs()
            .max(self.obstruction_half_extents.z.abs())
    }

    /// Return the authoritative axis-aligned bounds used by sim-space queries.
    pub(crate) fn simulation_bounds(&self) -> (Vec3, Vec3) {
        (
            self.simulation_center(),
            self.obstruction_half_extents.abs(),
        )
    }

    pub(crate) const fn auto_attackable_setting(&self) -> bool {
        self.auto_attackable
    }

    pub(crate) fn set_auto_attackable(&mut self, auto_attackable: bool) {
        self.auto_attackable = auto_attackable;
    }

    /// Return whether a persistent retail action currently prevents damage.
    #[must_use]
    pub const fn is_invulnerable(&self) -> bool {
        matches!(self.invulnerability, InvulnerabilityState::Invulnerable)
    }

    pub(crate) fn set_invulnerable(&mut self, invulnerable: bool) {
        self.invulnerability = if invulnerable {
            InvulnerabilityState::Invulnerable
        } else {
            InvulnerabilityState::Vulnerable
        };
    }

    /// Return whether a timed Board action currently owns this target.
    #[must_use]
    pub const fn is_being_boarded(&self) -> bool {
        matches!(self.boarding_state, BoardingState::BeingBoarded)
    }

    pub(crate) fn set_being_boarded(&mut self, being_boarded: bool) {
        self.boarding_state = if being_boarded {
            BoardingState::BeingBoarded
        } else {
            BoardingState::Free
        };
    }

    pub(crate) fn set_external_shield(&mut self, external_shield: bool) {
        self.external_shield = if external_shield {
            ExternalShieldState::Enabled
        } else {
            ExternalShieldState::Disabled
        };
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
        let should_be_idle = self.is_alive()
            && !self.is_incapacitated()
            && self.state == UnitState::Idle
            && parent_is_idle;
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
        self.damage_oriented(amount, false, Vec3::ZERO)
    }

    pub(crate) fn damage_directional(&mut self, amount: f32, direction: Vec3) -> bool {
        self.damage_oriented(amount, true, direction)
    }

    fn damage_oriented(&mut self, amount: f32, directional: bool, direction: Vec3) -> bool {
        if !amount.is_finite()
            || amount <= 0.0
            || !self.is_alive()
            || self.is_incapacitated()
            || self.is_invulnerable()
            || self.is_being_boarded()
        {
            return false;
        }
        let direction_dot_forward = direction.dot(self.base.forward);
        let hitpoint_damage =
            self.shields
                .absorb_damage(amount, directional, direction_dot_forward);
        self.hitpoints = (self.hitpoints - hitpoint_damage).max(0.0);
        match self.revival.on_damage(self.hitpoints, self.max_hitpoints) {
            DamageDisposition::Mortal => self.kill(),
            DamageDisposition::Incapacitated => {
                if self.is_down() {
                    self.hitpoints = 1.0_f32.min(self.max_hitpoints);
                    self.shields.set_current(0.0);
                }
                self.cancel_for_incapacitation();
            }
            DamageDisposition::Active => {}
        }
        true
    }

    /// Return whether this live unit has retail's independent `Down` flag.
    #[must_use]
    pub fn is_down(&self) -> bool {
        self.revival.is_down()
    }

    /// Return whether this live unit has retail's `IsHibernating` flag.
    #[must_use]
    pub fn is_hibernating(&self) -> bool {
        self.revival.is_hibernating()
    }

    /// Return whether down/hibernating state prevents normal gameplay actions.
    #[must_use]
    pub fn is_incapacitated(&self) -> bool {
        self.is_down() || self.is_hibernating()
    }

    /// Return the immutable revival definition configured for this unit.
    #[must_use]
    pub fn revival_profile(&self) -> Option<UnitRevivalProfile> {
        self.revival.profile()
    }

    pub(crate) fn configure_revival(&mut self, profile: UnitRevivalProfile) {
        self.revival.configure(profile);
    }

    pub(crate) fn has_hero_revival(&self) -> bool {
        self.revival.is_hero()
    }

    pub(crate) fn down_hero(&mut self) -> bool {
        if !self.revival.down_hero() {
            return false;
        }
        self.hitpoints = 1.0_f32.min(self.max_hitpoints);
        self.shields.set_current(0.0);
        self.cancel_for_incapacitation();
        true
    }

    pub(crate) fn override_revival_at_zero(&mut self) -> bool {
        self.revival.override_at_zero(self.hitpoints)
    }

    pub(crate) fn advance_revival(&mut self, dt: f32) -> bool {
        if self.revival.should_die_at_zero(self.hitpoints) {
            self.kill();
            return false;
        }
        let advance = self.revival.advance(dt, self.hitpoints, self.max_hitpoints);
        self.hitpoints = advance.hitpoints;
        advance.hero_ready
    }

    pub(crate) fn finish_hero_revival(&mut self) -> bool {
        if !self.revival.finish_hero_revival() {
            return false;
        }
        self.line_of_sight_scalar = 1.0;
        true
    }

    pub(crate) fn hash_revival_state(&self, checksum: &mut crate::sync::SyncChecksum) {
        self.revival.hash_state(checksum);
    }

    fn cancel_for_incapacitation(&mut self) {
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.cancel_detonate_action();
        self.clear_tactic_state();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = UnitState::Idle;
        self.cancel_idle_action();
    }

    /// Kill this unit or building.
    pub fn kill(&mut self) {
        self.hitpoints = 0.0;
        self.state = UnitState::Dead;
        self.base.kill();
        self.revival.clear_incapacitation();
        self.attack_target = None;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.combat.reset();
        self.clear_tactic_state();
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
            || self.is_incapacitated()
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
        let velocity_scalar = self.effective_velocity_scalar();
        let physics_replacement = self.is_physics_replacement();
        if let Some(body) = &mut self.physics {
            let arrived = body.update(
                &mut self.base,
                self.move_target,
                dt,
                velocity_scalar,
                reverse_move,
            );
            if arrived || (!physics_replacement && self.move_target.is_none() && body.is_grounded())
            {
                self.stop();
            }
            return;
        }
        let Some(target) = self.move_target else {
            return;
        };
        let to_target = target - self.base.position;
        let distance = to_target.length();
        let speed = self.speed * velocity_scalar;
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
        if !self.is_building() && self.base.is_mobile() && self.is_operational() {
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
        self.ammunition.advance(dt);
        let airborne = self
            .physics
            .as_ref()
            .is_some_and(|body| !body.is_grounded());
        if self.base.is_mobile()
            && !self.is_incapacitated()
            && !self.is_garrisoned()
            && (airborne
                || self.is_physics_replacement()
                || self.state == UnitState::Moving
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
