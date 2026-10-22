//! Retail per-player squad-AI scheduling state.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

use super::Player;

#[derive(Debug, Clone, Default)]
pub(super) struct PlayerSquadAiState {
    squads: Vec<EntityId>,
    secondary_turret_scan_index: usize,
}

impl Player {
    pub(crate) fn add_squad_ai_squad(&mut self, squad_id: EntityId) {
        if !self.squad_ai.squads.contains(&squad_id) {
            self.squad_ai.squads.push(squad_id);
        }
    }

    pub(crate) fn remove_squad_ai_squad(&mut self, squad_id: EntityId) {
        let Some(index) = self
            .squad_ai
            .squads
            .iter()
            .position(|candidate| *candidate == squad_id)
        else {
            return;
        };
        self.squad_ai.squads.remove(index);
        if index < self.squad_ai.secondary_turret_scan_index {
            self.squad_ai.secondary_turret_scan_index -= 1;
        }
    }

    pub(crate) fn next_secondary_turret_scan_squad(&mut self) -> Option<EntityId> {
        let count = self.squad_ai.squads.len();
        if count == 0 {
            self.squad_ai.secondary_turret_scan_index = 0;
            return None;
        }
        if count == 1 {
            return self.squad_ai.squads.first().copied();
        }
        if self.squad_ai.secondary_turret_scan_index >= count {
            self.squad_ai.secondary_turret_scan_index = 0;
        }
        let squad_id = self.squad_ai.squads[self.squad_ai.secondary_turret_scan_index];
        self.squad_ai.secondary_turret_scan_index += 1;
        if self.squad_ai.secondary_turret_scan_index >= count {
            self.squad_ai.secondary_turret_scan_index = 0;
        }
        Some(squad_id)
    }

    pub(crate) fn hash_squad_ai_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.squad_ai.squads.len()).unwrap_or(u32::MAX));
        for squad_id in &self.squad_ai.squads {
            checksum.hash_u32(squad_id.as_u32());
        }
        checksum
            .hash_u32(u32::try_from(self.squad_ai.secondary_turret_scan_index).unwrap_or(u32::MAX));
    }
}

#[cfg(test)]
mod tests {
    use crate::entity_id::{EntityClass, EntityId};

    use super::*;

    fn squad(index: u16) -> EntityId {
        EntityId::with_generation(EntityClass::Squad, index, 0)
    }

    #[test]
    fn one_squad_receives_a_token_every_update() {
        let mut player = Player::new(1);
        player.add_squad_ai_squad(squad(4));

        assert_eq!(player.next_secondary_turret_scan_squad(), Some(squad(4)));
        assert_eq!(player.next_secondary_turret_scan_squad(), Some(squad(4)));
    }

    #[test]
    fn multiple_squads_rotate_in_insertion_order() {
        let mut player = Player::new(1);
        for index in [7, 2, 9] {
            player.add_squad_ai_squad(squad(index));
        }

        let grants = (0..5)
            .map(|_| player.next_secondary_turret_scan_squad().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(grants, [squad(7), squad(2), squad(9), squad(7), squad(2)]);
    }

    #[test]
    fn removal_before_the_cursor_preserves_the_next_squad() {
        let mut player = Player::new(1);
        for index in [7, 2, 9] {
            player.add_squad_ai_squad(squad(index));
        }
        assert_eq!(player.next_secondary_turret_scan_squad(), Some(squad(7)));
        assert_eq!(player.next_secondary_turret_scan_squad(), Some(squad(2)));

        player.remove_squad_ai_squad(squad(7));

        assert_eq!(player.next_secondary_turret_scan_squad(), Some(squad(9)));
    }
}
