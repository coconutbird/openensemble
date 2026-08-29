//! Squad entities - the primary controllable groups.
//!
//! Based on `BSquad` from the original source.
//! A squad is a group of units that move and act together.

mod cryo;
mod detonate;
mod garrison;
mod join;
pub mod marine;
mod mines;
mod mode;
mod orders;
mod rage;
mod recovery;
mod repair;
mod shields;
mod transport;
pub mod warthog;

pub use cryo::SquadCryoState;
pub(crate) use cryo::{SquadCryo, SquadCryoConfig, SquadCryoEffect};
pub(crate) use detonate::DetonateOrder;
pub use detonate::SquadDetonatePhase;
pub use garrison::{SquadContainmentState, SquadGarrison};
pub use join::{JoinKind, JoinMergeType, SquadBoardState, SquadMergeState};
pub(crate) use mines::MineOrder;
pub use mode::SquadMode;
pub use recovery::{RecoveryType, SquadRecovery};
pub use shields::SquadShields;
pub(crate) use transport::{SquadPowerTransportPlan, SquadTransportPlan};
pub use transport::{
    PowerTransportPhase, SquadPowerTransport, SquadTransportFlyIn, TransportFlyInPhase,
};

use super::{BaseEntity, EntityIdle};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, PopulationCost};
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
    /// Executing a non-movement work action.
    Working,
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
    /// Retail `EntityIdle` action presence and elapsed duration.
    pub(crate) idle: EntityIdle,
    /// Gameplay implementation selected from the proto-squad name.
    pub archetype: SquadArchetype,
    /// Formation behavior selected from the proto-squad metadata.
    pub formation: SquadFormation,
    /// Movement target position (if moving).
    pub move_target: Option<Vec3>,
    /// Active and alternate-queued retail movement commands.
    pub(crate) orders: orders::SquadOrders,
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
    /// Persistent retail freezing, frozen, and thawing action state.
    pub(crate) cryo: SquadCryo,
    /// Shared regen reference count owned by active repair actions.
    pub(crate) repair: repair::SquadRepair,
    /// Persistent Rage controller references locking ordinary squad work.
    pub(crate) rage: rage::SquadRage,
    /// Game time of the most recent accepted member-damage event.
    pub last_damaged_time: u32,
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
    /// Live player-prototype squad ammunition maximum.
    ammunition_maximum: f32,
    /// Current retail veterancy level earned by this squad.
    veterancy_level: i32,
    /// Total retail veterancy experience committed to this squad.
    experience: f32,
    /// Damage bounty waiting for the current attack action to resolve.
    experience_bank: f32,
    /// Nominal pathing turn radius from the proto squad.
    pub turn_radius: f32,
    /// Minimum pathing turn radius from the proto squad.
    pub min_turn_radius: f32,
    /// Maximum pathing turn radius from the proto squad.
    pub max_turn_radius: f32,
    /// Distance at which an attack-move may automatically acquire a target.
    pub aggro_distance: f32,
    /// Maximum pursuit distance for an automatically acquired target.
    pub leash_distance: f32,
    /// Whether movement keeps the squad facing opposite its travel direction.
    reverse_move: bool,
    /// Units in this squad, sorted by entity ID for deterministic iteration.
    pub unit_ids: Vec<EntityId>,
    /// Live population charged to this logical squad.
    pub population_costs: Vec<PopulationCost>,
    /// Building whose production queue created this squad.
    pub trained_by: Option<EntityId>,
    /// Shared authored train-limit bucket, when one linked this squad.
    pub train_limit_bucket: Option<u8>,
    /// Destination squad used by the retail hot-drop/teleporter action.
    pub teleporter_destination: Option<EntityId>,
    /// Persistent retail Join action and its owned `BubbleShield` state.
    pub(crate) join: join::SquadJoin,
    /// Target-side provenance for an immediate Merge transformation.
    merge_state: Option<SquadMergeState>,
    /// Squad whose first child receives damage intended for this squad.
    damage_proxy: Option<EntityId>,
    /// Ordered retail entity references linking this source to wall endpoints.
    associated_wall_tower_ids: Vec<EntityId>,
    /// Towing squad this squad is currently hitched to.
    pub(crate) towing_partner: Option<EntityId>,
    /// Trailer squad currently hitched behind this towing squad.
    pub(crate) trailer_partner: Option<EntityId>,
    /// Logical containment order and passenger state.
    pub garrison: SquadGarrison,
    /// Trigger-created transport action owned by a synthetic transport squad.
    pub(crate) transport_fly_in: Option<SquadTransportFlyIn>,
    /// Native-power pickup/drop-off action owned by a synthetic carrier squad.
    pub(crate) power_transport: Option<SquadPowerTransport>,
    /// Members that completed the current command-ability attack cycle.
    ability_used_unit_ids: Vec<EntityId>,
    /// Active Mines order and deterministic per-member progress.
    pub(crate) mines: mines::SquadMines,
    /// Targeted suicide action and its authored phase transitions.
    pub(crate) detonate: detonate::SquadDetonate,
}

impl Default for Squad {
    fn default() -> Self {
        Self {
            base: BaseEntity::default(),
            state: SquadState::Idle,
            idle: EntityIdle::default(),
            archetype: SquadArchetype::Generic,
            formation: SquadFormation::Generic,
            move_target: None,
            orders: orders::SquadOrders::default(),
            attack_target: None,
            attack_range: 0.0,
            mode: SquadMode::Normal,
            attack_ability_id: None,
            recovery: SquadRecovery::default(),
            shields: SquadShields::default(),
            cryo: SquadCryo::default(),
            repair: repair::SquadRepair::default(),
            rage: rage::SquadRage::default(),
            last_damaged_time: 0,
            speed: 10.0, // Default speed
            acceleration: 0.0,
            turn_rate_degrees: 0.0,
            proto_squad_id: -1,
            proto_squad_name: String::new(),
            ammunition_maximum: 0.0,
            veterancy_level: 0,
            experience: 0.0,
            experience_bank: 0.0,
            turn_radius: 0.0,
            min_turn_radius: 0.0,
            max_turn_radius: 0.0,
            aggro_distance: 0.0,
            leash_distance: 0.0,
            reverse_move: false,
            unit_ids: Vec::new(),
            population_costs: Vec::new(),
            trained_by: None,
            train_limit_bucket: None,
            teleporter_destination: None,
            join: join::SquadJoin::default(),
            merge_state: None,
            damage_proxy: None,
            associated_wall_tower_ids: Vec::new(),
            towing_partner: None,
            trailer_partner: None,
            garrison: SquadGarrison::default(),
            transport_fly_in: None,
            power_transport: None,
            ability_used_unit_ids: Vec::new(),
            mines: mines::SquadMines::default(),
            detonate: detonate::SquadDetonate::default(),
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

    /// Return this squad's earned veterancy level.
    #[must_use]
    pub const fn veterancy_level(&self) -> i32 {
        self.veterancy_level
    }

    /// Return the squad's committed retail veterancy experience.
    #[must_use]
    pub const fn experience(&self) -> f32 {
        self.experience
    }

    /// Return damage bounty waiting on completion of the active attack.
    #[must_use]
    pub const fn banked_experience(&self) -> f32 {
        self.experience_bank
    }

    pub(crate) fn set_veterancy_level(&mut self, level: i32) {
        self.veterancy_level = level.max(0);
    }

    /// Number of overlapping repair actions currently affecting this squad.
    #[must_use]
    pub const fn repair_regen_source_count(&self) -> u32 {
        self.repair.source_count()
    }

    /// Whether a native Rage execution currently owns this squad's controls.
    #[must_use]
    pub const fn is_raging(&self) -> bool {
        self.rage.is_active()
    }

    /// Number of persistent Rage sources currently owning this squad.
    #[must_use]
    pub const fn rage_source_count(&self) -> u32 {
        self.rage.source_count()
    }

    /// Retail exposes Rage locomotion as sprinting presentation state.
    #[must_use]
    pub const fn is_sprinting(&self) -> bool {
        self.is_raging()
    }

    /// Return the player-prototype maximum used for squad ammunition ratios.
    #[must_use]
    pub const fn ammunition_maximum(&self) -> f32 {
        self.ammunition_maximum
    }

    pub(crate) fn set_ammunition_maximum(&mut self, maximum: f32) {
        self.ammunition_maximum = if maximum.is_finite() { maximum } else { 0.0 };
    }

    pub(crate) fn bank_experience(&mut self, experience: f32) -> bool {
        if !experience.is_finite() {
            return false;
        }
        let next = self.experience_bank + experience;
        if !next.is_finite() {
            return false;
        }
        self.experience_bank = next;
        true
    }

    pub(crate) fn apply_experience_bank(&mut self) -> f32 {
        self.experience = (self.experience + self.experience_bank).min(f32::MAX);
        self.experience_bank = 0.0;
        self.experience
    }

    pub(crate) fn clear_experience_bank(&mut self) {
        self.experience_bank = 0.0;
    }

    /// Return whether this squad currently owns a retail idle action.
    #[must_use]
    pub fn has_idle_action(&self) -> bool {
        self.idle.is_active()
    }

    /// Return the elapsed duration of the current idle action in milliseconds.
    #[must_use]
    pub fn idle_duration(&self) -> u32 {
        self.idle.duration_ms()
    }

    pub(crate) fn reconcile_idle_action(&mut self, elapsed_ms: u32) {
        let should_be_idle = self.is_alive() && self.state == SquadState::Idle;
        self.idle.reconcile(should_be_idle, elapsed_ms);
    }

    pub(crate) fn cancel_idle_action(&mut self) {
        self.idle.cancel();
    }

    /// Issue a move order to the given position.
    pub fn move_to(&mut self, target: Vec3) {
        if !self.base.is_mobile() || self.garrison.is_garrisoned() || self.is_raging() {
            return;
        }
        self.garrison.cancel_pending();
        self.move_to_internal(target);
    }

    pub(crate) fn move_to_garrison_target(&mut self, target: Vec3) {
        if !self.garrison.is_garrisoned() {
            self.move_to_internal(target);
        }
    }

    fn move_to_internal(&mut self, target: Vec3) {
        self.start_direct_move(target);
    }

    /// Remove every authoritative movement, combat, and containment order.
    pub(crate) fn remove_all_orders(&mut self) {
        self.garrison.cancel_pending();
        self.cancel_scripted_move_orders();
        self.move_target = None;
        self.attack_target = None;
        self.clear_experience_bank();
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.join.cancel();
        self.mines.cancel();
        self.detonate.cancel();
        self.base.velocity = Vec3::ZERO;
        if self.is_alive() {
            self.state = SquadState::Idle;
        }
        self.cancel_idle_action();
    }

    /// Issue an attack order against a generational entity ID.
    pub fn attack(
        &mut self,
        target: EntityId,
        range: f32,
        mode: Option<SquadMode>,
        ability_id: Option<u8>,
    ) -> bool {
        if !self.is_alive()
            || self.garrison.is_garrisoned()
            || self.is_cryo_frozen()
            || self.is_raging()
            || target.is_invalid()
        {
            return false;
        }
        self.garrison.cancel_pending();
        self.cancel_scripted_move_orders();
        self.join.cancel();
        self.mines.cancel();
        self.detonate.cancel();
        if self.attack_target != Some(target) {
            self.clear_experience_bank();
        }
        self.attack_target = Some(target);
        self.attack_range = valid_attack_range(range);
        if let Some(mode) = mode {
            self.mode = mode;
        }
        self.attack_ability_id = ability_id;
        self.ability_used_unit_ids.clear();
        self.cancel_idle_action();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        self.state = SquadState::Attacking;
        true
    }

    pub(crate) fn chase_attack_target(&mut self, target: Vec3) {
        if self.state == SquadState::Attacking && self.base.is_mobile() {
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
        let interrupted_movement = self.state == SquadState::Moving || self.move_target.is_some();
        self.cancel_scripted_move_orders();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.state == SquadState::Moving {
            self.state = SquadState::Idle;
        }
        if interrupted_movement {
            self.cancel_idle_action();
        }
    }

    /// Mark the squad dead and cancel all authoritative orders.
    pub fn kill(&mut self) {
        self.state = SquadState::Dead;
        self.base.kill();
        self.cancel_scripted_move_orders();
        self.move_target = None;
        self.attack_target = None;
        self.clear_experience_bank();
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.join.cancel();
        self.mines.cancel();
        self.detonate.cancel();
        self.base.velocity = Vec3::ZERO;
        self.cancel_idle_action();
    }

    /// Check if the squad is moving.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        !self.garrison.is_garrisoned() && self.state == SquadState::Moving
    }

    /// Return whether retail reverse movement is enabled for this squad.
    #[must_use]
    pub const fn is_reverse_moving(&self) -> bool {
        self.reverse_move
    }

    pub(crate) fn set_reverse_move(&mut self, reverse_move: bool) {
        self.reverse_move = reverse_move;
    }

    /// Link this source squad's hot-drop action to a destination squad.
    pub fn set_teleporter_destination(&mut self, destination: EntityId) {
        self.teleporter_destination = (!destination.is_invalid()).then_some(destination);
    }

    /// Clear a teleporter link that targets a removed squad.
    pub(crate) fn clear_teleporter_destination(&mut self, removed: EntityId) {
        if self.teleporter_destination == Some(removed) {
            self.teleporter_destination = None;
        }
    }

    /// Redirect member damage to the first child of another squad.
    pub fn set_damage_proxy(&mut self, proxy: EntityId) {
        self.damage_proxy = (!proxy.is_invalid()).then_some(proxy);
    }

    /// Remove this squad's active damage redirection.
    pub fn clear_damage_proxy(&mut self) {
        self.damage_proxy = None;
    }

    /// Return the squad currently receiving this squad's damage.
    #[must_use]
    pub const fn damage_proxy(&self) -> Option<EntityId> {
        self.damage_proxy
    }

    pub(crate) fn clear_damage_proxy_target(&mut self, removed: EntityId) {
        if self.damage_proxy == Some(removed) {
            self.damage_proxy = None;
        }
    }

    /// Wall endpoints linked by `SetTowerWallDestination`, in entity-ref order.
    #[must_use]
    pub fn associated_wall_towers(&self) -> &[EntityId] {
        &self.associated_wall_tower_ids
    }

    /// Active trigger transport action when this is the carrier squad.
    #[must_use]
    pub const fn transport_fly_in(&self) -> Option<&SquadTransportFlyIn> {
        self.transport_fly_in.as_ref()
    }

    /// Active native-power transport action when this is the carrier squad.
    #[must_use]
    pub const fn power_transport(&self) -> Option<&SquadPowerTransport> {
        self.power_transport.as_ref()
    }

    pub(crate) fn add_associated_wall_tower(&mut self, target: EntityId) {
        self.associated_wall_tower_ids.push(target);
    }

    pub(crate) fn remove_associated_wall_tower(&mut self, removed: EntityId) {
        self.associated_wall_tower_ids
            .retain(|&target| target != removed);
    }

    /// Return whether this squad is a trailer attached to another squad.
    #[must_use]
    pub fn is_hitched(&self) -> bool {
        self.towing_partner.is_some()
    }

    /// Return the towing squad this trailer is attached to.
    #[must_use]
    pub fn hitched_to_squad(&self) -> Option<EntityId> {
        self.towing_partner
    }

    /// Return the trailer attached behind this towing squad.
    #[must_use]
    pub fn hitched_squad(&self) -> Option<EntityId> {
        self.trailer_partner
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

        if !self.base.is_mobile() {
            self.base.velocity = Vec3::ZERO;
            return false;
        }
        let Some(target) = self.move_target else {
            return false;
        };

        let to_target = target - self.base.position;
        let distance = to_target.length();

        if distance < ARRIVAL_THRESHOLD {
            // Arrived at destination
            self.base.position = target;
            self.finish_current_movement();
            return true;
        }

        let direction = to_target / distance;
        let facing = if self.reverse_move {
            -direction
        } else {
            direction
        };
        self.base.forward = turn_toward(self.base.forward, facing, self.turn_rate_degrees, dt);
        let movement_direction = if self.reverse_move {
            -self.base.forward
        } else {
            self.base.forward
        };
        let current_speed = self.base.velocity.length();
        let desired_speed = desired_speed(
            self.speed * self.cryo_movement_modifier(),
            self.acceleration,
            distance,
        );
        let next_speed = approach_speed(current_speed, desired_speed, self.acceleration, dt);
        let move_distance = next_speed * dt;

        if move_distance >= distance && movement_direction.dot(direction) > 0.999 {
            // Would overshoot, just arrive
            self.base.position = target;
            self.finish_current_movement();
            return true;
        }

        // Update position and velocity
        self.base.velocity = movement_direction * next_speed;
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
        if self.is_cryo_frozen() {
            self.base.velocity = Vec3::ZERO;
            return;
        }
        if self.base.is_mobile()
            && !self.garrison.is_garrisoned()
            && (self.state == SquadState::Moving
                || (self.state == SquadState::Attacking && self.move_target.is_some()))
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
