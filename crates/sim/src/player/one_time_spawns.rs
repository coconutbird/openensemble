//! Per-player `OneTimeSpawnSquad` consumption state.

use super::Player;
use crate::sync::SyncChecksum;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub(super) struct OneTimeSpawnState {
    used_squad_prototypes: BTreeSet<i32>,
}

impl Player {
    pub(crate) fn has_used_one_time_spawn(&self, prototype_id: i32) -> bool {
        self.one_time_spawns
            .used_squad_prototypes
            .contains(&prototype_id)
    }

    pub(crate) fn mark_one_time_spawn_used(&mut self, prototype_id: i32) -> bool {
        prototype_id >= 0
            && self
                .one_time_spawns
                .used_squad_prototypes
                .insert(prototype_id)
    }

    pub(crate) fn hash_one_time_spawn_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            u32::try_from(self.one_time_spawns.used_squad_prototypes.len()).unwrap_or(u32::MAX),
        );
        for &prototype_id in &self.one_time_spawns.used_squad_prototypes {
            checksum.hash_i32(prototype_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_time_spawn_ids_can_only_be_claimed_once() {
        let mut player = Player::new(1);
        assert!(!player.has_used_one_time_spawn(7));
        assert!(player.mark_one_time_spawn_used(7));
        assert!(player.has_used_one_time_spawn(7));
        assert!(!player.mark_one_time_spawn_used(7));
        assert!(!player.mark_one_time_spawn_used(-1));
    }
}
