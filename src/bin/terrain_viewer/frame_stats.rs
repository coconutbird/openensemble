//! Frame timing statistics for the terrain viewer.
//!
//! The viewer records the frame time, the CPU time of the simulation update and the CPU time of
//! the render encode. Each second it makes a report for the information window. A report with a
//! slow average frame goes to the log as a warning, and every report goes to the debug log.

use std::time::Duration;

use num_traits::ToPrimitive;

/// Length of one report window, in seconds.
const REPORT_WINDOW_SECONDS: f32 = 1.0;

/// Average frame time above which a report goes to the log as a warning, in milliseconds.
const SLOW_FRAME_MS: f32 = 33.4;

/// Timings of one report window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameReport {
    /// Frames per second.
    pub(crate) fps: f32,
    /// Average frame time, in milliseconds.
    pub(crate) frame_ms: f32,
    /// Longest frame time, in milliseconds.
    pub(crate) max_frame_ms: f32,
    /// Average CPU time of the simulation update for each frame, in milliseconds.
    pub(crate) simulation_ms: f32,
    /// Average CPU time of the render encode for each frame, in milliseconds.
    pub(crate) render_ms: f32,
}

/// Rolling frame, simulation and render timings.
#[derive(Debug, Default)]
pub(crate) struct FrameStats {
    window_seconds: f32,
    frames: u32,
    max_frame_seconds: f32,
    simulation: Duration,
    render: Duration,
    last: Option<FrameReport>,
}

impl FrameStats {
    /// Records the time of one frame. Returns a new report when the window is full.
    pub(crate) fn record_frame(&mut self, delta_seconds: f32) -> Option<FrameReport> {
        self.window_seconds += delta_seconds;
        self.frames += 1;
        self.max_frame_seconds = self.max_frame_seconds.max(delta_seconds);
        if self.window_seconds < REPORT_WINDOW_SECONDS {
            return None;
        }
        let frames = self.frames.to_f32().unwrap_or(f32::MAX);
        let report = FrameReport {
            fps: frames / self.window_seconds,
            frame_ms: self.window_seconds * 1_000.0 / frames,
            max_frame_ms: self.max_frame_seconds * 1_000.0,
            simulation_ms: self.simulation.as_secs_f32() * 1_000.0 / frames,
            render_ms: self.render.as_secs_f32() * 1_000.0 / frames,
        };
        *self = Self {
            last: Some(report),
            ..Self::default()
        };
        if report.frame_ms > SLOW_FRAME_MS {
            log::warn!("Slow frames: {}", report.summary());
        } else {
            log::debug!("Frames: {}", report.summary());
        }
        Some(report)
    }

    /// Adds the CPU time of a simulation update to the current window.
    pub(crate) fn record_simulation(&mut self, duration: Duration) {
        self.simulation += duration;
    }

    /// Adds the CPU time of a render encode to the current window.
    pub(crate) fn record_render(&mut self, duration: Duration) {
        self.render += duration;
    }

    /// Returns the report of the last full window.
    pub(crate) fn last(&self) -> Option<FrameReport> {
        self.last
    }
}

impl FrameReport {
    /// Returns the report as one line of text.
    pub(crate) fn summary(&self) -> String {
        format!(
            "{:.0} FPS, frame {:.1} ms (max {:.1} ms), simulation {:.2} ms, render encode {:.2} ms",
            self.fps, self.frame_ms, self.max_frame_ms, self.simulation_ms, self.render_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FrameStats;
    use std::time::Duration;

    #[test]
    fn a_report_averages_the_window_and_starts_a_new_one() {
        let mut stats = FrameStats::default();
        assert_eq!(stats.record_frame(0.25), None);
        stats.record_simulation(Duration::from_millis(4));
        stats.record_render(Duration::from_millis(8));
        assert_eq!(stats.record_frame(0.25), None);
        assert_eq!(stats.record_frame(0.25), None);
        let report = stats.record_frame(0.25).expect("the window is full");

        assert!((report.fps - 4.0).abs() < 1e-4);
        assert!((report.frame_ms - 250.0).abs() < 1e-3);
        assert!((report.max_frame_ms - 250.0).abs() < 1e-3);
        assert!((report.simulation_ms - 1.0).abs() < 1e-4);
        assert!((report.render_ms - 2.0).abs() < 1e-4);
        assert_eq!(stats.last(), Some(report));
        assert_eq!(stats.record_frame(0.1), None);
        assert_eq!(stats.last(), Some(report));
    }
}
