//! Retail Mines work-order execution and deterministic object placement.

use super::World;
use crate::entities::squads::MineOrder;
use crate::entities::{SquadMode, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackQuery, AttackQueryFlags, GameplayCatalog, MineActionProfile, TacticRelation,
};
use crate::player::{PlayerId, TeamRelation};
use crate::scenario::create_object_from_prototype;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

const MINE_SPACING: f32 = 5.0;
const MINE_SEARCH_RANGE: f32 = 20.0;
const POSITION_EPSILON: f32 = 0.01;

#[derive(Debug, Clone)]
struct MineTargetSnapshot {
    position: Vec3,
    player_id: Option<PlayerId>,
    proto_object_name: Option<String>,
    flags: AttackQueryFlags,
    entity_target: bool,
}

impl World {
    /// Issue a database-backed Mines order to one owned squad.
    pub fn issue_mines_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target_entity: Option<EntityId>,
        target_position: Option<Vec3>,
        explicit_range: Option<f32>,
        requested_ability_id: u8,
    ) -> bool {
        if self.get_player(player_id).is_none()
            || target_entity.is_some_and(EntityId::is_invalid)
            || target_position.is_some_and(|position| !position.is_finite())
            || explicit_range.is_some_and(|range| !range.is_finite())
            || target_entity.is_some_and(|target| self.squad_move_entity_target(target).is_none())
        {
            return false;
        }
        if self
            .squads
            .get(squad_id)
            .is_none_or(|squad| squad.base.player_id != player_id)
        {
            return false;
        }
        let _cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        let accepted = self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.begin_mines_order(MineOrder {
                target_entity,
                target_position,
                explicit_range,
                requested_ability_id,
            })
        });
        if accepted {
            self.cancel_incoming_power_transport(squad_id);
        }
        accepted
    }

    pub(super) fn update_mines(&mut self, database: &Database, gameplay: &GameplayCatalog) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| squad.is_placing_mines().then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_squad_mines(squad_id, database, gameplay);
        }
    }

    fn update_squad_mines(
        &mut self,
        squad_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some((order, source_position, source_player, squad_mode, workers)) =
            self.squads.get(squad_id).and_then(|squad| {
                if squad.state != SquadState::Working {
                    return None;
                }
                Some((
                    squad.mines.order()?,
                    squad.base.position,
                    squad.base.player_id,
                    squad.mode,
                    squad.mines.pending_workers(),
                ))
            })
        else {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.cancel_mines_order();
            }
            return;
        };
        let Some(target) = self.mine_target_snapshot(order) else {
            self.finish_mine_workers(squad_id, &workers);
            self.complete_finished_mines(squad_id);
            return;
        };
        for unit_id in workers {
            let Some(profile) = self.select_mine_profile(
                unit_id,
                source_player,
                squad_mode,
                order.requested_ability_id,
                &target,
                gameplay,
            ) else {
                self.finish_mine_worker(squad_id, unit_id);
                continue;
            };
            let range = order.explicit_range.unwrap_or(profile.work_range());
            if !mine_target_is_in_range(source_position, &target, range)
                || !self.place_mine_for_worker(squad_id, unit_id, &profile, database)
            {
                self.finish_mine_worker(squad_id, unit_id);
            }
        }
        self.complete_finished_mines(squad_id);
    }

    fn select_mine_profile(
        &self,
        unit_id: EntityId,
        source_player: PlayerId,
        squad_mode: SquadMode,
        ability_id: u8,
        target: &MineTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> Option<MineActionProfile> {
        let unit = self.units.get(unit_id)?;
        if !unit.is_operational() || unit.base.player_id != source_player {
            return None;
        }
        let query = AttackQuery {
            relation: target
                .player_id
                .map_or(TacticRelation::Enemy, |target_player| {
                    self.mine_tactic_relation(source_player, target_player)
                }),
            squad_mode,
            ability_id: Some(ability_id),
            target_proto_object_name: target.proto_object_name.as_deref(),
            tactic_state: unit.tactic_state(),
            flags: target.flags,
        };
        gameplay.select_mine_action(&unit.proto_object_name, &query, |action| {
            let authored_enabled = action.start_disabled != Some(true);
            let player_enabled =
                self.get_player(source_player)
                    .map_or(authored_enabled, |player| {
                        player.technologies.action_enabled(
                            &unit.proto_object_name,
                            &action.name,
                            authored_enabled,
                        )
                    });
            unit.actions.is_enabled(&action.name, !player_enabled)
        })
    }

    fn place_mine_for_worker(
        &mut self,
        squad_id: EntityId,
        unit_id: EntityId,
        profile: &MineActionProfile,
        database: &Database,
    ) -> bool {
        let (owner, position, cost) = {
            let Some(unit) = self.units.get(unit_id) else {
                return false;
            };
            (
                unit.base.player_id,
                unit.base.position,
                profile.ammunition_cost(),
            )
        };
        if cost > 0.0 {
            let Some(unit) = self.units.get_mut(unit_id) else {
                return false;
            };
            if unit.ammunition.current() < cost {
                return false;
            }
            unit.ammunition.adjust(-cost);
        }
        let spawned = self
            .find_mine_position(position, profile.mine_object_name(), database)
            .and_then(|mine_position| {
                create_object_from_prototype(
                    self,
                    owner,
                    mine_position,
                    Vec3::Z,
                    profile.mine_object_name(),
                    database,
                )
            });
        if spawned.is_none() {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.ammunition.adjust(cost);
            }
            return false;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.mines.mark_placed(unit_id);
        }
        true
    }

    fn mine_target_snapshot(&self, order: MineOrder) -> Option<MineTargetSnapshot> {
        if let Some(target_id) = order.target_entity {
            let (unit_id, position) = self.squad_move_entity_target(target_id)?;
            let unit = self.units.get(unit_id)?;
            let mut flags = AttackQueryFlags::empty();
            if unit.base.player_id == 0 {
                flags.insert(AttackQueryFlags::TARGET_GAIA);
            }
            if unit.hitpoints < unit.max_hitpoints {
                flags.insert(AttackQueryFlags::TARGET_DAMAGED);
            }
            if unit.is_building() && !unit.built {
                flags.insert(AttackQueryFlags::TARGET_UNBUILT);
            }
            return Some(MineTargetSnapshot {
                position,
                player_id: Some(unit.base.player_id),
                proto_object_name: Some(unit.proto_object_name.clone()),
                flags,
                entity_target: true,
            });
        }
        Some(MineTargetSnapshot {
            position: order.target_position?,
            player_id: None,
            proto_object_name: None,
            flags: AttackQueryFlags::empty(),
            entity_target: false,
        })
    }

    fn mine_tactic_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
        if source == target {
            return TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => TacticRelation::Ally,
            Some(TeamRelation::Enemy) => TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
        }
    }

    fn finish_mine_worker(&mut self, squad_id: EntityId, unit_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.mines.finish_worker(unit_id);
        }
    }

    fn finish_mine_workers(&mut self, squad_id: EntityId, unit_ids: &[EntityId]) {
        for &unit_id in unit_ids {
            self.finish_mine_worker(squad_id, unit_id);
        }
    }

    fn complete_finished_mines(&mut self, squad_id: EntityId) {
        if self
            .squads
            .get(squad_id)
            .is_some_and(|squad| squad.mines.all_finished())
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.complete_mines_order();
        }
    }

    fn find_mine_position(
        &self,
        owner_position: Vec3,
        proto_name: &str,
        database: &Database,
    ) -> Option<Vec3> {
        let prototype = database
            .objects
            .iter()
            .find(|prototype| prototype.name.eq_ignore_ascii_case(proto_name))?;
        let mut mine_positions = self
            .units
            .iter()
            .filter_map(|(_, unit)| {
                (unit.is_alive()
                    && unit.proto_object_name.eq_ignore_ascii_case(proto_name)
                    && xz_distance_squared(unit.base.position, owner_position)
                        <= MINE_SEARCH_RANGE * MINE_SEARCH_RANGE)
                    .then_some(unit.base.position)
            })
            .collect::<Vec<_>>();
        let snapped = snapped_mine_origin(owner_position);
        if mine_positions.is_empty() {
            mine_positions.push(snapped);
        }
        mine_spiral_positions(snapped)
            .into_iter()
            .find_map(|mut candidate| {
                if mine_positions
                    .iter()
                    .any(|position| same_xz(*position, candidate))
                    || self.is_outside_playable_bounds(candidate, true)
                    || !self.mine_spot_is_clear(candidate, prototype)
                {
                    return None;
                }
                if let Some(height) = self.terrain_height(candidate, false) {
                    candidate.y = height;
                }
                Some(candidate)
            })
    }

    fn mine_spot_is_clear(&self, candidate: Vec3, prototype: &ProtoObject) -> bool {
        let mine_x = valid_radius(prototype.obstruction_radius_x);
        let mine_z = valid_radius(prototype.obstruction_radius_z);
        if mine_x == 0.0 && mine_z == 0.0 {
            return true;
        }
        self.units.iter().all(|(_, unit)| {
            !unit.is_alive()
                || !aabbs_overlap_xz(
                    candidate,
                    Vec3::new(mine_x, 0.0, mine_z),
                    unit.base.position,
                    unit.obstruction_half_extents,
                )
        })
    }
}

fn mine_target_is_in_range(source: Vec3, target: &MineTargetSnapshot, range: f32) -> bool {
    if !range.is_finite() || range < 0.0 {
        return false;
    }
    let distance_squared = xz_distance_squared(source, target.position);
    if target.entity_target {
        distance_squared <= range * range
    } else {
        distance_squared < range * range
    }
}

fn snapped_mine_origin(position: Vec3) -> Vec3 {
    Vec3::new(
        (position.x / MINE_SPACING).trunc() * MINE_SPACING,
        position.y,
        (position.z / MINE_SPACING).trunc() * MINE_SPACING,
    )
}

fn mine_spiral_positions(origin: Vec3) -> Vec<Vec3> {
    let mut positions = Vec::with_capacity(81);
    let mut start_x = origin.x;
    let mut start_z = origin.z;
    let mut step_count = 1_i32;
    for _ in 0..=4 {
        let mut x = start_x;
        let mut z = start_z;
        for _ in 0..step_count {
            positions.push(Vec3::new(x, origin.y, z));
            x += MINE_SPACING;
        }
        if step_count > 1 {
            x -= MINE_SPACING;
            z += MINE_SPACING;
            for _ in 0..step_count - 1 {
                positions.push(Vec3::new(x, origin.y, z));
                z += MINE_SPACING;
            }
            x -= MINE_SPACING;
            z -= MINE_SPACING;
            for _ in 0..step_count - 1 {
                positions.push(Vec3::new(x, origin.y, z));
                x -= MINE_SPACING;
            }
            x += MINE_SPACING;
            z -= MINE_SPACING;
            for _ in 0..step_count - 2 {
                positions.push(Vec3::new(x, origin.y, z));
                z -= MINE_SPACING;
            }
        }
        start_x -= MINE_SPACING;
        start_z -= MINE_SPACING;
        step_count += 2;
    }
    positions
}

fn xz_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x * delta.x + delta.z * delta.z
}

fn same_xz(left: Vec3, right: Vec3) -> bool {
    (left.x - right.x).abs() < POSITION_EPSILON && (left.z - right.z).abs() < POSITION_EPSILON
}

fn valid_radius(radius: Option<f32>) -> f32 {
    radius
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_default()
}

fn aabbs_overlap_xz(
    left_position: Vec3,
    left_half_extents: Vec3,
    right_position: Vec3,
    right_half_extents: Vec3,
) -> bool {
    (left_position.x - right_position.x).abs()
        < left_half_extents.x.abs() + right_half_extents.x.abs()
        && (left_position.z - right_position.z).abs()
            < left_half_extents.z.abs() + right_half_extents.z.abs()
}

#[cfg(test)]
mod tests;
