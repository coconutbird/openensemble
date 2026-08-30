//! Monotonic renderer-owned time for unscripted presentation animations.

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
}

#[cfg(test)]
mod tests {
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
}
