//! Authoritative execution of retail per-unit `Gather` actions.

use super::World;
use crate::entities::{GatherPhase, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, GatherActionProfile};
use crate::player::{GAIA_PLAYER, PlayerId};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct GatherTarget {
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    radius: f32,
    resource_name: String,
    amount: f32,
    unlimited: bool,
    die_at_zero: bool,
    gatherer_limit: i32,
}

#[derive(Debug, Clone)]
struct GatherParticipant {
    unit_id: EntityId,
    position: Vec3,
    radius: f32,
    work_range: f32,
    work_rate: f32,
    resource_id: usize,
    team_share: bool,
}

impl World {
    /// Issue a squad gather order against one finite or unlimited unit resource node.
    pub fn issue_gather_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some((resource_name, unit_ids)) =
            self.gather_order_context(player_id, squad_id, target_id)
        else {
            return false;
        };
        let selections = unit_ids
            .iter()
            .filter_map(|&unit_id| {
                let unit = self.units.get(unit_id)?;
                let profile = gameplay
                    .gather_actions(&unit.proto_object_name)
                    .iter()
                    .find(|profile| {
                        profile.resource_name().eq_ignore_ascii_case(&resource_name)
                            && self.gather_action_enabled(unit, profile)
                    })?;
                Some((unit_id, profile.action_name().to_owned()))
            })
            .collect::<Vec<_>>();
        if selections.is_empty() {
            return false;
        }
        let _cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        squad.begin_gather(target_id);
        for unit_id in unit_ids {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.stop();
            if let Some((_, action_name)) = selections.iter().find(|(id, _)| *id == unit_id) {
                unit.gather.start(target_id, action_name);
            } else {
                unit.cancel_gather_action();
            }
        }
        self.cancel_incoming_power_transport(squad_id);
        true
    }

    /// Count units that currently own working gather controllers for a target.
    #[must_use]
    pub fn unit_resource_gatherer_count(&self, target_id: EntityId) -> usize {
        self.units
            .iter()
            .filter(|(_, unit)| {
                unit.is_alive()
                    && unit.gather.target_id() == Some(target_id)
                    && unit.gather.phase() == GatherPhase::Working
            })
            .count()
    }

    /// Return whether any unit is actively gathering from a target resource node.
    #[must_use]
    pub fn is_unit_being_gathered_from(&self, target_id: EntityId) -> bool {
        self.unit_resource_gatherer_count(target_id) != 0
    }

    pub(super) fn update_gathering(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.reconcile_gather_members();
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                (squad.is_alive()
                    && matches!(
                        squad.gather.phase(),
                        GatherPhase::Moving | GatherPhase::Working
                    ))
                .then_some(squad_id)
            })
            .collect::<Vec<_>>();
        let mut reservations = BTreeMap::<EntityId, usize>::new();
        let mut depleted_targets = BTreeSet::new();
        for squad_id in squad_ids {
            if let Some(target_id) =
                self.update_gather_squad(squad_id, dt, gameplay, &mut reservations)
            {
                depleted_targets.insert(target_id);
            }
        }
        for target_id in depleted_targets {
            let _killed = self.kill_unit(target_id, false);
        }
    }

    fn gather_order_context(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
        target_id: EntityId,
    ) -> Option<(String, Vec<EntityId>)> {
        let squad = self.squads.get(squad_id)?;
        if squad.base.player_id != player_id
            || !squad.is_alive()
            || self.is_squad_incapacitated(squad_id)
            || squad.garrison.is_garrisoned()
            || squad.is_cryo_frozen()
            || squad.is_raging()
            || squad.is_jumping()
            || squad.trained_air_birth.is_some()
            || squad.unit_ids.binary_search(&target_id).is_ok()
        {
            return None;
        }
        let target = self.units.get(target_id).filter(|unit| unit.is_alive())?;
        if !matches!(target.base.player_id, GAIA_PLAYER) && target.base.player_id != player_id {
            return None;
        }
        let resource_name = target.resource_node.resource_name()?.to_owned();
        (target.resource_node.is_available() && target.resource_node.gatherer_limit() != 0)
            .then(|| (resource_name, squad.unit_ids.clone()))
    }

    fn update_gather_squad(
        &mut self,
        squad_id: EntityId,
        dt: f32,
        gameplay: &GameplayCatalog,
        reservations: &mut BTreeMap<EntityId, usize>,
    ) -> Option<EntityId> {
        let Some((player_id, target_id)) = self.active_gather_order(squad_id) else {
            self.finish_gather_order(squad_id, GatherPhase::Failed);
            return None;
        };
        let Some(target) = self.gather_target(target_id) else {
            self.finish_gather_order(squad_id, GatherPhase::Failed);
            return None;
        };
        if target.player_id != GAIA_PLAYER && target.player_id != player_id {
            self.finish_gather_order(squad_id, GatherPhase::Failed);
            return None;
        }
        if !target.unlimited && target.amount <= 0.0 {
            self.finish_gather_order(squad_id, GatherPhase::Done);
            return target.die_at_zero.then_some(target_id);
        }
        let participants = self.gather_participants(squad_id, target_id, &target, gameplay);
        if participants.is_empty() {
            self.finish_gather_order(squad_id, GatherPhase::Failed);
            return None;
        }
        self.cancel_invalid_gather_members(squad_id, target_id, &participants);
        let in_range = participants
            .iter()
            .filter(|participant| gather_in_range(participant, &target))
            .collect::<Vec<_>>();
        let reserved = reservations.get(&target_id).copied().unwrap_or_default();
        let capacity = gather_capacity(target.gatherer_limit, reserved);
        let workers = in_range.into_iter().take(capacity).collect::<Vec<_>>();
        if workers.is_empty() {
            if capacity == 0 {
                self.wait_for_gather_slot(squad_id, target_id, &participants);
            } else {
                self.move_to_gather_target(squad_id, target_id, &target, &participants);
            }
            return None;
        }
        *reservations.entry(target_id).or_default() += workers.len();
        self.hold_gather_position(squad_id, target_id, &target, &participants, &workers);
        let exhausted = self.apply_gather_work(player_id, target_id, dt, &workers);
        if exhausted {
            self.finish_gather_order(squad_id, GatherPhase::Done);
            return target.die_at_zero.then_some(target_id);
        }
        None
    }

    fn active_gather_order(&self, squad_id: EntityId) -> Option<(PlayerId, EntityId)> {
        let squad = self.squads.get(squad_id)?;
        if !squad.is_alive()
            || self.is_squad_incapacitated(squad_id)
            || squad.garrison.is_garrisoned()
            || squad.is_cryo_frozen()
            || squad.is_raging()
            || squad.trained_air_birth.is_some()
        {
            return None;
        }
        Some((squad.base.player_id, squad.gather.target_id()?))
    }

    fn gather_target(&self, target_id: EntityId) -> Option<GatherTarget> {
        let target = self.units.get(target_id).filter(|unit| unit.is_alive())?;
        Some(GatherTarget {
            player_id: target.base.player_id,
            position: target.base.position,
            forward: target.base.forward,
            radius: target.obstruction_radius(),
            resource_name: target.resource_node.resource_name()?.to_owned(),
            amount: target.resource_node.amount(),
            unlimited: target.resource_node.unlimited(),
            die_at_zero: target.resource_node.die_at_zero(),
            gatherer_limit: target.resource_node.gatherer_limit(),
        })
    }

    fn gather_participants(
        &self,
        squad_id: EntityId,
        target_id: EntityId,
        target: &GatherTarget,
        gameplay: &GameplayCatalog,
    ) -> Vec<GatherParticipant> {
        self.squads
            .get(squad_id)
            .into_iter()
            .flat_map(|squad| &squad.unit_ids)
            .filter_map(|&unit_id| {
                let unit = self
                    .units
                    .get(unit_id)
                    .filter(|unit| unit.is_operational())?;
                let profile = gameplay
                    .gather_action_named(&unit.proto_object_name, unit.gather.action_name())?;
                (unit.gather.target_id() == Some(target_id)
                    && profile
                        .resource_name()
                        .eq_ignore_ascii_case(&target.resource_name)
                    && self.gather_action_enabled(unit, profile))
                .then(|| self.gather_participant(unit_id, unit, profile))
            })
            .collect()
    }

    fn gather_participant(
        &self,
        unit_id: EntityId,
        unit: &Unit,
        profile: &GatherActionProfile,
    ) -> GatherParticipant {
        let work_rate =
            self.get_player(unit.base.player_id)
                .map_or(profile.work_rate(), |player| {
                    player.technologies.action_work_rate(
                        &unit.proto_object_name,
                        profile.action_name(),
                        profile.work_rate(),
                    )
                })
                * unit.work_rate_scalar;
        GatherParticipant {
            unit_id,
            position: unit.base.position,
            radius: unit.obstruction_radius(),
            work_range: profile.work_range(),
            work_rate: finite_nonnegative(work_rate),
            resource_id: profile.resource_id(),
            team_share: profile.team_share(),
        }
    }

    fn gather_action_enabled(&self, unit: &Unit, profile: &GatherActionProfile) -> bool {
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    fn reconcile_gather_members(&mut self) {
        for (_, unit) in self.units.iter_mut() {
            let Some(target_id) = unit.gather.target_id() else {
                continue;
            };
            let connected = unit
                .squad_id
                .and_then(|squad_id| self.squads.get(squad_id))
                .is_some_and(|squad| squad.gather.target_id() == Some(target_id));
            if !connected {
                unit.cancel_gather_action();
            }
        }
    }

    fn cancel_invalid_gather_members(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        participants: &[GatherParticipant],
    ) {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.gather.target_id() == Some(target_id)
                && participants
                    .binary_search_by_key(&unit_id, |participant| participant.unit_id)
                    .is_err()
            {
                unit.cancel_gather_action();
            }
        }
    }

    fn move_to_gather_target(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        target: &GatherTarget,
        participants: &[GatherParticipant],
    ) {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        let destination = gather_approach(
            squad.base.position,
            squad.base.forward,
            target,
            participants,
        );
        squad.start_direct_move(destination);
        squad.gather.set_phase(GatherPhase::Moving);
        for participant in participants {
            if let Some(unit) = self.units.get_mut(participant.unit_id)
                && unit.gather.target_id() == Some(target_id)
            {
                unit.gather.set_phase(GatherPhase::Moving);
            }
        }
    }

    fn wait_for_gather_slot(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        participants: &[GatherParticipant],
    ) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            squad.gather.set_phase(GatherPhase::Working);
        }
        for participant in participants {
            if let Some(unit) = self.units.get_mut(participant.unit_id)
                && unit.gather.target_id() == Some(target_id)
            {
                unit.gather.set_phase(GatherPhase::Moving);
                stop_gather_unit(unit);
            }
        }
    }

    fn hold_gather_position(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        target: &GatherTarget,
        participants: &[GatherParticipant],
        workers: &[&GatherParticipant],
    ) {
        let direction = planar_direction(
            target.position,
            self.squads
                .get(squad_id)
                .map_or(Vec3::ZERO, |s| s.base.position),
        );
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            squad.gather.set_phase(GatherPhase::Working);
            if direction != Vec3::ZERO {
                squad.base.set_forward(direction);
            }
        }
        for participant in participants {
            let working = workers
                .iter()
                .any(|worker| worker.unit_id == participant.unit_id);
            if let Some(unit) = self.units.get_mut(participant.unit_id)
                && unit.gather.target_id() == Some(target_id)
            {
                unit.gather.set_phase(if working {
                    GatherPhase::Working
                } else {
                    GatherPhase::Moving
                });
                stop_gather_unit(unit);
                let facing = planar_direction(target.position, unit.base.position);
                if facing != Vec3::ZERO {
                    unit.base.set_forward(facing);
                }
            }
        }
    }

    fn apply_gather_work(
        &mut self,
        player_id: PlayerId,
        target_id: EntityId,
        dt: f32,
        workers: &[&GatherParticipant],
    ) -> bool {
        for worker in workers {
            let requested = finite_nonnegative(worker.work_rate * dt);
            let gathered = self
                .units
                .get_mut(target_id)
                .map_or(0.0, |target| target.resource_node.gather(requested));
            self.credit_gathered_resource(
                player_id,
                worker.resource_id,
                gathered,
                worker.team_share,
            );
        }
        self.units.get(target_id).is_some_and(|target| {
            !target.resource_node.unlimited() && target.resource_node.amount() <= 0.0
        })
    }

    fn credit_gathered_resource(
        &mut self,
        player_id: PlayerId,
        resource_id: usize,
        amount: f32,
        team_share: bool,
    ) {
        if amount <= 0.0 {
            return;
        }
        if !team_share {
            if let Some(player) = self.get_player_mut(player_id) {
                player.add_resource(resource_id, amount);
            }
            return;
        }
        let Some(team_id) = self.get_player(player_id).map(|player| player.team_id) else {
            return;
        };
        let recipients = self
            .active_players()
            .filter(|player| player.is_playing() && player.team_id == team_id)
            .map(|player| player.id)
            .collect::<Vec<_>>();
        let recipient_count = u16::try_from(recipients.len()).unwrap_or(u16::MAX).max(1);
        let share = amount / f32::from(recipient_count);
        if recipients.is_empty() {
            if let Some(player) = self.get_player_mut(player_id) {
                player.add_resource(resource_id, amount);
            }
        } else {
            for recipient in recipients {
                if let Some(player) = self.get_player_mut(recipient) {
                    player.add_resource(resource_id, share);
                }
            }
        }
    }

    fn finish_gather_order(&mut self, squad_id: EntityId, phase: GatherPhase) {
        let Some((target_id, unit_ids)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.gather.target_id(), squad.unit_ids.clone()))
        else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.gather.set_phase(phase);
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.gather.target_id() == target_id
            {
                unit.gather.set_phase(phase);
                stop_gather_unit(unit);
            }
        }
    }
}

fn gather_capacity(limit: i32, reserved: usize) -> usize {
    if limit < 0 {
        usize::MAX
    } else {
        usize::try_from(limit)
            .unwrap_or_default()
            .saturating_sub(reserved)
    }
}

fn gather_in_range(participant: &GatherParticipant, target: &GatherTarget) -> bool {
    let delta = participant.position - target.position;
    let surface_distance = delta.x.hypot(delta.z) - participant.radius - target.radius;
    surface_distance.max(0.0) <= participant.work_range
}

fn gather_approach(
    squad_position: Vec3,
    squad_forward: Vec3,
    target: &GatherTarget,
    participants: &[GatherParticipant],
) -> Vec3 {
    let mut away = planar_direction(squad_position, target.position);
    if away == Vec3::ZERO {
        away = -planar_forward(target.forward);
    }
    if away == Vec3::ZERO {
        away = -planar_forward(squad_forward);
    }
    if away == Vec3::ZERO {
        away = Vec3::NEG_Z;
    }
    let member_reach = participants
        .iter()
        .map(|participant| participant.radius + participant.work_range)
        .fold(0.0_f32, f32::max);
    let mut destination = target.position + away * (target.radius + member_reach);
    destination.y = squad_position.y;
    destination
}

fn planar_direction(to: Vec3, from: Vec3) -> Vec3 {
    Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero()
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero()
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn stop_gather_unit(unit: &mut Unit) {
    unit.move_target = None;
    unit.base.velocity = Vec3::ZERO;
    if unit.is_alive() {
        unit.state = UnitState::Idle;
    }
}

#[cfg(test)]
mod tests;
