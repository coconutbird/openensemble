//! Projection of sim-owned full-screen fade state into renderer color data.

use sim::World;

/// Return the current authoritative fade overlay as unmultiplied RGBA bytes.
#[must_use]
pub fn screen_fade_rgba(world: &World) -> Option<[u8; 4]> {
    world
        .screen_fade_overlay()
        .map(sim::ScreenFadeOverlay::rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::ScreenFadeSequence;

    #[test]
    fn projection_contains_no_independent_animation_state() {
        let mut world = World::new();
        world.start_screen_fade(
            [10, 20, 30],
            ScreenFadeSequence::ToColor {
                duration_ms: 100,
                fade_in: false,
            },
        );
        assert_eq!(screen_fade_rgba(&world), Some([10, 20, 30, 0]));
        world.advance_time(50);
        assert_eq!(screen_fade_rgba(&world), Some([10, 20, 30, 127]));
        world.advance_time(50);
        assert_eq!(screen_fade_rgba(&world), None);
    }
}
