//! Work order 4 (Repair): `BSquadActionRepair`, squad action 65.
//!
//! `BSquad_executeOrder` @ `0x140399780` case 4 creates the action unless the squad is
//! already repairing. The action (vtable `0x141192020`, update `0x1403D26C0`) records the
//! damaged members and the missing members of the squad prototype when it starts. Each update
//! it adds the update length to its timer, `pct = min(timer / repair time, 1)`, where the
//! repair time is `BPlayer+756` (the leader `RepairTime`). The members heal linearly from
//! their start hitpoints during the first `repairPercent` of the time, then the missing members
//! come back one by one. The action fails when the squad takes damage after the start and is
//! done at `pct == 1`.

use super::World;
use crate::EntityId;
use crate::entities::{RepairMember, SquadRepairAction, SquadWorkAction};
use crate::entity::Entity;
use crate::player::PlayerId;
use crate::scenario::add_squad_member_from_prototype;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

impl World {
    /// Start Work order 4 (Repair) on a squad.
    ///
    /// Returns `false` when the command does not reach the squad, the squad is dead or
    /// contained, or the squad already repairs itself (`mFlagIsRepairing`, `squad+741`).
    pub fn issue_squad_repair_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        database: &Database,
    ) -> bool {
        let accepted = self.squads.get(squad_id).is_some_and(|squad| {
            Self::command_reaches_squad_owner(player_id, squad.base.player_id)
                && squad.is_alive()
                && !squad.garrison.is_garrisoned()
                && !squad.is_self_repairing()
        });
        if !accepted {
            return false;
        }
        let action = self.squad_repair_action(squad_id, database);
        let _capture_cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        // `InitFromSquad` sets `mFlagNonMobile`: the squad stops where it is.
        squad.stop();
        squad.work_action = Some(SquadWorkAction::Repair(action));
        true
    }

    /// `BSquadActionRepair::InitFromSquad`: damaged members, missing members and the split of
    /// the repair time.
    fn squad_repair_action(&self, squad_id: EntityId, database: &Database) -> SquadRepairAction {
        let mut damaged_hitpoints = 0.0;
        let mut members = Vec::new();
        let (unit_ids, damaged_time, repair_time) = self.get_squad(squad_id).map_or_else(
            || (Vec::new(), 0, 0.0),
            |squad| {
                (
                    squad.unit_ids.clone(),
                    squad.last_damaged_time,
                    self.player_repair_time(squad.base.player_id, database),
                )
            },
        );
        for &unit_id in &unit_ids {
            let Some(unit) = self.get_unit(unit_id) else {
                continue;
            };
            if unit.hitpoints < unit.max_hitpoints {
                damaged_hitpoints += unit.max_hitpoints - unit.hitpoints;
                members.push(RepairMember {
                    unit_id,
                    start_hitpoints: unit.hitpoints,
                });
            }
        }
        let mut reinforcements = Vec::new();
        let mut reinforce_hitpoints = 0.0;
        let mut counted = vec![false; unit_ids.len()];
        for member in self
            .squad_repair_profile(squad_id, database)
            .map(|profile| profile.members)
            .unwrap_or_default()
        {
            let mut present = 0;
            for (index, unit_id) in unit_ids.iter().enumerate() {
                if !counted[index]
                    && self.get_unit(*unit_id).is_some_and(|unit| {
                        unit.proto_object_name
                            .eq_ignore_ascii_case(&member.prototype)
                    })
                {
                    present += 1;
                    counted[index] = true;
                }
            }
            let missing = member.count.saturating_sub(present);
            for _ in 0..missing {
                reinforcements.push(member.prototype.clone());
            }
            reinforce_hitpoints += missing.to_f32().unwrap_or(f32::MAX) * member.maximum_hitpoints;
        }
        let mut repair_percent = 1.0;
        if !reinforcements.is_empty() && damaged_hitpoints + reinforce_hitpoints > 0.0 {
            repair_percent = damaged_hitpoints / (damaged_hitpoints + reinforce_hitpoints);
        }
        SquadRepairAction {
            members,
            reinforcements,
            timer: 0.0,
            repair_time,
            damaged_time,
            repair_percent,
            reinforce_index: 0,
        }
    }

    /// `BPlayer+756`: the `RepairTime` of the player's leader.
    fn player_repair_time(&self, player_id: PlayerId, database: &Database) -> f32 {
        self.get_player(player_id)
            .and_then(|player| usize::try_from(player.leader_id).ok())
            .and_then(|index| database.leaders.get(index))
            .and_then(|leader| leader.repair_time)
            .filter(|time| time.is_finite())
            .unwrap_or(0.0)
    }

    /// Advance one self-repair action. Returns whether the action continues.
    pub(in crate::world) fn update_squad_repair(
        &mut self,
        squad_id: EntityId,
        mut action: SquadRepairAction,
        dt: f32,
        database: &Database,
    ) -> Option<SquadRepairAction> {
        let squad = self.get_squad(squad_id)?;
        if !squad.is_alive() || squad.last_damaged_time > action.damaged_time {
            return None;
        }
        action.timer += dt;
        let pct = if action.repair_time > 0.0 {
            (action.timer / action.repair_time).min(1.0)
        } else {
            1.0
        };
        if !action.members.is_empty() && action.repair_percent > 0.0 {
            let repair_pct = (pct / action.repair_percent).min(1.0);
            for member in &action.members {
                let Some(unit) = self.get_unit_mut(member.unit_id) else {
                    continue;
                };
                if unit.hitpoints < unit.max_hitpoints {
                    let damaged = unit.max_hitpoints - member.start_hitpoints;
                    unit.hitpoints =
                        (member.start_hitpoints + damaged * repair_pct).min(unit.max_hitpoints);
                }
            }
        }
        if !action.reinforcements.is_empty()
            && pct >= action.repair_percent
            && action.repair_percent < 1.0
        {
            let restore_count = (action.reinforcements.len().to_f32().unwrap_or(f32::MAX) * pct)
                .to_usize()
                .unwrap_or(0)
                .min(action.reinforcements.len());
            for prototype in
                &action.reinforcements[action.reinforce_index.min(restore_count)..restore_count]
            {
                let _unit_id = add_squad_member_from_prototype(self, squad_id, prototype, database);
            }
            action.reinforce_index = action.reinforce_index.max(restore_count);
        }
        (pct < 1.0).then_some(action)
    }
}

#[cfg(test)]
mod tests;
