//! Dynamic goto-base and rally behavior installed by `LockdownMenu` units.

use super::World;
use crate::EntityId;
use crate::entities::SquadMode;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use glam::Vec3;
use num_traits::ToPrimitive;
use std::f32::consts::FRAC_PI_8;

const DEFAULT_RALLY_DISTANCE: f32 = 50.0;
const RALLY_CANDIDATE_COUNT: usize = 8;
const MIN_RALLY_CLEARANCE: f32 = 0.5;

#[derive(Debug, Clone)]
struct LockdownMenuSnapshot {
    leader_id: EntityId,
    player_id: PlayerId,
    prototype: String,
    position: Vec3,
    forward: Vec3,
}

impl World {
    pub(in crate::world) fn remove_unit_goto_base_references(&mut self, unit_id: EntityId) {
        let affected_players = self
            .players_mut()
            .filter_map(|player| player.remove_goto_base(unit_id).then_some(player.id))
            .collect::<Vec<_>>();
        for player_id in affected_players {
            let _cleared = self.clear_unit_rally_point(unit_id, player_id);
            self.check_player_rally_point(player_id);
        }
    }

    pub(in crate::world) fn transfer_unit_goto_base_reference(
        &mut self,
        unit_id: EntityId,
        old_owner: PlayerId,
        new_owner: PlayerId,
    ) {
        if old_owner == new_owner {
            return;
        }
        let transferred = self
            .get_player_mut(old_owner)
            .is_some_and(|player| player.remove_goto_base(unit_id));
        if !transferred {
            return;
        }
        if let Some(player) = self.get_player_mut(new_owner) {
            player.add_goto_base(unit_id);
        }
        self.check_player_rally_point(old_owner);
        self.check_player_rally_point(new_owner);
    }

    pub(super) fn apply_lockdown_menu_mode(
        &mut self,
        squad_id: EntityId,
        source_mode: SquadMode,
        target_mode: SquadMode,
        gameplay: &GameplayCatalog,
    ) {
        let Some(snapshot) = self.lockdown_menu_snapshot(squad_id) else {
            return;
        };
        if !gameplay.has_lockdown_menu(&snapshot.prototype)
            || !gameplay.uses_vehicle_goto(&snapshot.prototype)
        {
            return;
        }
        if source_mode == SquadMode::Lockdown && target_mode != SquadMode::Lockdown {
            let _cleared = self.clear_unit_rally_point(snapshot.leader_id, snapshot.player_id);
            if let Some(player) = self.get_player_mut(snapshot.player_id) {
                player.remove_goto_base(snapshot.leader_id);
            }
            self.check_player_rally_point(snapshot.player_id);
        } else if source_mode != SquadMode::Lockdown && target_mode == SquadMode::Lockdown {
            if let Some(player) = self.get_player_mut(snapshot.player_id) {
                player.add_goto_base(snapshot.leader_id);
            }
            if gameplay.has_rally_point(&snapshot.prototype)
                && let Some(position) = self.default_lockdown_rally_point(&snapshot)
            {
                let _set = self.set_unit_rally_point(
                    snapshot.leader_id,
                    snapshot.player_id,
                    position,
                    None,
                );
            }
        }
    }

    fn lockdown_menu_snapshot(&self, squad_id: EntityId) -> Option<LockdownMenuSnapshot> {
        let squad = self.squads.get(squad_id)?;
        let leader = squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .find(|unit| unit.is_operational())?;
        Some(LockdownMenuSnapshot {
            leader_id: leader.base.id,
            player_id: squad.base.player_id,
            prototype: leader.proto_object_name.clone(),
            position: leader.base.position,
            forward: leader.base.forward,
        })
    }

    fn default_lockdown_rally_point(&self, snapshot: &LockdownMenuSnapshot) -> Option<Vec3> {
        let forward = Vec3::new(snapshot.forward.x, 0.0, snapshot.forward.z)
            .try_normalize()
            .unwrap_or(Vec3::Z);
        let behind = -forward * DEFAULT_RALLY_DISTANCE;
        (0..RALLY_CANDIDATE_COUNT).find_map(|index| {
            let angle = FRAC_PI_8 * index.to_f32()?;
            let (sin, cos) = angle.sin_cos();
            let offset = Vec3::new(
                behind.x * cos - behind.z * sin,
                0.0,
                behind.x * sin + behind.z * cos,
            );
            self.valid_lockdown_rally_candidate(snapshot, snapshot.position + offset)
        })
    }

    fn valid_lockdown_rally_candidate(
        &self,
        snapshot: &LockdownMenuSnapshot,
        mut candidate: Vec3,
    ) -> Option<Vec3> {
        if let Some(bounds) = self.terrain_bounds()
            && !bounds.contains(candidate)
        {
            return None;
        }
        if self.has_terrain_simulation() {
            candidate.y = self.terrain_height(candidate, false)?;
        }
        (!self.rally_candidate_is_obstructed(snapshot.leader_id, candidate)).then_some(candidate)
    }

    fn rally_candidate_is_obstructed(&self, leader_id: EntityId, candidate: Vec3) -> bool {
        self.units.iter().any(|(unit_id, unit)| {
            if unit_id == leader_id || !unit.is_alive() {
                return false;
            }
            let offset = (candidate - unit.base.position).abs();
            let extents = unit
                .obstruction_half_extents
                .abs()
                .max(Vec3::splat(MIN_RALLY_CLEARANCE));
            offset.x <= extents.x && offset.z <= extents.z
        })
    }
}
