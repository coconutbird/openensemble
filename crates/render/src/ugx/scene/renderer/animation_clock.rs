//! Monotonic renderer-owned time for unscripted presentation animations.

use super::super::movement_animations::MovementAnimation;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PresentationAnimationClock {
    phase: f32,
    last_frame_time: Option<f32>,
}

impl PresentationAnimationClock {
    pub(super) fn sample(&mut self, frame_time_seconds: f32, duration_seconds: f32) -> f32 {
        if !frame_time_seconds.is_finite() {
            return self.phase;
        }
        if let Some(previous) = self.last_frame_time {
            let delta = frame_time_seconds - previous;
            if delta.is_finite()
                && delta > 0.0
                && duration_seconds.is_finite()
                && duration_seconds > f32::EPSILON
            {
                self.phase += delta / duration_seconds;
                if !self.phase.is_finite() {
                    self.phase = 0.0;
                }
            }
        }
        self.last_frame_time = Some(frame_time_seconds);
        self.phase
    }

    pub(super) fn rebase_movement(
        &mut self,
        previous: Option<MovementAnimation>,
        next: Option<MovementAnimation>,
        idle_transition_complete: bool,
    ) {
        if previous == next {
            return;
        }
        let preserve_position = matches!(next, Some(MovementAnimation::WalkIdle))
            || previous.is_some_and(MovementAnimation::is_motion)
                && next.is_some_and(MovementAnimation::is_motion)
            || previous == Some(MovementAnimation::WalkIdle)
                && next.is_some_and(MovementAnimation::is_motion)
                && !idle_transition_complete;
        self.phase = if preserve_position {
            self.phase.rem_euclid(1.0)
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::ugx::scene::movement_animations::MovementAnimation;

    use super::PresentationAnimationClock;

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn starts_at_zero_and_accumulates_positive_frame_time() {
        let mut clock = PresentationAnimationClock::default();
        assert_near(clock.sample(50.0, 2.0), 0.0);
        assert_near(clock.sample(50.25, 2.0), 0.125);
        assert_near(clock.sample(51.0, 2.0), 0.5);
    }

    #[test]
    fn time_rewind_resets_the_baseline_without_reversing_the_pose() {
        let mut clock = PresentationAnimationClock::default();
        clock.sample(10.0, 1.0);
        assert_near(clock.sample(11.0, 1.0), 1.0);
        assert_near(clock.sample(2.0, 1.0), 1.0);
        assert_near(clock.sample(2.5, 1.0), 1.5);
        assert_near(clock.sample(f32::NAN, 1.0), 1.5);
    }

    #[test]
    fn clip_changes_preserve_normalized_phase() {
        let mut clock = PresentationAnimationClock::default();
        clock.sample(5.0, 2.0);
        assert_near(clock.sample(6.0, 2.0), 0.5);
        assert_near(clock.sample(7.0, 4.0), 0.75);
    }

    #[test]
    fn movement_changes_preserve_only_source_synchronized_positions() {
        let mut clock = PresentationAnimationClock::default();
        clock.sample(5.0, 1.0);
        clock.sample(7.25, 1.0);

        clock.rebase_movement(
            Some(MovementAnimation::Walk),
            Some(MovementAnimation::Jog),
            true,
        );
        assert_near(clock.sample(7.25, 1.0), 0.25);
        clock.rebase_movement(
            Some(MovementAnimation::Jog),
            Some(MovementAnimation::WalkIdle),
            false,
        );
        assert_near(clock.sample(7.25, 1.0), 0.25);
        clock.rebase_movement(
            Some(MovementAnimation::WalkIdle),
            Some(MovementAnimation::Walk),
            false,
        );
        assert_near(clock.sample(7.25, 1.0), 0.25);
    }

    #[test]
    fn completed_idle_handoffs_and_direct_stops_restart_at_zero() {
        let mut clock = PresentationAnimationClock::default();
        clock.sample(5.0, 1.0);
        clock.sample(5.75, 1.0);
        clock.rebase_movement(
            Some(MovementAnimation::WalkIdle),
            Some(MovementAnimation::Walk),
            true,
        );
        assert_near(clock.sample(5.75, 1.0), 0.0);

        clock.sample(6.25, 1.0);
        clock.rebase_movement(
            Some(MovementAnimation::Walk),
            Some(MovementAnimation::Idle),
            true,
        );
        assert_near(clock.sample(6.25, 1.0), 0.0);
    }
}
