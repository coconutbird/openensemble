//! Authoritative world-clock access and dependent state advancement.

use super::World;

impl World {
    /// Get current game time in milliseconds.
    #[must_use]
    pub fn game_time(&self) -> u32 {
        self.game_time_ms
    }

    /// Advance game time and all state derived directly from the world clock.
    pub fn advance_time(&mut self, milliseconds: u32) {
        self.game_time_ms = self.game_time_ms.wrapping_add(milliseconds);
        self.update_game_timers();
        self.update_camera_shakes();
        self.update_rumbles();
        self.update_screen_fade();
    }
}
