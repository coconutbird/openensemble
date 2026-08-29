//! Persistent squad Join-order state.

use super::{Squad, SquadArchetype, SquadFormation, SquadMode, SquadState};
use crate::entities::UnitScalarModifiers;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, PopulationCost};
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Retail behavior selected by an authored `JoinType` node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// Persist beside an allied target and redirect support actions to it.
    Follow,
    /// Move the joining unit into the target squad.
    Merge,
    /// Contain the joining unit in the target and optionally take ownership.
    Board,
    /// Follow an allied target and mirror its validated attack opportunity.
    FollowAttack,
}

/// Independent retail occupancy bit placed on the target squad by a Join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinMergeType {
    /// Ground/Spartan join channel (retail bit 1).
    Ground,
    /// Air/monitor join channel (retail bit 2).
    Air,
}

/// Authoritative provenance retained by a squad transformed through Merge.
#[derive(Debug, Clone)]
pub struct SquadMergeState {
    joining_unit_id: EntityId,
    merge_type: JoinMergeType,
    joining_proto_squad_id: i32,
    joining_proto_squad_name: String,
    target_proto_squad_id: i32,
    target_proto_squad_name: String,
    damage_modifier: f32,
    damage_taken_modifier: f32,
    joining_population_costs: Vec<PopulationCost>,
    target_population_costs: Vec<PopulationCost>,
    joining_settings: JoiningSquadSettings,
    buffed_unit_ids: Vec<EntityId>,
}

#[derive(Debug, Clone)]
struct JoiningSquadSettings {
    archetype: SquadArchetype,
    formation: SquadFormation,
    mode: SquadMode,
    turn_radius: f32,
    min_turn_radius: f32,
    max_turn_radius: f32,
    aggro_distance: f32,
    leash_distance: f32,
    trained_by: Option<EntityId>,
    train_limit_bucket: Option<u8>,
}

/// Authoritative timed boarding and completed containment state.
#[derive(Debug, Clone)]
pub struct SquadBoardState {
    target_unit_id: EntityId,
    former_owner: PlayerId,
    board_time: f32,
    elapsed_seconds: f32,
    complete: bool,
    revert_damage_fraction: f32,
    unjoin_max_distance: f32,
    veterancy_override: bool,
    source_veterancy_level: i32,
    levels: i32,
    veterancy_modifiers: UnitScalarModifiers,
    veterancy_applied: bool,
    damage_modifier: f32,
    damage_taken_modifier: f32,
    attachment_proto_object_id: Option<i32>,
    attachment_proto_object_name: Option<String>,
    attachment_entity_id: Option<EntityId>,
    buffed_unit_ids: Vec<EntityId>,
}

/// Authoritative state retained by a persistent retail Join action.
#[derive(Debug, Clone, Default)]
pub(crate) struct SquadJoin {
    target_squad_id: Option<EntityId>,
    ability_id: Option<u8>,
    connected: bool,
    kind: Option<JoinKind>,
    merge_type: Option<JoinMergeType>,
    follow_attack_refresh_seconds: f32,
    bubble_shield_squad_id: Option<EntityId>,
    last_bubble_damage_time_ms: u32,
    board: Option<SquadBoardState>,
}

impl SquadJoin {
    fn begin(&mut self, target_squad_id: EntityId, ability_id: Option<u8>) {
        self.target_squad_id = Some(target_squad_id);
        self.ability_id = ability_id;
        self.connected = false;
        self.kind = None;
        self.merge_type = None;
        self.follow_attack_refresh_seconds = 0.0;
        self.bubble_shield_squad_id = None;
        self.last_bubble_damage_time_ms = 0;
        self.board = None;
    }

    pub(super) fn cancel(&mut self) {
        self.target_squad_id = None;
        self.ability_id = None;
        self.connected = false;
        self.kind = None;
        self.merge_type = None;
        self.follow_attack_refresh_seconds = 0.0;
        self.board = None;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.target_squad_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.ability_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(u32::from(self.connected));
        checksum.hash_u32(self.kind.map_or(u32::MAX, join_kind_wire_value));
        checksum.hash_u32(self.merge_type.map_or(u32::MAX, join_merge_type_wire_value));
        checksum.hash_f32(self.follow_attack_refresh_seconds);
        checksum.hash_u32(
            self.bubble_shield_squad_id
                .map_or(u32::MAX, EntityId::as_u32),
        );
        checksum.hash_u32(self.last_bubble_damage_time_ms);
        if let Some(board) = &self.board {
            checksum.hash_u32(1);
            board.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
    }
}

impl SquadBoardState {
    pub(crate) fn new(
        target_unit_id: EntityId,
        former_owner: PlayerId,
        timing: (f32, f32, f32),
        veterancy: (bool, i32, i32),
        veterancy_modifiers: UnitScalarModifiers,
        modifiers: (f32, f32),
        attachment: Option<(i32, String)>,
    ) -> Self {
        let (attachment_proto_object_id, attachment_proto_object_name) = attachment
            .map_or((None, None), |(prototype_id, prototype_name)| {
                (Some(prototype_id), Some(prototype_name))
            });
        Self {
            target_unit_id,
            former_owner,
            board_time: timing.0,
            elapsed_seconds: 0.0,
            complete: false,
            revert_damage_fraction: timing.1,
            unjoin_max_distance: timing.2,
            veterancy_override: veterancy.0,
            source_veterancy_level: veterancy.1,
            levels: veterancy.2,
            veterancy_modifiers,
            veterancy_applied: false,
            damage_modifier: modifiers.0,
            damage_taken_modifier: modifiers.1,
            attachment_proto_object_id,
            attachment_proto_object_name,
            attachment_entity_id: None,
            buffed_unit_ids: Vec::new(),
        }
    }

    /// Concrete target unit used as the post-board container.
    #[must_use]
    pub const fn target_unit_id(&self) -> EntityId {
        self.target_unit_id
    }

    /// Owner of the target when boarding began.
    #[must_use]
    pub const fn former_owner(&self) -> PlayerId {
        self.former_owner
    }

    /// Return whether ownership transfer and containment completed.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Elapsed timed-board duration.
    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    /// Authored timed-board duration.
    #[must_use]
    pub const fn board_time(&self) -> f32 {
        self.board_time
    }

    pub(crate) fn advance(&mut self, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0.0 || self.complete {
            return false;
        }
        self.elapsed_seconds += dt;
        self.elapsed_seconds > self.board_time
    }

    pub(crate) fn mark_complete(&mut self) {
        self.complete = true;
        self.elapsed_seconds = self.elapsed_seconds.max(self.board_time);
    }

    pub(crate) const fn revert_damage_fraction(&self) -> f32 {
        self.revert_damage_fraction
    }

    pub(crate) const fn unjoin_max_distance(&self) -> f32 {
        self.unjoin_max_distance
    }

    /// Whether the contained Spartan contributes its veterancy contract.
    #[must_use]
    pub const fn veterancy_override(&self) -> bool {
        self.veterancy_override
    }

    /// Extra effective veterancy levels authored by this Board action.
    #[must_use]
    pub const fn levels(&self) -> i32 {
        self.levels
    }

    /// Earned Spartan levels contributed to the boarded target.
    #[must_use]
    pub const fn source_veterancy_level(&self) -> i32 {
        self.source_veterancy_level
    }

    /// Effective level bonus exposed by the boarded target.
    #[must_use]
    pub fn effective_veterancy_bonus(&self) -> i32 {
        if self.veterancy_override {
            self.source_veterancy_level.saturating_add(self.levels)
        } else {
            0
        }
    }

    pub(crate) const fn veterancy_modifiers(&self) -> UnitScalarModifiers {
        self.veterancy_modifiers
    }

    pub(crate) const fn veterancy_was_applied(&self) -> bool {
        self.veterancy_applied
    }

    pub(crate) fn mark_veterancy_applied(&mut self) {
        self.veterancy_applied = true;
    }

    pub(crate) const fn damage_modifier(&self) -> f32 {
        self.damage_modifier
    }

    pub(crate) const fn damage_taken_modifier(&self) -> f32 {
        self.damage_taken_modifier
    }

    /// Scenario-layered visual effect prototype requested after takeover.
    #[must_use]
    pub fn attachment_proto_object_name(&self) -> Option<&str> {
        self.attachment_proto_object_name.as_deref()
    }

    /// Live authoritative attachment created on the boarded unit.
    #[must_use]
    pub const fn attachment_entity_id(&self) -> Option<EntityId> {
        self.attachment_entity_id
    }

    pub(crate) fn attachment_spec(&self) -> Option<(i32, &str)> {
        Some((
            self.attachment_proto_object_id?,
            self.attachment_proto_object_name.as_deref()?,
        ))
    }

    pub(crate) fn set_attachment_entity_id(&mut self, entity_id: EntityId) {
        self.attachment_entity_id = Some(entity_id);
    }

    pub(crate) fn buffed_unit_ids(&self) -> &[EntityId] {
        &self.buffed_unit_ids
    }

    pub(crate) fn mark_buffed(&mut self, unit_id: EntityId) -> bool {
        match self.buffed_unit_ids.binary_search(&unit_id) {
            Ok(_) => false,
            Err(index) => {
                self.buffed_unit_ids.insert(index, unit_id);
                true
            }
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.target_unit_id.as_u32());
        checksum.hash_u32(u32::from(self.former_owner));
        checksum.hash_f32(self.board_time);
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_u32(u32::from(self.complete));
        checksum.hash_f32(self.revert_damage_fraction);
        checksum.hash_f32(self.unjoin_max_distance);
        checksum.hash_u32(u32::from(self.veterancy_override));
        checksum.hash_i32(self.source_veterancy_level);
        checksum.hash_i32(self.levels);
        for modifier in self.veterancy_modifiers.components() {
            checksum.hash_f32(modifier);
        }
        checksum.hash_u32(u32::from(self.veterancy_applied));
        checksum.hash_f32(self.damage_modifier);
        checksum.hash_f32(self.damage_taken_modifier);
        checksum.hash_i32(self.attachment_proto_object_id.unwrap_or(-1));
        if let Some(name) = &self.attachment_proto_object_name {
            checksum.hash_u32(1);
            hash_string(checksum, name);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(self.attachment_entity_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(u32::try_from(self.buffed_unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.buffed_unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

impl SquadMergeState {
    pub(crate) fn new(
        joining_unit_id: EntityId,
        merge_type: JoinMergeType,
        joining_squad: &Squad,
        target_squad: &Squad,
        modifiers: (f32, f32),
    ) -> Self {
        Self {
            joining_unit_id,
            merge_type,
            joining_proto_squad_id: joining_squad.proto_squad_id,
            joining_proto_squad_name: joining_squad.proto_squad_name.clone(),
            target_proto_squad_id: target_squad.proto_squad_id,
            target_proto_squad_name: target_squad.proto_squad_name.clone(),
            damage_modifier: modifiers.0,
            damage_taken_modifier: modifiers.1,
            joining_population_costs: joining_squad.population_costs.clone(),
            target_population_costs: target_squad.population_costs.clone(),
            joining_settings: JoiningSquadSettings::from_squad(joining_squad),
            buffed_unit_ids: Vec::new(),
        }
    }

    /// Unit contributed by the squad that initiated Merge.
    #[must_use]
    pub const fn joining_unit_id(&self) -> EntityId {
        self.joining_unit_id
    }

    /// Occupancy channel retained on the transformed target squad.
    #[must_use]
    pub const fn merge_type(&self) -> JoinMergeType {
        self.merge_type
    }

    /// Original proto squad of the joining unit.
    #[must_use]
    pub fn joining_proto_squad_name(&self) -> &str {
        &self.joining_proto_squad_name
    }

    /// Original proto squad that received the joining unit.
    #[must_use]
    pub fn target_proto_squad_name(&self) -> &str {
        &self.target_proto_squad_name
    }

    pub(crate) const fn joining_proto_squad_id(&self) -> i32 {
        self.joining_proto_squad_id
    }

    pub(crate) const fn target_proto_squad_id(&self) -> i32 {
        self.target_proto_squad_id
    }

    pub(crate) const fn damage_modifier(&self) -> f32 {
        self.damage_modifier
    }

    pub(crate) const fn damage_taken_modifier(&self) -> f32 {
        self.damage_taken_modifier
    }

    pub(crate) fn joining_population_costs(&self) -> &[PopulationCost] {
        &self.joining_population_costs
    }

    pub(crate) fn target_population_costs(&self) -> &[PopulationCost] {
        &self.target_population_costs
    }

    pub(crate) fn buffed_unit_ids(&self) -> &[EntityId] {
        &self.buffed_unit_ids
    }

    pub(crate) fn mark_buffed(&mut self, unit_id: EntityId) -> bool {
        match self.buffed_unit_ids.binary_search(&unit_id) {
            Ok(_) => false,
            Err(index) => {
                self.buffed_unit_ids.insert(index, unit_id);
                true
            }
        }
    }

    pub(crate) fn restore_joining_settings(&self, squad: &mut Squad) {
        self.joining_settings.apply(squad);
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.joining_unit_id.as_u32());
        checksum.hash_u32(join_merge_type_wire_value(self.merge_type));
        checksum.hash_i32(self.joining_proto_squad_id);
        hash_string(checksum, &self.joining_proto_squad_name);
        checksum.hash_i32(self.target_proto_squad_id);
        hash_string(checksum, &self.target_proto_squad_name);
        checksum.hash_f32(self.damage_modifier);
        checksum.hash_f32(self.damage_taken_modifier);
        hash_population_costs(checksum, &self.joining_population_costs);
        hash_population_costs(checksum, &self.target_population_costs);
        self.joining_settings.hash_state(checksum);
        checksum.hash_u32(u32::try_from(self.buffed_unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.buffed_unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

impl JoiningSquadSettings {
    fn from_squad(squad: &Squad) -> Self {
        Self {
            archetype: squad.archetype,
            formation: squad.formation,
            mode: squad.mode,
            turn_radius: squad.turn_radius,
            min_turn_radius: squad.min_turn_radius,
            max_turn_radius: squad.max_turn_radius,
            aggro_distance: squad.aggro_distance,
            leash_distance: squad.leash_distance,
            trained_by: squad.trained_by,
            train_limit_bucket: squad.train_limit_bucket,
        }
    }

    fn apply(&self, squad: &mut Squad) {
        squad.archetype = self.archetype;
        squad.formation = self.formation;
        squad.mode = self.mode;
        squad.turn_radius = self.turn_radius;
        squad.min_turn_radius = self.min_turn_radius;
        squad.max_turn_radius = self.max_turn_radius;
        squad.aggro_distance = self.aggro_distance;
        squad.leash_distance = self.leash_distance;
        squad.trained_by = self.trained_by;
        squad.train_limit_bucket = self.train_limit_bucket;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.archetype as u32);
        checksum.hash_u32(self.formation as u32);
        checksum.hash_u32(self.mode as u32);
        checksum.hash_f32(self.turn_radius);
        checksum.hash_f32(self.min_turn_radius);
        checksum.hash_f32(self.max_turn_radius);
        checksum.hash_f32(self.aggro_distance);
        checksum.hash_f32(self.leash_distance);
        checksum.hash_u32(self.trained_by.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.train_limit_bucket.map_or(u32::MAX, u32::from));
    }
}

impl Squad {
    /// Return the squad targeted by this squad's persistent Join action.
    #[must_use]
    pub const fn join_target(&self) -> Option<EntityId> {
        self.join.target_squad_id
    }

    /// Return the `BubbleShield` squad owned by this joined squad, when live.
    #[must_use]
    pub const fn bubble_shield_squad(&self) -> Option<EntityId> {
        self.join.bubble_shield_squad_id
    }

    pub(crate) const fn join_ability_id(&self) -> Option<u8> {
        self.join.ability_id
    }

    pub(crate) const fn join_is_connected(&self) -> bool {
        self.join.connected
    }

    /// Return the behavior selected when this Join connected.
    #[must_use]
    pub const fn join_kind(&self) -> Option<JoinKind> {
        self.join.kind
    }

    /// Return the target occupancy channel selected by the connected Join.
    #[must_use]
    pub const fn join_merge_type(&self) -> Option<JoinMergeType> {
        self.join.merge_type
    }

    /// Return timed or completed Board state owned by this squad.
    #[must_use]
    pub const fn board_state(&self) -> Option<&SquadBoardState> {
        self.join.board.as_ref()
    }

    pub(crate) fn join_occupies_channel(&self) -> bool {
        self.join.kind.is_some()
    }

    /// Return Merge provenance when this squad is a transformed target.
    #[must_use]
    pub const fn merge_state(&self) -> Option<&SquadMergeState> {
        self.merge_state.as_ref()
    }

    pub(crate) const fn last_bubble_damage_time(&self) -> u32 {
        self.join.last_bubble_damage_time_ms
    }

    pub(crate) fn begin_join(&mut self, target_squad_id: EntityId, ability_id: Option<u8>) {
        self.remove_all_orders();
        self.join.begin(target_squad_id, ability_id);
    }

    pub(crate) fn cancel_join(&mut self) {
        self.join.cancel();
    }

    pub(crate) fn mark_join_connected(&mut self, kind: JoinKind, merge_type: JoinMergeType) {
        self.join.connected = true;
        self.join.kind = Some(kind);
        self.join.merge_type = Some(merge_type);
    }

    pub(crate) fn mark_boarding(&mut self, merge_type: JoinMergeType, state: SquadBoardState) {
        self.join.connected = false;
        self.join.kind = Some(JoinKind::Board);
        self.join.merge_type = Some(merge_type);
        self.join.board = Some(state);
    }

    pub(crate) fn mark_board_complete(&mut self) {
        self.join.connected = true;
        if let Some(board) = self.join.board.as_mut() {
            board.mark_complete();
        }
    }

    pub(crate) fn board_state_mut(&mut self) -> Option<&mut SquadBoardState> {
        self.join.board.as_mut()
    }

    pub(crate) fn take_board_state(&mut self) -> Option<SquadBoardState> {
        self.join.board.take()
    }

    pub(crate) fn set_merge_state(&mut self, state: SquadMergeState) {
        self.merge_state = Some(state);
    }

    pub(crate) fn merge_state_mut(&mut self) -> Option<&mut SquadMergeState> {
        self.merge_state.as_mut()
    }

    pub(crate) fn take_merge_state(&mut self) -> Option<SquadMergeState> {
        self.merge_state.take()
    }

    pub(crate) fn advance_follow_attack_refresh(&mut self, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        self.join.follow_attack_refresh_seconds += dt;
        if self.join.follow_attack_refresh_seconds <= 0.5 {
            return false;
        }
        self.join.follow_attack_refresh_seconds = 0.0;
        true
    }

    pub(crate) fn set_join_attack_target(&mut self, target: Option<EntityId>) {
        self.attack_target = target;
        self.attack_range = 0.0;
        self.attack_ability_id = None;
        self.ability_used_unit_ids.clear();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if target.is_some() {
            self.state = SquadState::Attacking;
            self.cancel_idle_action();
        } else if self.state == SquadState::Attacking {
            self.state = SquadState::Idle;
            self.cancel_idle_action();
        }
    }

    pub(crate) fn set_bubble_shield_squad(&mut self, shield_squad_id: Option<EntityId>) {
        self.join.bubble_shield_squad_id = shield_squad_id;
    }

    pub(crate) fn note_bubble_damage_time(&mut self, damage_time_ms: u32) {
        self.join.last_bubble_damage_time_ms =
            self.join.last_bubble_damage_time_ms.max(damage_time_ms);
    }

    pub(crate) fn clear_join_reference(&mut self, removed: EntityId) {
        if self.join.target_squad_id == Some(removed) {
            self.join.cancel();
        }
        if self.join.bubble_shield_squad_id == Some(removed) {
            self.join.bubble_shield_squad_id = None;
        }
    }

    pub(crate) fn follow_join_target(&mut self, target: Vec3, work_range: f32) {
        if !self.is_alive() || !self.base.is_mobile() || self.garrison.is_garrisoned() {
            return;
        }
        let delta = Vec3::new(
            target.x - self.base.position.x,
            0.0,
            target.z - self.base.position.z,
        );
        if delta.length_squared() <= work_range.max(0.0).powi(2) {
            self.move_target = None;
            self.base.velocity = Vec3::ZERO;
            if self.state == SquadState::Moving {
                self.state = SquadState::Idle;
            }
        } else {
            self.move_target = Some(target);
            self.state = SquadState::Moving;
            self.cancel_idle_action();
        }
    }

    pub(crate) fn hash_join_state(&self, checksum: &mut SyncChecksum) {
        self.join.hash_state(checksum);
        if let Some(state) = &self.merge_state {
            checksum.hash_u32(1);
            state.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

fn hash_population_costs(checksum: &mut SyncChecksum, costs: &[PopulationCost]) {
    checksum.hash_u32(u32::try_from(costs.len()).unwrap_or(u32::MAX));
    for cost in costs {
        checksum.hash_u32(u32::try_from(cost.population_type).unwrap_or(u32::MAX));
        checksum.hash_f32(cost.amount);
    }
}

const fn join_kind_wire_value(kind: JoinKind) -> u32 {
    match kind {
        JoinKind::Follow => 0,
        JoinKind::Merge => 1,
        JoinKind::Board => 2,
        JoinKind::FollowAttack => 3,
    }
}

const fn join_merge_type_wire_value(merge_type: JoinMergeType) -> u32 {
    match merge_type {
        JoinMergeType::Ground => 1,
        JoinMergeType::Air => 2,
    }
}
