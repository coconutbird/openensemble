//! Dynamic production destinations installed by deployed mobile bases.

use super::Player;
use crate::EntityId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Default)]
pub(super) struct PlayerGotoBases {
    unit_ids: Vec<EntityId>,
}

impl Player {
    pub(crate) fn add_goto_base(&mut self, unit_id: EntityId) {
        if !self.goto_bases.unit_ids.contains(&unit_id) {
            self.goto_bases.unit_ids.push(unit_id);
        }
    }

    pub(crate) fn remove_goto_base(&mut self, unit_id: EntityId) -> bool {
        let old_len = self.goto_bases.unit_ids.len();
        self.goto_bases
            .unit_ids
            .retain(|candidate| *candidate != unit_id);
        self.goto_bases.unit_ids.len() != old_len
    }

    /// Return deployed mobile-base units in insertion order.
    #[must_use]
    pub fn goto_base_units(&self) -> &[EntityId] {
        &self.goto_bases.unit_ids
    }

    pub(crate) fn hash_goto_base_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.goto_bases.unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.goto_bases.unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityClass;

    #[test]
    fn deployed_base_membership_is_unique_and_ordered() {
        let mut player = Player::new(1);
        let first = EntityId::new(EntityClass::Unit, 3);
        let second = EntityId::new(EntityClass::Unit, 1);

        player.add_goto_base(first);
        player.add_goto_base(second);
        player.add_goto_base(first);
        assert_eq!(player.goto_base_units(), &[first, second]);

        player.remove_goto_base(first);
        assert_eq!(player.goto_base_units(), &[second]);
    }
}
