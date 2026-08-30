//! Synchronized attribution retained by a unit after lethal damage.

use super::Unit;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, TeamId};
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UnitDeathState {
    killer_entity_id: Option<EntityId>,
    killer_player_id: Option<PlayerId>,
    killer_team_id: Option<TeamId>,
    weapon_type: Option<String>,
}

impl UnitDeathState {
    pub(crate) fn new(
        killer_entity_id: Option<EntityId>,
        killer_player_id: Option<PlayerId>,
        killer_team_id: Option<TeamId>,
        weapon_type: Option<&str>,
    ) -> Self {
        Self {
            killer_entity_id,
            killer_player_id,
            killer_team_id,
            weapon_type: weapon_type.map(str::to_owned),
        }
    }

    pub(crate) const fn killer_entity_id(&self) -> Option<EntityId> {
        self.killer_entity_id
    }

    pub(crate) const fn killer_player_id(&self) -> Option<PlayerId> {
        self.killer_player_id
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.killer_entity_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.killer_player_id.map_or(u32::MAX, u32::from));
        checksum.hash_u32(self.killer_team_id.map_or(u32::MAX, u32::from));
        if let Some(weapon_type) = &self.weapon_type {
            checksum.hash_u32(u32::try_from(weapon_type.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(weapon_type.as_bytes());
        } else {
            checksum.hash_u32(u32::MAX);
        }
    }
}

impl Unit {
    /// Return the entity credited with this unit's death.
    #[must_use]
    pub const fn killed_by_entity_id(&self) -> Option<EntityId> {
        self.death.killer_entity_id
    }

    /// Return the player credited with this unit's death.
    #[must_use]
    pub const fn killed_by_player_id(&self) -> Option<PlayerId> {
        self.death.killer_player_id
    }

    /// Return the team captured for the credited player at death time.
    #[must_use]
    pub const fn killed_by_team_id(&self) -> Option<TeamId> {
        self.death.killer_team_id
    }

    /// Return the authored weapon type responsible for this unit's death.
    #[must_use]
    pub fn killed_by_weapon_type(&self) -> Option<&str> {
        self.death.weapon_type.as_deref()
    }

    pub(crate) fn record_death(&mut self, death: UnitDeathState) {
        self.death = death;
    }

    pub(crate) fn hash_death_state(&self, checksum: &mut SyncChecksum) {
        self.death.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn attribution_changes_the_sync_hash() {
        let empty = UnitDeathState::default();
        let credited = UnitDeathState::new(
            Some(EntityId::with_generation(EntityClass::Unit, 7, 3)),
            Some(2),
            Some(4),
            Some("Basic"),
        );

        assert_ne!(hash(&empty), hash(&credited));
    }

    fn hash(state: &UnitDeathState) -> u32 {
        let mut checksum = SyncChecksum::new();
        state.hash_state(&mut checksum);
        checksum.value()
    }
}
