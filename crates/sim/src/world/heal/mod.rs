//! Persistent retail Medic/Monitor hitpoint healing.

use super::World;
use crate::entities::{HealPhase, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, HealActionProfile};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct HealUpdateContext {
    profile: HealActionProfile,
    target_squad_id: Option<EntityId>,
    enabled: bool,
    work_rate: f32,
}

impl World {
    pub(in crate::world) fn update_heals(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let unit_ids = self.units.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.update_unit_heal(unit_id, dt, database, gameplay);
        }
    }

    fn update_unit_heal(
        &mut self,
        unit_id: EntityId,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(context) = self.heal_update_context(unit_id, gameplay) else {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.heal.reset();
            }
            return;
        };
        let phase = {
            let Some(unit) = self.units.get_mut(unit_id) else {
                return;
            };
            unit.heal.connect(context.profile.action_name());
            unit.heal.set_target(context.target_squad_id);
            unit.heal.phase()
        };
        if !context.enabled {
            return;
        }
        let Some(target_squad_id) = context.target_squad_id else {
            self.wait_for_heal_target(unit_id, None);
            return;
        };
        let ready = self.time_to_heal(target_squad_id, context.profile.min_idle_duration_ms())
            && self.squad_needs_healing(
                target_squad_id,
                database,
                context.profile.allow_reinforce(),
            );
        match phase {
            HealPhase::None => {}
            HealPhase::Waiting if ready => {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.heal.begin_work(target_squad_id);
                }
            }
            HealPhase::Working if ready => {
                let requested = finite_nonnegative(context.work_rate * dt);
                let _excess = self.repair_squad_by_hitpoints(
                    database,
                    target_squad_id,
                    requested,
                    context.profile.allow_reinforce(),
                );
            }
            HealPhase::Waiting | HealPhase::Working => {
                self.wait_for_heal_target(unit_id, Some(target_squad_id));
            }
        }
    }

    fn heal_update_context(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<HealUpdateContext> {
        let unit = self.units.get(unit_id).filter(|unit| unit.is_alive())?;
        let profile = gameplay.heal(&unit.proto_object_name)?.clone();
        let target_squad_id = self.heal_target_squad(unit, profile.heal_target());
        let authored_enabled = !profile.starts_disabled();
        let (player_enabled, work_rate) = self.get_player(unit.base.player_id).map_or(
            (authored_enabled, profile.work_rate()),
            |player| {
                (
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    ),
                    player.technologies.action_work_rate(
                        &unit.proto_object_name,
                        profile.action_name(),
                        profile.work_rate(),
                    ),
                )
            },
        );
        Some(HealUpdateContext {
            enabled: unit
                .actions
                .is_enabled(profile.action_name(), !player_enabled),
            profile,
            target_squad_id,
            work_rate: finite_nonnegative(work_rate),
        })
    }

    fn heal_target_squad(&self, unit: &Unit, heal_target: bool) -> Option<EntityId> {
        let parent_id = unit.squad_id?;
        if !heal_target {
            return self.squads.get(parent_id).map(|_| parent_id);
        }
        self.squads
            .get(parent_id)
            .and_then(crate::entities::Squad::join_target)
            .filter(|target_id| self.squads.get(*target_id).is_some())
            .or_else(|| self.squads.get(parent_id).map(|_| parent_id))
    }

    fn time_to_heal(&self, squad_id: EntityId, minimum_ms: u32) -> bool {
        self.squads.get(squad_id).is_some_and(|squad| {
            squad.idle_duration() >= minimum_ms
                && self.game_time_ms.wrapping_sub(squad.last_damaged_time) >= minimum_ms
                && self.game_time_ms.wrapping_sub(squad.last_attacked_time) >= minimum_ms
        })
    }

    fn squad_needs_healing(
        &self,
        squad_id: EntityId,
        database: &Database,
        allow_reinforce: bool,
    ) -> bool {
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        let damaged = squad.unit_ids.iter().any(|unit_id| {
            self.units
                .get(*unit_id)
                .is_some_and(|unit| unit.hitpoints < unit.max_hitpoints)
        });
        damaged || (allow_reinforce && self.squad_is_missing_authored_members(squad_id, database))
    }

    fn wait_for_heal_target(&mut self, unit_id: EntityId, target_squad_id: Option<EntityId>) {
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.heal.wait_for(target_squad_id);
        }
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
