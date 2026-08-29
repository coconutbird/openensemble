//! Authoritative squad containment and retail teleporter traversal.

use super::World;
use crate::entities::squads::formation_offset_to_world;
use crate::entities::{SquadContainmentState, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use glam::Vec3;
use std::collections::BTreeSet;
use std::f32::consts::TAU;
use thiserror::Error;

const DEFAULT_GARRISON_RANGE: f32 = 1.0;
const PLACEMENT_SPACING: f32 = 1.1;
const MIN_OBSTRUCTION_RADIUS: f32 = 0.2;

/// Failure to accept a player-authored garrison lifecycle command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GarrisonError {
    /// The recipient squad no longer exists.
    #[error("recipient squad does not exist")]
    SquadNotFound,
    /// The command player does not own the recipient squad.
    #[error("recipient squad is not owned by the command player")]
    NotOwned,
    /// The recipient squad is dead or has no surviving member.
    #[error("recipient squad cannot execute a garrison order")]
    SquadUnavailable,
    /// The target does not resolve to a live container unit.
    #[error("garrison target is not a live container")]
    InvalidTarget,
    /// Authored containment rules reject the squad.
    #[error("garrison target cannot contain the recipient squad")]
    CannotContain,
    /// The squad is already physically contained.
    #[error("recipient squad is already garrisoned")]
    AlreadyGarrisoned,
    /// The squad is not currently inside a container.
    #[error("recipient squad is not garrisoned")]
    NotGarrisoned,
}

#[derive(Debug, Clone, Copy)]
struct ContainerSnapshot {
    unit_id: EntityId,
    parent_squad: Option<EntityId>,
    position: Vec3,
    forward: Vec3,
    radius: f32,
    teleporter: bool,
}

#[derive(Debug, Clone, Copy)]
struct DestinationSnapshot {
    unit_id: EntityId,
    position: Vec3,
    forward: Vec3,
    radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainmentDecision {
    Accept,
    Wait,
    Reject,
}

impl World {
    /// Start moving one player-owned squad into an authored container.
    ///
    /// `target` may identify either the concrete container unit or its parent
    /// squad. A non-positive range selects the target's authored hot-drop range.
    ///
    /// # Errors
    ///
    /// Returns [`GarrisonError`] when the recipient, ownership, target, or
    /// authored containment rules reject the order.
    pub fn issue_garrison_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target: EntityId,
        range: f32,
    ) -> Result<(), GarrisonError> {
        self.validate_garrison_recipient(player_id, squad_id)?;
        let container = self
            .resolve_container_target(target)
            .ok_or(GarrisonError::InvalidTarget)?;
        if container.parent_squad == Some(squad_id)
            || self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.contains_unit(container.unit_id))
        {
            return Err(GarrisonError::InvalidTarget);
        }
        if self.containment_decision(squad_id, container) == ContainmentDecision::Reject {
            return Err(GarrisonError::CannotContain);
        }

        let now_ms = self.game_time_ms;
        let range = valid_nonnegative(range).unwrap_or_default();
        let squad = self
            .squads
            .get_mut(squad_id)
            .ok_or(GarrisonError::SquadNotFound)?;
        squad.clear_attack_order();
        squad.stop();
        squad
            .garrison
            .begin_garrison(container.unit_id, range, now_ms);
        squad.move_to_garrison_target(container.position);
        Ok(())
    }

    /// Start unloading one player-owned passenger squad.
    ///
    /// Teleporter passengers use the configured destination endpoint. Other
    /// containers unload beside themselves. A finite rally point becomes the
    /// squad's next authoritative move order after release.
    ///
    /// # Errors
    ///
    /// Returns [`GarrisonError`] when the recipient is missing, is not owned by
    /// the command player, is not contained, or lost its container.
    pub fn issue_ungarrison_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        rally_point: Option<Vec3>,
    ) -> Result<(), GarrisonError> {
        let squad = self
            .squads
            .get(squad_id)
            .ok_or(GarrisonError::SquadNotFound)?;
        if squad.base.player_id != player_id {
            return Err(GarrisonError::NotOwned);
        }
        let SquadContainmentState::Garrisoned { container, .. } = squad.garrison.state() else {
            return Err(GarrisonError::NotGarrisoned);
        };
        let source = self
            .container_snapshot(container)
            .ok_or(GarrisonError::InvalidTarget)?;
        let destination = self.exit_destination(source);
        let exit_position = self.exit_position(squad_id, destination, source.teleporter);
        let rally_point = rally_point.filter(|position| position.is_finite());
        self.begin_ungarrison(squad_id, source, destination, exit_position, rally_point);
        Ok(())
    }

    /// Start retail trigger-priority unloading for squads inside a container
    /// squad.
    ///
    /// An empty `passenger_filter` unloads every contained squad. Trigger
    /// orders intentionally bypass a single command-player check: retail
    /// groups contained passengers by their current owners before dispatch.
    pub fn trigger_unload_squad(
        &mut self,
        container_squad_id: EntityId,
        passenger_filter: &[EntityId],
    ) -> usize {
        let Some(container) = self.squads.get(container_squad_id) else {
            return 0;
        };
        let passengers = container.garrison.contained_squad_ids().to_vec();
        let mut accepted = 0;
        for passenger_id in passengers {
            if !passenger_filter.is_empty() && !passenger_filter.contains(&passenger_id) {
                continue;
            }
            let Some(player_id) = self
                .squads
                .get(passenger_id)
                .map(|squad| squad.base.player_id)
            else {
                continue;
            };
            if self
                .issue_ungarrison_order(player_id, passenger_id, None)
                .is_ok()
            {
                accepted += 1;
            }
        }
        accepted
    }

    pub(crate) fn update_garrisons(&mut self, gameplay: Option<&GameplayCatalog>) {
        let states = self
            .squads
            .iter()
            .map(|(id, squad)| (id, squad.garrison.state()))
            .collect::<Vec<_>>();
        for (squad_id, state) in states {
            match state {
                SquadContainmentState::Garrisoning { started_at_ms, .. }
                    if started_at_ms != self.game_time_ms =>
                {
                    self.update_garrisoning_squad(squad_id, gameplay);
                }
                SquadContainmentState::Garrisoned { since_ms, .. }
                    if since_ms != self.game_time_ms =>
                {
                    self.update_teleporter_passenger(squad_id, gameplay);
                }
                SquadContainmentState::Ungarrisoning { started_at_ms, .. }
                    if started_at_ms != self.game_time_ms =>
                {
                    self.finish_ungarrison(squad_id);
                }
                _ => {}
            }
        }
        self.sync_contained_squads();
    }

    pub(super) fn prepare_remove_squad_garrison(&mut self, squad_id: EntityId) {
        let Some(squad) = self.squads.get(squad_id) else {
            return;
        };
        let container_id = squad.garrison.container_id();
        let position = squad.base.position;
        let forward = squad.base.forward;
        let unit_ids = squad.unit_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut passengers = squad
            .garrison
            .contained_squad_ids()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for unit_id in &unit_ids {
            if let Some(unit) = self.units.get(*unit_id) {
                passengers.extend(unit.garrison.contained_unit_ids().iter().filter_map(
                    |contained_id| self.units.get(*contained_id).and_then(|unit| unit.squad_id),
                ));
            }
        }

        if let Some(container_id) = container_id {
            let release = self
                .units
                .get(container_id)
                .map_or((position, forward), |unit| {
                    (unit.base.position, unit.base.forward)
                });
            self.emergency_release_squad(squad_id, release.0, release.1);
        }
        for passenger_id in passengers {
            if passenger_id != squad_id {
                self.emergency_release_squad(passenger_id, position, forward);
            }
        }
        self.cancel_garrison_orders_targeting(&unit_ids);
    }

    pub(super) fn prepare_remove_unit_garrison(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get(unit_id) else {
            return;
        };
        let position = unit.base.position;
        let forward = unit.base.forward;
        let inverse_container = unit.garrison.container_id();
        let contained_unit_ids = unit.garrison.contained_unit_ids().to_vec();
        let mut passengers = contained_unit_ids
            .iter()
            .filter_map(|contained_id| self.units.get(*contained_id).and_then(|unit| unit.squad_id))
            .collect::<BTreeSet<_>>();
        passengers.extend(self.squads.iter().filter_map(|(squad_id, squad)| {
            (squad.garrison.container_id() == Some(unit_id)).then_some(squad_id)
        }));

        if let Some(container_id) = inverse_container
            && let Some(container) = self.units.get_mut(container_id)
        {
            container.garrison.remove_contained_unit(unit_id);
        }
        for passenger_id in passengers {
            self.emergency_release_squad(passenger_id, position, forward);
        }
        for contained_id in contained_unit_ids {
            if let Some(contained) = self.units.get_mut(contained_id) {
                contained.garrison.set_container(None);
                contained.base.position = position;
                contained.base.forward = normalized_forward(forward);
            }
        }
        self.cancel_garrison_orders_targeting(&BTreeSet::from([unit_id]));
    }

    fn validate_garrison_recipient(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Result<(), GarrisonError> {
        let squad = self
            .squads
            .get(squad_id)
            .ok_or(GarrisonError::SquadNotFound)?;
        if squad.base.player_id != player_id {
            return Err(GarrisonError::NotOwned);
        }
        if squad.garrison.is_garrisoned() {
            return Err(GarrisonError::AlreadyGarrisoned);
        }
        if !squad.is_alive()
            || !squad
                .unit_ids
                .iter()
                .any(|unit_id| self.units.get(*unit_id).is_some_and(Unit::is_operational))
        {
            return Err(GarrisonError::SquadUnavailable);
        }
        Ok(())
    }

    fn update_garrisoning_squad(&mut self, squad_id: EntityId, gameplay: Option<&GameplayCatalog>) {
        let Some(SquadContainmentState::Garrisoning { target, range, .. }) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.garrison.state())
        else {
            return;
        };
        let Some(container) = self.container_snapshot(target) else {
            self.cancel_pending_garrison(squad_id);
            return;
        };
        let decision = self.containment_decision(squad_id, container);
        if decision == ContainmentDecision::Reject {
            self.cancel_pending_garrison(squad_id);
            return;
        }
        let work_range = self.effective_garrison_range(range, container, gameplay);
        if !self.squad_is_in_garrison_range(squad_id, container, work_range) {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.move_to_garrison_target(container.position);
            }
            return;
        }
        if decision == ContainmentDecision::Wait {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.stop();
            }
            return;
        }
        self.complete_garrison(squad_id, container);
    }

    fn complete_garrison(&mut self, squad_id: EntityId, container: ContainerSnapshot) {
        let Some(squad) = self.squads.get(squad_id) else {
            return;
        };
        let passenger_units = squad
            .unit_ids
            .iter()
            .copied()
            .filter(|unit_id| self.units.get(*unit_id).is_some_and(Unit::is_operational))
            .collect::<Vec<_>>();
        if passenger_units.is_empty() {
            self.cancel_pending_garrison(squad_id);
            return;
        }

        for unit_id in &passenger_units {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.garrison.set_container(Some(container.unit_id));
                unit.stop();
                unit.state = UnitState::Idle;
                unit.base.position = container.position;
                unit.base.forward = normalized_forward(container.forward);
            }
        }
        if let Some(container_unit) = self.units.get_mut(container.unit_id) {
            for unit_id in &passenger_units {
                container_unit.garrison.add_contained_unit(*unit_id);
            }
        }
        if let Some(container_squad_id) = container.parent_squad
            && let Some(container_squad) = self.squads.get_mut(container_squad_id)
        {
            container_squad.garrison.add_contained_squad(squad_id);
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.clear_attack_order();
            squad.stop();
            squad.state = SquadState::Idle;
            squad.base.position = container.position;
            squad.base.forward = normalized_forward(container.forward);
            squad
                .garrison
                .mark_garrisoned(container.unit_id, self.game_time_ms);
        }
    }

    pub(in crate::world) fn contain_join_squad(
        &mut self,
        passenger_squad_id: EntityId,
        target_squad_id: EntityId,
    ) -> Option<EntityId> {
        let target_unit_id = self
            .squads
            .get(target_squad_id)?
            .unit_ids
            .iter()
            .find(|unit_id| self.units.get(**unit_id).is_some_and(Unit::is_operational))
            .copied()?;
        let target = self.units.get(target_unit_id)?;
        let container = ContainerSnapshot {
            unit_id: target_unit_id,
            parent_squad: Some(target_squad_id),
            position: target.base.position,
            forward: normalized_forward(target.base.forward),
            radius: unit_obstruction_radius(target),
            teleporter: false,
        };
        self.complete_garrison(passenger_squad_id, container);
        self.squads
            .get(passenger_squad_id)
            .and_then(|squad| squad.garrison.container_id())
    }

    pub(in crate::world) fn release_join_passenger(
        &mut self,
        squad_id: EntityId,
        position: Vec3,
        forward: Vec3,
    ) {
        self.emergency_release_squad(squad_id, position, forward);
    }

    fn update_teleporter_passenger(
        &mut self,
        squad_id: EntityId,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(SquadContainmentState::Garrisoned { container, .. }) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.garrison.state())
        else {
            return;
        };
        let Some(source) = self.container_snapshot(container) else {
            let release = self.units.get(container).map_or_else(
                || {
                    self.squads
                        .get(squad_id)
                        .map_or((Vec3::ZERO, Vec3::Z), |squad| {
                            (squad.base.position, squad.base.forward)
                        })
                },
                |unit| (unit.base.position, unit.base.forward),
            );
            self.emergency_release_squad(squad_id, release.0, release.1);
            return;
        };
        if !source.teleporter || self.first_contained_squad(source) != Some(squad_id) {
            return;
        }
        let destination = self.exit_destination(source);
        let work_range = gameplay
            .and_then(|catalog| {
                self.units
                    .get(source.unit_id)
                    .and_then(|unit| catalog.teleporter_work_range(&unit.proto_object_name))
            })
            .unwrap_or(DEFAULT_GARRISON_RANGE);
        let exit_position = self.exit_position(squad_id, destination, true);
        let rally_position = Some(self.random_rally_position(exit_position, work_range));
        self.begin_ungarrison(squad_id, source, destination, exit_position, rally_position);
    }

    fn begin_ungarrison(
        &mut self,
        squad_id: EntityId,
        source: ContainerSnapshot,
        destination: DestinationSnapshot,
        exit_position: Vec3,
        rally_position: Option<Vec3>,
    ) {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        squad.clear_attack_order();
        squad.stop();
        squad.state = SquadState::Idle;
        squad.garrison.begin_ungarrison(
            source.unit_id,
            destination.unit_id,
            exit_position,
            rally_position,
            self.game_time_ms,
        );
    }

    fn finish_ungarrison(&mut self, squad_id: EntityId) {
        let Some(SquadContainmentState::Ungarrisoning {
            destination,
            exit_position,
            rally_position,
            ..
        }) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.garrison.state())
        else {
            return;
        };
        let forward = self
            .units
            .get(destination)
            .map_or(Vec3::Z, |unit| unit.base.forward);
        self.detach_passenger_refs(squad_id);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.garrison.finish_action();
            squad.base.position = exit_position;
            squad.base.forward = normalized_forward(forward);
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Idle;
        }
        self.place_squad_members(squad_id, exit_position, forward, true);
        if let Some(rally_position) = rally_position
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.move_to(rally_position);
        }
    }

    fn containment_decision(
        &self,
        squad_id: EntityId,
        container: ContainerSnapshot,
    ) -> ContainmentDecision {
        let Some(container_unit) = self.units.get(container.unit_id) else {
            return ContainmentDecision::Reject;
        };
        let rejected = || {
            if container.teleporter {
                ContainmentDecision::Wait
            } else {
                ContainmentDecision::Reject
            }
        };
        if !container_unit.garrison.can_contain()
            || !container_unit.is_operational()
            || self.container_has_enemy(container_unit, squad_id)
        {
            return rejected();
        }
        if container_unit.garrison.one_squad_containment()
            && self
                .first_contained_squad(container)
                .is_some_and(|contained| contained != squad_id)
        {
            return rejected();
        }
        let maximum = container_unit.garrison.maximum_population();
        if maximum <= 0.0 {
            return ContainmentDecision::Accept;
        }
        if !self.container_accepts_squad_type(container_unit, squad_id) {
            return rejected();
        }
        let required = self.squad_population_cost(squad_id);
        let occupied_population = self.contained_population(container_unit);
        if required + occupied_population > maximum + f32::EPSILON {
            rejected()
        } else {
            ContainmentDecision::Accept
        }
    }

    fn container_has_enemy(&self, container: &Unit, squad_id: EntityId) -> bool {
        let Some(player_id) = self.squads.get(squad_id).map(|squad| squad.base.player_id) else {
            return true;
        };
        container
            .garrison
            .contained_unit_ids()
            .iter()
            .any(|unit_id| {
                self.units
                    .get(*unit_id)
                    .is_some_and(|unit| self.players_are_enemies(player_id, unit.base.player_id))
            })
    }

    fn container_accepts_squad_type(&self, container: &Unit, squad_id: EntityId) -> bool {
        let accepted = container.garrison.accepted_object_types();
        if accepted.is_empty() {
            return true;
        }
        let Some(proto_unit) = self.squads.get(squad_id).and_then(|squad| {
            squad
                .unit_ids
                .iter()
                .find_map(|unit_id| self.units.get(*unit_id))
        }) else {
            return false;
        };
        accepted
            .iter()
            .any(|object_type| proto_unit.is_object_type(object_type))
    }

    fn contained_population(&self, container: &Unit) -> f32 {
        container
            .garrison
            .contained_unit_ids()
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id).and_then(|unit| unit.squad_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|squad_id| self.squad_population_cost(squad_id))
            .sum()
    }

    fn squad_population_cost(&self, squad_id: EntityId) -> f32 {
        self.squads
            .get(squad_id)
            .and_then(|squad| squad.population_costs.first())
            .map_or(0.0, |cost| cost.amount.max(0.0))
    }

    fn resolve_container_target(&self, target: EntityId) -> Option<ContainerSnapshot> {
        if let Some(snapshot) = self.container_snapshot(target) {
            return Some(snapshot);
        }
        let squad = self.squads.get(target)?;
        squad
            .unit_ids
            .iter()
            .find_map(|unit_id| self.container_snapshot(*unit_id))
    }

    fn container_snapshot(&self, unit_id: EntityId) -> Option<ContainerSnapshot> {
        let unit = self.units.get(unit_id)?;
        (unit.garrison.can_contain() && unit.is_operational()).then_some(ContainerSnapshot {
            unit_id,
            parent_squad: unit.squad_id,
            position: unit.base.position,
            forward: normalized_forward(unit.base.forward),
            radius: unit_obstruction_radius(unit),
            teleporter: unit.garrison.is_teleporter(),
        })
    }

    fn destination_snapshot(&self, target: EntityId) -> Option<DestinationSnapshot> {
        if let Some(unit) = self.units.get(target).filter(|unit| unit.is_alive()) {
            return Some(DestinationSnapshot {
                unit_id: target,
                position: unit.base.position,
                forward: normalized_forward(unit.base.forward),
                radius: unit_obstruction_radius(unit),
            });
        }
        let squad = self.squads.get(target)?;
        squad.unit_ids.iter().find_map(|unit_id| {
            let unit = self.units.get(*unit_id).filter(|unit| unit.is_alive())?;
            Some(DestinationSnapshot {
                unit_id: *unit_id,
                position: unit.base.position,
                forward: normalized_forward(unit.base.forward),
                radius: unit_obstruction_radius(unit),
            })
        })
    }

    fn exit_destination(&self, source: ContainerSnapshot) -> DestinationSnapshot {
        let linked = source.parent_squad.and_then(|squad_id| {
            self.squads
                .get(squad_id)
                .and_then(|squad| squad.teleporter_destination)
        });
        linked
            .and_then(|target| self.destination_snapshot(target))
            .unwrap_or(DestinationSnapshot {
                unit_id: source.unit_id,
                position: source.position,
                forward: source.forward,
                radius: source.radius,
            })
    }

    fn effective_garrison_range(
        &self,
        requested: f32,
        container: ContainerSnapshot,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        valid_positive(requested)
            .or_else(|| {
                gameplay.and_then(|catalog| {
                    self.units
                        .get(container.unit_id)
                        .and_then(|unit| catalog.teleporter_work_range(&unit.proto_object_name))
                })
            })
            .unwrap_or(DEFAULT_GARRISON_RANGE)
    }

    fn squad_is_in_garrison_range(
        &self,
        squad_id: EntityId,
        container: ContainerSnapshot,
        range: f32,
    ) -> bool {
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        let delta = squad.base.position - container.position;
        let center_distance = Vec3::new(delta.x, 0.0, delta.z).length();
        let surface_distance =
            center_distance - self.squad_obstruction_radius(squad_id) - container.radius;
        surface_distance <= range.max(0.0)
    }

    fn squad_obstruction_radius(&self, squad_id: EntityId) -> f32 {
        self.squads.get(squad_id).map_or(0.0, |squad| {
            squad
                .unit_ids
                .iter()
                .filter_map(|unit_id| self.units.get(*unit_id))
                .map(|unit| {
                    let offset = Vec3::new(unit.formation_offset.x, 0.0, unit.formation_offset.z);
                    offset.length() + unit_obstruction_radius(unit)
                })
                .fold(0.0, f32::max)
        })
    }

    fn exit_position(
        &mut self,
        squad_id: EntityId,
        destination: DestinationSnapshot,
        consume_teleporter_random: bool,
    ) -> Vec3 {
        let squad_radius = self
            .squad_obstruction_radius(squad_id)
            .max(MIN_OBSTRUCTION_RADIUS);
        let distance = (destination.radius + squad_radius) * PLACEMENT_SPACING;
        if consume_teleporter_random {
            let angle = self.rng.f_rand(0.0, TAU);
            let (sin, cos) = angle.sin_cos();
            let _retail_unused_start = destination.position + Vec3::new(sin, 0.0, cos) * distance;
        }
        let right = Vec3::Y.cross(destination.forward).normalize_or_zero();
        destination.position + destination.forward * distance - right * distance
    }

    fn random_rally_position(&mut self, origin: Vec3, distance: f32) -> Vec3 {
        let angle = self.rng.f_rand(0.0, TAU);
        let (sin, cos) = angle.sin_cos();
        origin + Vec3::new(sin, 0.0, cos) * distance.max(0.0)
    }

    fn first_contained_squad(&self, container: ContainerSnapshot) -> Option<EntityId> {
        container
            .parent_squad
            .and_then(|squad_id| self.squads.get(squad_id))
            .and_then(|squad| squad.garrison.contained_squad_ids().first().copied())
            .or_else(|| {
                self.units
                    .get(container.unit_id)?
                    .garrison
                    .contained_unit_ids()
                    .iter()
                    .filter_map(|unit_id| self.units.get(*unit_id).and_then(|unit| unit.squad_id))
                    .min()
            })
    }

    fn cancel_pending_garrison(&mut self, squad_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.garrison.cancel_pending();
            squad.stop();
        }
    }

    fn cancel_garrison_orders_targeting(&mut self, targets: &BTreeSet<EntityId>) {
        for (_, squad) in self.squads.iter_mut() {
            if matches!(
                squad.garrison.state(),
                SquadContainmentState::Garrisoning { target, .. } if targets.contains(&target)
            ) {
                squad.garrison.cancel_pending();
                squad.stop();
            }
        }
    }

    pub(crate) fn detach_passenger_refs(&mut self, squad_id: EntityId) {
        let unit_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        let inverse_refs = unit_ids
            .iter()
            .filter_map(|unit_id| {
                self.units
                    .get(*unit_id)
                    .and_then(|unit| unit.garrison.container_id())
                    .map(|container| (*unit_id, container))
            })
            .collect::<Vec<_>>();
        for (unit_id, container_id) in inverse_refs {
            if let Some(container) = self.units.get_mut(container_id) {
                container.garrison.remove_contained_unit(unit_id);
            }
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.garrison.set_container(None);
            }
        }
        for (_, squad) in self.squads.iter_mut() {
            squad.garrison.remove_contained_squad(squad_id);
        }
    }

    fn emergency_release_squad(&mut self, squad_id: EntityId, position: Vec3, forward: Vec3) {
        if !self.squads.contains(squad_id) {
            return;
        }
        self.detach_passenger_refs(squad_id);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.garrison.finish_action();
            squad.clear_attack_order();
            squad.stop();
            squad.state = SquadState::Idle;
            squad.base.position = position;
            squad.base.forward = normalized_forward(forward);
        }
        self.place_squad_members(squad_id, position, forward, true);
    }

    fn sync_contained_squads(&mut self) {
        let transforms = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| match squad.garrison.state() {
                SquadContainmentState::Garrisoned { container, .. } => {
                    let unit = self.units.get(container)?;
                    Some((squad_id, unit.base.position, unit.base.forward, false))
                }
                SquadContainmentState::Ungarrisoning {
                    destination,
                    exit_position,
                    ..
                } => Some((
                    squad_id,
                    exit_position,
                    self.units
                        .get(destination)
                        .map_or(squad.base.forward, |unit| unit.base.forward),
                    true,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (squad_id, position, forward, spread_members) in transforms {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.base.position = position;
                squad.base.forward = normalized_forward(forward);
                squad.base.velocity = Vec3::ZERO;
                squad.state = SquadState::Idle;
            }
            self.place_squad_members(squad_id, position, forward, spread_members);
        }
    }

    pub(crate) fn place_squad_members(
        &mut self,
        squad_id: EntityId,
        position: Vec3,
        forward: Vec3,
        spread_members: bool,
    ) {
        let forward = normalized_forward(forward);
        let unit_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        for unit_id in unit_ids {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.base.position = if spread_members {
                position + formation_offset_to_world(forward, unit.formation_offset)
            } else {
                position
            };
            unit.base.forward = forward;
            unit.base.velocity = Vec3::ZERO;
            unit.state = UnitState::Idle;
            unit.stop();
        }
    }
}

fn unit_obstruction_radius(unit: &Unit) -> f32 {
    unit.obstruction_half_extents
        .x
        .max(unit.obstruction_half_extents.z)
        .max(0.0)
}

fn normalized_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

fn valid_nonnegative(value: f32) -> Option<f32> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn valid_positive(value: f32) -> Option<f32> {
    (value.is_finite() && value > 0.0).then_some(value)
}
