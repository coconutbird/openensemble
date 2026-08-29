//! Authoritative objective-arrow state authored by retail triggers.

use std::collections::BTreeMap;

use glam::Vec3;

use super::super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// One visible objective pointer for a player and retail UI widget slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectivePointer {
    widget_id: i32,
    target_position: Vec3,
    use_target: bool,
    force_target_visible: bool,
}

impl ObjectivePointer {
    /// Retail UI widget receiving the arrow state.
    #[must_use]
    pub const fn widget_id(self) -> i32 {
        self.widget_id
    }

    /// World-space target resolved by the simulation when the trigger fires.
    #[must_use]
    pub const fn target_position(self) -> Vec3 {
        self.target_position
    }

    /// Whether retail directs the arrow toward the authored target.
    #[must_use]
    pub const fn use_target(self) -> bool {
        self.use_target
    }

    /// Whether the target remains presentable through normal visibility policy.
    #[must_use]
    pub const fn force_target_visible(self) -> bool {
        self.force_target_visible
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct ObjectivePointerState {
    players: BTreeMap<PlayerId, BTreeMap<i32, ObjectivePointer>>,
}

impl World {
    /// Iterate one player's visible objective pointers in stable widget order.
    pub fn objective_pointers(
        &self,
        player_id: PlayerId,
    ) -> impl Iterator<Item = &ObjectivePointer> {
        self.presentation_control
            .objective_pointers
            .players
            .get(&player_id)
            .into_iter()
            .flat_map(BTreeMap::values)
    }

    /// Look up one visible objective pointer by player and widget ID.
    #[must_use]
    pub fn objective_pointer(
        &self,
        player_id: PlayerId,
        widget_id: i32,
    ) -> Option<&ObjectivePointer> {
        self.presentation_control
            .objective_pointers
            .players
            .get(&player_id)?
            .get(&widget_id)
    }

    pub(crate) fn show_objective_pointer(
        &mut self,
        player_id: PlayerId,
        widget_id: i32,
        target_position: Vec3,
        use_target: bool,
        force_target_visible: bool,
    ) {
        if player_id == 0 || self.get_player(player_id).is_none() || !target_position.is_finite() {
            return;
        }
        self.presentation_control
            .objective_pointers
            .players
            .entry(player_id)
            .or_default()
            .insert(
                widget_id,
                ObjectivePointer {
                    widget_id,
                    target_position,
                    use_target,
                    force_target_visible,
                },
            );
    }

    pub(crate) fn hide_objective_pointer(&mut self, player_id: PlayerId, widget_id: i32) {
        let state = &mut self.presentation_control.objective_pointers.players;
        let Some(pointers) = state.get_mut(&player_id) else {
            return;
        };
        pointers.remove(&widget_id);
        if pointers.is_empty() {
            state.remove(&player_id);
        }
    }
}

impl ObjectivePointerState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, pointers) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            checksum.hash_u32(u32::try_from(pointers.len()).unwrap_or(u32::MAX));
            for pointer in pointers.values().copied() {
                checksum.hash_i32(pointer.widget_id);
                checksum.hash_vec3(
                    pointer.target_position.x,
                    pointer.target_position.y,
                    pointer.target_position.z,
                );
                checksum.hash_u32(u32::from(pointer.use_target));
                checksum.hash_u32(u32::from(pointer.force_target_visible));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_pointers_are_player_scoped_checksummed_and_removable() {
        let mut world = World::new();
        world.init_players(2);
        let baseline = world.checksum();
        let target = Vec3::new(10.0, 2.0, 30.0);

        world.show_objective_pointer(1, 4, target, true, false);
        let pointer = world.objective_pointer(1, 4).copied().unwrap();
        assert_eq!(pointer.target_position(), target);
        assert!(pointer.use_target());
        assert!(!pointer.force_target_visible());
        assert!(world.objective_pointer(2, 4).is_none());
        assert_ne!(world.checksum(), baseline);

        world.hide_objective_pointer(1, 4);
        assert!(world.objective_pointers(1).next().is_none());
        assert_eq!(world.checksum(), baseline);
    }
}
