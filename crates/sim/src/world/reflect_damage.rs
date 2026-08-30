//! Source-backed handling of persistent squad `ReflectDamage` notifications.

use super::World;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;

#[derive(Debug, Clone, Copy)]
pub(super) struct ReflectedDamage {
    pub target_id: EntityId,
    pub player_id: PlayerId,
    pub damage: f32,
}

impl World {
    pub(super) fn reflected_damage_request(
        &self,
        damaged_unit_id: EntityId,
        attacker_id: EntityId,
        incoming_damage: f32,
        gameplay: &GameplayCatalog,
    ) -> Option<ReflectedDamage> {
        self.units.get(attacker_id)?;
        let damaged = self.units.get(damaged_unit_id)?;
        let squad = self.squads.get(damaged.squad_id?)?;
        let leader_id = *squad.unit_ids.first()?;
        let leader = self.units.get(leader_id)?;
        let profile = gameplay.reflect_damage(&leader.proto_object_name)?;
        let authored_enabled = !profile.starts_disabled();
        let (player_enabled, work_rate) = self.get_player(squad.base.player_id).map_or(
            (authored_enabled, profile.work_rate()),
            |player| {
                (
                    player.technologies.action_enabled(
                        &leader.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    ),
                    player.technologies.action_work_rate(
                        &leader.proto_object_name,
                        profile.action_name(),
                        profile.work_rate(),
                    ),
                )
            },
        );
        if !leader
            .actions
            .is_enabled(profile.action_name(), !player_enabled)
        {
            return None;
        }
        let damage = retail_damage_event_payload(incoming_damage) * work_rate;
        (damage.is_finite() && damage > 0.0).then_some(ReflectedDamage {
            target_id: attacker_id,
            player_id: squad.base.player_id,
            damage,
        })
    }
}

fn retail_damage_event_payload(damage: f32) -> f32 {
    if !damage.is_finite() || damage <= 0.0 {
        return 0.0;
    }
    damage.trunc()
}

#[cfg(test)]
mod tests;
