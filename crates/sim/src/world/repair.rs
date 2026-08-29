//! Retail squad repair-cost, combat-value healing, and reinforcement.

use super::World;
use crate::EntityId;
use crate::entity::Entity;
use crate::scenario::add_squad_member_from_prototype;
use num_traits::ToPrimitive;
use pipeline::database::hw1::{Database, ProtoObject};

#[derive(Debug, Clone)]
struct RepairMemberProfile {
    prototype: String,
    count: u32,
    maximum_hitpoints: f32,
    combat_value: f32,
}

#[derive(Debug, Clone, Default)]
struct SquadRepairProfile {
    members: Vec<RepairMemberProfile>,
    maximum_hitpoints: f32,
    combat_value: f32,
}

#[derive(Debug, Clone, Copy)]
struct SpreadRepair {
    squad_id: EntityId,
    base_combat_value: f32,
    repair_difference: f32,
}

impl World {
    pub(crate) fn squad_hitpoint_fraction(&self, squad_id: EntityId, database: &Database) -> f32 {
        let Some(profile) = self.squad_repair_profile(squad_id, database) else {
            return 0.0;
        };
        if profile.maximum_hitpoints < f32::EPSILON {
            return 0.0;
        }
        self.squad_current_hitpoints(squad_id) / profile.maximum_hitpoints
    }

    pub(crate) fn repair_squads_by_combat_value(
        &mut self,
        database: &Database,
        squad_ids: &[EntityId],
        combat_value: f32,
        spread_across_squads: bool,
        allow_reinforce: bool,
    ) {
        if !combat_value.is_finite() || combat_value <= 0.0 || squad_ids.is_empty() {
            return;
        }
        if spread_across_squads {
            self.repair_spread_across_squads(database, squad_ids, combat_value, allow_reinforce);
        } else {
            for &squad_id in squad_ids {
                let _excess = self.repair_squad_by_combat_value(
                    database,
                    squad_id,
                    combat_value,
                    allow_reinforce,
                );
            }
        }
    }

    fn repair_spread_across_squads(
        &mut self,
        database: &Database,
        squad_ids: &[EntityId],
        combat_value: f32,
        allow_reinforce: bool,
    ) {
        let candidates = squad_ids
            .iter()
            .filter_map(|&squad_id| {
                let profile = self.squad_repair_profile(squad_id, database)?;
                let repair_cost = self.squad_repair_cost(squad_id, &profile)?;
                Some((squad_id, profile.combat_value, repair_cost))
            })
            .collect::<Vec<_>>();
        let total_combat_value = candidates.iter().map(|entry| entry.1).sum::<f32>();
        if candidates.is_empty() || total_combat_value < f32::EPSILON {
            return;
        }
        let sorted = sorted_spread_repairs(&candidates, total_combat_value, combat_value);
        self.apply_spread_repairs(
            database,
            &sorted,
            total_combat_value,
            combat_value,
            allow_reinforce,
        );
    }

    fn apply_spread_repairs(
        &mut self,
        database: &Database,
        repairs: &[SpreadRepair],
        mut remaining_combat_value: f32,
        mut remaining_repair: f32,
        allow_reinforce: bool,
    ) {
        for repair in repairs {
            let squad_repair =
                (repair.base_combat_value / remaining_combat_value) * remaining_repair;
            let excess = self.repair_squad_by_combat_value(
                database,
                repair.squad_id,
                squad_repair,
                allow_reinforce,
            );
            remaining_repair -= squad_repair - excess;
            remaining_combat_value -= repair.base_combat_value;
        }
    }

    fn repair_squad_by_combat_value(
        &mut self,
        database: &Database,
        squad_id: EntityId,
        repair_combat_value: f32,
        _allow_reinforce: bool,
    ) -> f32 {
        let Some(profile) = self.squad_repair_profile(squad_id, database) else {
            return 0.0;
        };
        if repair_combat_value <= 0.0
            || profile.combat_value <= 0.0
            || profile.maximum_hitpoints <= 0.0
        {
            return 0.0;
        }
        let requested_hitpoints =
            profile.maximum_hitpoints * (repair_combat_value / profile.combat_value);
        // Retail BSquad::repairCombatValue ignores its AllowReinforce argument.
        let excess_hitpoints =
            self.repair_squad_hitpoints(database, squad_id, &profile, requested_hitpoints, true);
        (excess_hitpoints / profile.maximum_hitpoints) * profile.combat_value
    }

    fn repair_squad_hitpoints(
        &mut self,
        database: &Database,
        squad_id: EntityId,
        profile: &SquadRepairProfile,
        requested_hitpoints: f32,
        allow_reinforce: bool,
    ) -> f32 {
        let Some((alive, member_ids)) = self
            .get_squad(squad_id)
            .map(|squad| (squad.is_alive(), squad.unit_ids.clone()))
        else {
            return 0.0;
        };
        if !alive || member_ids.is_empty() {
            return requested_hitpoints;
        }
        if requested_hitpoints <= 0.0 {
            return 0.0;
        }
        let reinforce = allow_reinforce && authored_member_count(profile) > 1;
        let mut missing = profile
            .members
            .iter()
            .map(|member| member.count)
            .collect::<Vec<_>>();
        let mut remaining = requested_hitpoints;
        if !self.repair_existing_members(&member_ids, profile, &mut missing, &mut remaining) {
            return 0.0;
        }
        if reinforce {
            self.reinforce_missing_members(database, squad_id, profile, &missing, &mut remaining);
        }
        remaining
    }

    fn repair_existing_members(
        &mut self,
        member_ids: &[EntityId],
        profile: &SquadRepairProfile,
        missing: &mut [u32],
        remaining: &mut f32,
    ) -> bool {
        for &unit_id in member_ids {
            let Some((prototype, hitpoints, maximum)) = self
                .get_unit(unit_id)
                .filter(|unit| unit.is_alive())
                .map(|unit| {
                    (
                        unit.proto_object_name.clone(),
                        unit.hitpoints,
                        unit.max_hitpoints,
                    )
                })
            else {
                continue;
            };
            decrement_present_member(profile, missing, &prototype);
            if hitpoints <= 0.0 {
                return false;
            }
            let delta = (maximum - hitpoints).min(*remaining).max(0.0);
            if let Some(unit) = self.get_unit_mut(unit_id) {
                unit.hitpoints += delta;
            }
            *remaining -= delta;
            if *remaining <= 0.0 {
                return false;
            }
        }
        true
    }

    fn reinforce_missing_members(
        &mut self,
        database: &Database,
        squad_id: EntityId,
        profile: &SquadRepairProfile,
        missing: &[u32],
        remaining: &mut f32,
    ) {
        for (member, &count) in profile.members.iter().zip(missing) {
            for _ in 0..count {
                if *remaining <= 0.0 {
                    return;
                }
                let Some(unit_id) =
                    add_squad_member_from_prototype(self, squad_id, &member.prototype, database)
                else {
                    continue;
                };
                let maximum = self
                    .get_unit(unit_id)
                    .map_or(0.0, |unit| unit.max_hitpoints);
                let restored = maximum.min(*remaining).max(0.0);
                if let Some(unit) = self.get_unit_mut(unit_id) {
                    unit.hitpoints = restored;
                }
                *remaining -= restored;
            }
        }
    }

    fn squad_repair_profile(
        &self,
        squad_id: EntityId,
        database: &Database,
    ) -> Option<SquadRepairProfile> {
        let player_id = self.get_squad(squad_id)?.base.player_id;
        let prototype = self.effective_squad_prototype(squad_id, database)?;
        let technologies = &self.get_player(player_id)?.technologies;
        let mut profile = SquadRepairProfile::default();
        for entry in &prototype.units.as_ref()?.entries {
            let count = u32::try_from(entry.count.max(0)).unwrap_or_default();
            let Some(object) = find_object(database, &entry.proto_object) else {
                continue;
            };
            let maximum_hitpoints = effective_hitpoints(technologies, object);
            let combat_value = finite_nonnegative(object.combat_value);
            let count_f32 = count.to_f32().unwrap_or(f32::MAX);
            profile.maximum_hitpoints += maximum_hitpoints * count_f32;
            profile.combat_value += combat_value * count_f32;
            profile.members.push(RepairMemberProfile {
                prototype: object.name.clone(),
                count,
                maximum_hitpoints,
                combat_value,
            });
        }
        Some(profile)
    }

    fn squad_current_hitpoints(&self, squad_id: EntityId) -> f32 {
        self.get_squad(squad_id).map_or(0.0, |squad| {
            squad
                .unit_ids
                .iter()
                .filter_map(|unit_id| self.get_unit(*unit_id))
                .map(|unit| unit.hitpoints)
                .sum()
        })
    }

    fn squad_repair_cost(&self, squad_id: EntityId, profile: &SquadRepairProfile) -> Option<f32> {
        let squad = self.get_squad(squad_id)?;
        let mut total = 0.0;
        for member in &profile.members {
            let maximum = member.maximum_hitpoints * member.count.to_f32().unwrap_or(f32::MAX);
            if maximum <= 0.0 {
                continue;
            }
            let current = squad
                .unit_ids
                .iter()
                .filter_map(|unit_id| self.get_unit(*unit_id))
                .filter(|unit| {
                    unit.proto_object_name
                        .eq_ignore_ascii_case(&member.prototype)
                })
                .map(|unit| unit.hitpoints)
                .sum::<f32>();
            if current < maximum {
                let damage_fraction = (maximum - current) / maximum;
                total += member.combat_value
                    * member.count.to_f32().unwrap_or(f32::MAX)
                    * damage_fraction;
            }
        }
        (total > 0.0).then_some(total)
    }
}

fn sorted_spread_repairs(
    candidates: &[(EntityId, f32, f32)],
    total_combat_value: f32,
    combat_value: f32,
) -> Vec<SpreadRepair> {
    let mut sorted = Vec::<SpreadRepair>::with_capacity(candidates.len());
    for &(squad_id, base_combat_value, repair_cost) in candidates {
        let repair_portion = (base_combat_value / total_combat_value) * combat_value;
        let incoming = SpreadRepair {
            squad_id,
            base_combat_value,
            repair_difference: repair_portion - repair_cost,
        };
        let index = sorted
            .iter()
            .position(|existing| incoming.repair_difference >= existing.repair_difference)
            .unwrap_or(sorted.len());
        sorted.insert(index, incoming);
    }
    sorted
}

fn decrement_present_member(profile: &SquadRepairProfile, missing: &mut [u32], prototype: &str) {
    if let Some((index, _)) = profile
        .members
        .iter()
        .enumerate()
        .find(|(_, member)| member.prototype.eq_ignore_ascii_case(prototype))
    {
        missing[index] = missing[index].saturating_sub(1);
    }
}

fn authored_member_count(profile: &SquadRepairProfile) -> u32 {
    profile
        .members
        .iter()
        .fold(0_u32, |total, member| total.saturating_add(member.count))
}

fn effective_hitpoints(
    technologies: &crate::player::PlayerTechState,
    prototype: &ProtoObject,
) -> f32 {
    prototype.hitpoints.map_or(0.0, |base| {
        let adjusted = technologies.hitpoints(&prototype.name, base);
        if adjusted.is_finite() && adjusted > 0.0 {
            adjusted
        } else {
            0.0
        }
    })
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_default()
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|object| object.name.trim().eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod tests;
