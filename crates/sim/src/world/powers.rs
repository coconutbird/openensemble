//! Database-backed player power ownership transitions.

use super::World;
use crate::EntityId;
use crate::player::{PlayerId, PowerGrant, PowerRules, ProtoPowerId};
use pipeline::database::hw1::{Database, Power};

/// Resolve a scenario-layered power name to its retail runtime table index.
#[must_use]
pub fn power_prototype_id(database: &Database, name: &str) -> Option<ProtoPowerId> {
    name.trim()
        .parse()
        .ok()
        .filter(|id| power_by_id(database, *id).is_some())
        .or_else(|| {
            database
                .powers
                .iter()
                .position(|power| power.name.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|index| i32::try_from(index).ok())
        })
}

impl World {
    pub(crate) fn grant_player_power(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        mut grant: PowerGrant,
    ) -> bool {
        let Some(power) = power_by_id(database, grant.proto_power_id) else {
            return false;
        };
        if !grant.squad_id.is_invalid() && self.get_squad(grant.squad_id).is_none() {
            grant.squad_id = EntityId::INVALID;
        }
        let rules = rules(power);
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.grant_power(grant, rules, |power_id, icon_location| {
            power_by_id(database, power_id)
                .and_then(|power| power.attributes.as_ref())
                .is_some_and(|attributes| attributes.icon_locations.contains(&icon_location))
        });
        true
    }

    pub(crate) fn revoke_player_power(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        proto_power_id: ProtoPowerId,
        mut squad_id: EntityId,
    ) -> bool {
        let Some(power) = power_by_id(database, proto_power_id) else {
            return false;
        };
        if !squad_id.is_invalid() && self.get_squad(squad_id).is_none() {
            squad_id = EntityId::INVALID;
        }
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.revoke_power(proto_power_id, squad_id, rules(power));
        true
    }
}

fn power_by_id(database: &Database, proto_power_id: ProtoPowerId) -> Option<&Power> {
    usize::try_from(proto_power_id)
        .ok()
        .and_then(|index| database.powers.get(index))
}

fn rules(power: &Power) -> PowerRules {
    let attributes = power.attributes.as_ref();
    PowerRules {
        infinite_uses: attributes
            .and_then(|value| value.infinite_uses)
            .unwrap_or(false),
        multi_recharge: attributes
            .and_then(|value| value.multi_recharge_power)
            .unwrap_or(false),
        sequential_recharge: attributes
            .and_then(|value| value.sequential_recharge)
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests;
