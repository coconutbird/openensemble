//! Retail squad damage-proxy resolution.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use std::collections::BTreeSet;

mod bubble;
mod plasma;

impl World {
    /// Resolve the unit that ultimately receives damage requested for `target_id`.
    ///
    /// Retail redirects through the protected unit's parent squad to child zero
    /// of a live proxy squad. `BUnit::damage` repeats that operation, so a valid
    /// chain is followed here while malformed cycles terminate deterministically.
    pub(in crate::world) fn resolve_damage_target(&self, target_id: EntityId) -> EntityId {
        let mut resolved = target_id;
        let mut visited = BTreeSet::new();
        while visited.insert(resolved) {
            let Some(proxy_child) = self.immediate_damage_proxy_child(resolved) else {
                break;
            };
            resolved = proxy_child;
        }
        resolved
    }

    fn immediate_damage_proxy_child(&self, unit_id: EntityId) -> Option<EntityId> {
        let protected_squad_id = self.units.get(unit_id)?.squad_id?;
        let proxy_squad_id = self.squads.get(protected_squad_id)?.damage_proxy()?;
        let proxy_squad = self
            .squads
            .get(proxy_squad_id)
            .filter(|squad| squad.is_alive())?;
        proxy_squad.unit_ids.first().copied()
    }
}

#[cfg(test)]
mod tests;
