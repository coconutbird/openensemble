//! Scenario-layered capture-cost resolution.

use super::World;
use crate::player::{MAX_RESOURCES, PlayerId, Resources};
use pipeline::database::hw1::{Database, ProtoObject};

impl World {
    pub(super) fn capture_cost(
        &self,
        database: &Database,
        player_id: PlayerId,
        target: &ProtoObject,
    ) -> Option<Resources> {
        let player = self.get_player(player_id)?;
        let civilization = usize::try_from(player.civ_id)
            .ok()
            .and_then(|index| database.civs.get(index))
            .map(|civ| civ.name.as_str());
        let mut total = Resources::new();
        let applicable = target
            .capture_costs
            .iter()
            .filter(|entry| capture_cost_applies(entry.civilization.as_deref(), civilization));
        if applicable.clone().next().is_none() {
            return Some(total);
        }
        let resources = database.game_data.as_ref()?.resources.as_ref()?;
        for entry in applicable {
            if !entry.amount.is_finite() || entry.amount < 0.0 {
                return None;
            }
            let resource_id =
                resources
                    .entries
                    .iter()
                    .take(MAX_RESOURCES)
                    .position(|resource| {
                        resource
                            .name
                            .trim()
                            .eq_ignore_ascii_case(entry.resource_type.trim())
                    })?;
            let amount = total.get(resource_id) + entry.amount;
            if !amount.is_finite() {
                return None;
            }
            total.set(resource_id, amount);
        }
        Some(total)
    }
}

pub(super) fn target_prototype<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name))
}

fn capture_cost_applies(authored: Option<&str>, civilization: Option<&str>) -> bool {
    authored.map(str::trim).is_none_or(|authored| {
        authored.is_empty()
            || civilization.is_some_and(|name| name.trim().eq_ignore_ascii_case(authored))
    })
}
