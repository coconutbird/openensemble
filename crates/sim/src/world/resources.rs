//! Authoritative per-player resource updates.

use super::World;

impl World {
    /// Apply each playing player's configured per-second resource trickle.
    pub fn update_player_resources(&mut self, elapsed_seconds: f32) {
        for player in &mut self.players {
            player.update_resource_trickle(elapsed_seconds);
        }
    }
}
