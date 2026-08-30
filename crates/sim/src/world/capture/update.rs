//! Fixed-step capture work, exclusivity, decay, and ownership transfer.

use super::{CaptureTarget, World, planar_direction, planar_forward, stop_capture_unit};
use crate::entities::{CapturePhase, SquadMode, SquadState, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{CaptureActionProfile, GameplayCatalog};
use crate::player::PlayerId;
use glam::Vec3;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct CaptureParticipant {
    unit_id: EntityId,
    position: Vec3,
    radius: f32,
    work_range: f32,
    work_rate: f32,
    die_on_built: bool,
}

impl World {
    pub(in crate::world) fn update_captures(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        self.reconcile_capture_state();
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                matches!(
                    squad.capture.phase(),
                    CapturePhase::Moving | CapturePhase::Working
                )
                .then_some(squad_id)
            })
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_capture_squad(squad_id, dt, gameplay);
        }
        self.update_capture_decay(dt, database);
    }

    fn update_capture_squad(&mut self, squad_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let Some((player_id, target_id)) = self.active_capture_order(squad_id) else {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
            return;
        };
        let Some(target) = self.capture_target(target_id) else {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
            return;
        };
        if target.player_id == player_id {
            self.finish_capture_order(squad_id, CapturePhase::Done, true);
            return;
        }
        if !self.capture_target_accepts(&target, player_id, squad_id) {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
            return;
        }
        let participants = self.capture_participants(squad_id, target_id, gameplay);
        if participants.is_empty() {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
            return;
        }
        self.cancel_invalid_capture_members(squad_id, target_id, &participants);
        let workers = participants
            .iter()
            .filter(|participant| capture_in_range(participant, &target))
            .cloned()
            .collect::<Vec<_>>();
        if workers.is_empty() {
            self.handle_capture_out_of_range(squad_id, target_id, &target, &participants);
            return;
        }
        self.hold_capture_position(squad_id, target_id, &target, &participants, &workers);
        if self.apply_capture_work(player_id, target_id, dt, &workers) {
            self.complete_capture(squad_id, target_id, player_id, &workers);
        }
    }

    fn active_capture_order(&self, squad_id: EntityId) -> Option<(PlayerId, EntityId)> {
        let order = self.capture_order(squad_id)?;
        let squad = self.squads.get(squad_id)?;
        (matches!(
            squad.capture.phase(),
            CapturePhase::Moving | CapturePhase::Working
        ) && self
            .capture_order_source(order.player_id, squad_id)
            .is_some())
        .then_some((order.player_id, order.target_id))
    }

    fn capture_participants(
        &self,
        squad_id: EntityId,
        target_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Vec<CaptureParticipant> {
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
                    .capture_action(&unit.proto_object_name, unit.capture.action.action_name())?;
                (unit.capture.action.target_id() == Some(target_id)
                    && self.capture_profile_enabled(unit, profile))
                .then(|| self.capture_participant(unit_id, unit, profile))
            })
            .collect()
    }

    fn capture_participant(
        &self,
        unit_id: EntityId,
        unit: &Unit,
        profile: &CaptureActionProfile,
    ) -> CaptureParticipant {
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
        CaptureParticipant {
            unit_id,
            position: unit.base.position,
            radius: unit.obstruction_radius(),
            work_range: profile.work_range(),
            work_rate: finite_nonnegative(work_rate),
            die_on_built: profile.die_on_built(),
        }
    }

    fn capture_profile_enabled(&self, unit: &Unit, profile: &CaptureActionProfile) -> bool {
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

    fn handle_capture_out_of_range(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        target: &CaptureTarget,
        participants: &[CaptureParticipant],
    ) {
        let was_working = self
            .squads
            .get(squad_id)
            .is_some_and(|squad| squad.capture.phase() == CapturePhase::Working);
        let unit_ids = participants
            .iter()
            .map(|participant| participant.unit_id)
            .collect::<Vec<_>>();
        self.stop_capture_activity(squad_id, target_id, &unit_ids);
        if was_working && !self.begin_second_capture_approach(squad_id, &unit_ids) {
            self.finish_capture_order(squad_id, CapturePhase::Failed, false);
            return;
        }
        self.move_to_capture_target(squad_id, target, participants);
    }

    fn begin_second_capture_approach(&mut self, squad_id: EntityId, unit_ids: &[EntityId]) -> bool {
        let allowed = self
            .squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.capture.begin_second_approach());
        if allowed {
            for unit_id in unit_ids {
                if let Some(unit) = self.units.get_mut(*unit_id) {
                    let _started = unit.capture.action.begin_second_approach();
                }
            }
        }
        allowed
    }

    fn move_to_capture_target(
        &mut self,
        squad_id: EntityId,
        target: &CaptureTarget,
        participants: &[CaptureParticipant],
    ) {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        let destination = capture_approach(
            squad.base.position,
            squad.base.forward,
            target,
            participants,
        );
        squad.start_direct_move(destination);
        squad.capture.set_phase(CapturePhase::Moving);
        for participant in participants {
            if let Some(unit) = self.units.get_mut(participant.unit_id) {
                unit.capture.action.set_phase(CapturePhase::Moving);
            }
        }
    }

    fn hold_capture_position(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        target: &CaptureTarget,
        participants: &[CaptureParticipant],
        workers: &[CaptureParticipant],
    ) {
        let unit_ids = participants
            .iter()
            .map(|participant| participant.unit_id)
            .collect::<Vec<_>>();
        self.stop_capture_activity(squad_id, target_id, &unit_ids);
        let direction = self.squads.get(squad_id).map_or(Vec3::ZERO, |squad| {
            planar_direction(target.position, squad.base.position)
        });
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            squad.capture.set_phase(CapturePhase::Working);
            if direction != Vec3::ZERO {
                squad.base.set_forward(direction);
            }
        }
        for participant in participants {
            let working = workers
                .binary_search_by_key(&participant.unit_id, |worker| worker.unit_id)
                .is_ok();
            if let Some(unit) = self.units.get_mut(participant.unit_id) {
                unit.capture.action.set_phase(if working {
                    CapturePhase::Working
                } else {
                    CapturePhase::Moving
                });
                stop_capture_unit(unit);
                let facing = planar_direction(target.position, unit.base.position);
                if facing != Vec3::ZERO {
                    unit.base.set_forward(facing);
                }
            }
            if working && let Some(target) = self.units.get_mut(target_id) {
                target.capture.target.start_unit(participant.unit_id);
            }
        }
    }

    fn apply_capture_work(
        &mut self,
        player_id: PlayerId,
        target_id: EntityId,
        dt: f32,
        workers: &[CaptureParticipant],
    ) -> bool {
        for worker in workers {
            let amount = finite_nonnegative(worker.work_rate * dt);
            let completed = self
                .units
                .get_mut(target_id)
                .is_some_and(|target| target.capture.target.apply_work(player_id, amount));
            if completed {
                return true;
            }
        }
        false
    }

    fn complete_capture(
        &mut self,
        source_squad_id: EntityId,
        target_id: EntityId,
        actor_player_id: PlayerId,
        workers: &[CaptureParticipant],
    ) {
        let new_owner = self.capture_completion_owner(actor_player_id);
        let target_squad_id = self.units.get(target_id).and_then(|unit| unit.squad_id);
        let changed = if let Some(squad_id) = target_squad_id {
            self.change_squad_owner(squad_id, new_owner)
        } else {
            self.change_unit_owner(target_id, new_owner)
        };
        if !changed {
            self.finish_capture_order(source_squad_id, CapturePhase::Failed, false);
            return;
        }
        self.restore_captured_target(target_id, target_squad_id);
        if let Some(target) = self.units.get_mut(target_id) {
            target.capture.target.reset_progress();
            target.capture.target.clear_activity();
        }
        self.finish_capture_order(source_squad_id, CapturePhase::Done, true);
        let deaths = workers
            .iter()
            .filter(|worker| worker.die_on_built)
            .map(|worker| worker.unit_id)
            .collect::<Vec<_>>();
        for unit_id in deaths {
            let _killed = self.kill_unit(unit_id, false);
        }
    }

    fn capture_completion_owner(&self, actor_player_id: PlayerId) -> PlayerId {
        if !self.is_coop() {
            return actor_player_id;
        }
        self.get_player(actor_player_id)
            .and_then(crate::player::Player::coop_player_id)
            .filter(|partner_id| self.get_player(*partner_id).is_some())
            .unwrap_or(actor_player_id)
    }

    fn restore_captured_target(&mut self, target_id: EntityId, target_squad_id: Option<EntityId>) {
        let member_ids = target_squad_id
            .and_then(|squad_id| {
                self.squads.get_mut(squad_id).map(|squad| {
                    squad.mode = SquadMode::Lockdown;
                    squad.unit_ids.clone()
                })
            })
            .unwrap_or_else(|| vec![target_id]);
        for member_id in member_ids {
            if let Some(unit) = self.units.get_mut(member_id) {
                unit.hitpoints = unit.max_hitpoints;
            }
        }
    }

    pub(super) fn finish_capture_order(
        &mut self,
        squad_id: EntityId,
        phase: CapturePhase,
        captured: bool,
    ) {
        let Some(order) = self.capture_order(squad_id) else {
            return;
        };
        self.stop_capture_activity(squad_id, order.target_id, &order.unit_ids);
        self.disconnect_capture_link(order.target_id, order.player_id, squad_id, captured);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.capture.finish(phase);
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
        for unit_id in order.unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.capture.action.target_id() == Some(order.target_id)
            {
                unit.capture.action.finish(phase);
                stop_capture_unit(unit);
            }
        }
    }

    fn update_capture_decay(&mut self, dt: f32, database: &Database) {
        let rate = database
            .game_data
            .as_ref()
            .and_then(|data| data.capture_decay_rate)
            .filter(|rate| rate.is_finite() && *rate > 0.0)
            .unwrap_or_default();
        let amount = finite_nonnegative(rate * dt);
        if amount <= 0.0 {
            return;
        }
        for (_, target) in self.units.iter_mut() {
            target.capture.target.decay(amount);
        }
    }

    fn reconcile_capture_state(&mut self) {
        self.reconcile_capture_units();
        self.reconcile_capture_links();
        self.reconcile_capture_activity();
    }

    fn reconcile_capture_units(&mut self) {
        let stale = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                let target_id = unit.capture.action.target_id()?;
                let connected = unit.squad_id.and_then(|squad_id| {
                    self.squads.get(squad_id).map(|squad| {
                        matches!(
                            squad.capture.phase(),
                            CapturePhase::Moving | CapturePhase::Working
                        ) && squad.capture.target_id() == Some(target_id)
                    })
                });
                (connected != Some(true)).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in stale {
            self.detach_capture_source_unit(unit_id);
        }
    }

    fn reconcile_capture_links(&mut self) {
        let links = self
            .units
            .iter()
            .flat_map(|(target_id, target)| {
                target
                    .capture
                    .target
                    .linked_squads()
                    .into_iter()
                    .map(move |(player_id, squad_id)| (target_id, player_id, squad_id))
            })
            .collect::<Vec<_>>();
        for (target_id, player_id, squad_id) in links {
            let connected = self.squads.get(squad_id).is_some_and(|squad| {
                squad.capture.target_id() == Some(target_id)
                    && squad.capture.player_id() == Some(player_id)
                    && matches!(
                        squad.capture.phase(),
                        CapturePhase::Moving | CapturePhase::Working
                    )
            });
            if !connected {
                self.disconnect_capture_link(target_id, player_id, squad_id, false);
            }
        }
    }

    fn reconcile_capture_activity(&mut self) {
        let activity = self
            .units
            .iter()
            .map(|(target_id, target)| {
                (target_id, target.capture.target.active_unit_ids().to_vec())
            })
            .collect::<Vec<_>>();
        let mut stale = Vec::new();
        for (target_id, unit_ids) in activity {
            for unit_id in unit_ids {
                if self.units.get(unit_id).is_none_or(|unit| {
                    unit.capture.action.target_id() != Some(target_id)
                        || unit.capture.action.phase() != CapturePhase::Working
                }) {
                    stale.push((target_id, unit_id));
                }
            }
        }
        for (target_id, unit_id) in stale {
            if let Some(target) = self.units.get_mut(target_id) {
                target.capture.target.stop_unit(unit_id);
            }
        }
    }

    fn cancel_invalid_capture_members(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        participants: &[CaptureParticipant],
    ) {
        let unit_ids = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        for unit_id in unit_ids {
            if participants
                .binary_search_by_key(&unit_id, |participant| participant.unit_id)
                .is_ok()
            {
                continue;
            }
            if let Some(target) = self.units.get_mut(target_id) {
                target.capture.target.stop_unit(unit_id);
            }
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.cancel_capture_action();
            }
        }
    }
}

fn capture_in_range(participant: &CaptureParticipant, target: &CaptureTarget) -> bool {
    let delta = participant.position - target.position;
    let surface_distance = delta.x.hypot(delta.z) - participant.radius - target.radius;
    surface_distance.max(0.0) <= participant.work_range
}

fn capture_approach(
    squad_position: Vec3,
    squad_forward: Vec3,
    target: &CaptureTarget,
    participants: &[CaptureParticipant],
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
    let reach = participants
        .iter()
        .map(|participant| participant.radius + participant.work_range)
        .fold(0.0_f32, f32::max);
    let mut destination = target.position + away * (target.radius + reach);
    destination.y = squad_position.y;
    destination
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}
