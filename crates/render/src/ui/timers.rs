//! Selection and formatting for sim-owned retail game timers.

use sim::{GameTimer, PlayerId, World};

/// Renderer-local selection state for retail's single visible timer widget.
///
/// Timer values and lifetime remain authoritative in [`World`]. This adapter
/// remembers only which eligible timer most recently claimed the one UI slot.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SimulationTimerAdapter {
    applied_revision: u32,
    current_timer_id: Option<i32>,
}

impl SimulationTimerAdapter {
    /// Forget presentation selection when a different simulation is loaded.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Select the timer retail would expose to this local user.
    ///
    /// Newer eligible timers replace the visible widget. Destroying that timer
    /// hides it without falling back to an older still-active timer.
    pub fn synchronize<'world>(
        &mut self,
        world: &'world World,
        player_id: PlayerId,
        primary_user: bool,
    ) -> Option<&'world GameTimer> {
        let latest_revision = world.game_timer_presentation_revision();
        if latest_revision != self.applied_revision {
            if let Some(timer) = world
                .game_timers()
                .filter(|timer| timer.presentation_revision() > self.applied_revision)
                .filter(|timer| timer.audience().includes(player_id, primary_user))
                .max_by_key(|timer| timer.presentation_revision())
            {
                self.current_timer_id = Some(timer.id());
            }
            self.applied_revision = latest_revision;
        }

        let timer = self
            .current_timer_id
            .and_then(|timer_id| world.game_timer(timer_id))
            .filter(|timer| timer.audience().includes(player_id, primary_user));
        if timer.is_none() {
            self.current_timer_id = None;
        }
        timer
    }
}

/// Format milliseconds exactly as retail's timer widget: minute and second
/// fields with elapsed hours discarded.
#[must_use]
pub fn format_game_timer(time_ms: u32) -> String {
    let minutes = (time_ms / 60_000) % 60;
    let seconds = (time_ms / 1_000) % 60;
    format!("{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::GameTimerAudience;

    #[test]
    fn newest_visible_timer_replaces_the_single_widget_without_fallback() {
        let mut world = World::new();
        let first =
            world.create_game_timer(false, 60_000, 0, Some(1), GameTimerAudience::PrimaryUser);
        let mut adapter = SimulationTimerAdapter::default();
        assert_eq!(adapter.synchronize(&world, 1, true).unwrap().id(), first);

        let second =
            world.create_game_timer(false, 30_000, 0, Some(2), GameTimerAudience::PrimaryUser);
        assert_eq!(adapter.synchronize(&world, 1, true).unwrap().id(), second);
        assert!(world.destroy_game_timer(second));
        assert!(adapter.synchronize(&world, 1, true).is_none());
        assert!(world.game_timer(first).is_some());
    }

    #[test]
    fn audience_filters_replacements_for_the_local_user() {
        let mut world = World::new();
        let visible =
            world.create_game_timer(true, 0, 1_000, None, GameTimerAudience::Players(vec![2]));
        let mut hidden_adapter = SimulationTimerAdapter::default();
        assert!(hidden_adapter.synchronize(&world, 1, true).is_none());
        let mut adapter = SimulationTimerAdapter::default();
        assert_eq!(adapter.synchronize(&world, 2, false).unwrap().id(), visible);

        let _hidden =
            world.create_game_timer(true, 0, 1_000, None, GameTimerAudience::Players(vec![3]));
        assert_eq!(adapter.synchronize(&world, 2, false).unwrap().id(), visible);
    }

    #[test]
    fn timer_text_matches_retail_minute_second_wrapping() {
        assert_eq!(format_game_timer(0), "00:00");
        assert_eq!(format_game_timer(61_999), "01:01");
        assert_eq!(format_game_timer(3_661_000), "01:01");
    }
}
