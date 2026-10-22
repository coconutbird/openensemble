//! Keyboard and mouse camera controls of the client.
//!
//! - W, A, S, D and the arrow keys scroll. The mouse at a window edge also scrolls.
//! - Q and E turn the view. A drag with the middle mouse button also turns the view.
//! - Page Up and Page Down zoom.
//! - Shift is the fast modifier.

use glam::Vec2;
use num_traits::ToPrimitive;
use render::input::{Input, KeyCode};
use sim::PlayerPresentationState;

use super::camera::RtsCamera;

/// `ScrollBorderWidth`: the width of the screen edge that scrolls, in pixels.
const SCROLL_BORDER_WIDTH: f32 = 10.0;
/// `ScrollCornerRegionPercent`: the part of an edge near a corner that also scrolls along the
/// other axis, in percent of the window size.
const SCROLL_CORNER_REGION_PERCENT: f32 = 20.0;
/// The view turn for each pixel of a middle mouse button drag, in degrees.
///
/// This value is not a retail value.
const DRAG_ROTATE_DEGREES_PER_PIXEL: f32 = 0.25;

/// Index of the middle button in `Input::mouse_buttons`.
const MIDDLE_MOUSE_BUTTON: usize = 1;

/// The mouse state that the camera controls keep between frames.
#[derive(Debug, Default)]
pub(crate) struct CameraControls {
    /// The cursor position of the first frame. Edge scroll starts when the cursor moves away
    /// from it, because the cursor position is (0, 0) until the window gets a cursor event.
    first_cursor: Option<Vec2>,
    /// If true, the cursor moved in the window and edge scroll is active.
    edge_scroll_armed: bool,
    /// The cursor position of the last frame with the middle mouse button down.
    drag_cursor: Option<Vec2>,
}

impl CameraControls {
    /// Applies the input of one frame to `camera`. `state` gives the permissions of the
    /// simulation.
    pub(crate) fn update(
        &mut self,
        camera: &mut RtsCamera,
        input: &Input,
        window_size: (u32, u32),
        state: PlayerPresentationState,
        dt_seconds: f32,
    ) {
        let cursor = cursor_position(input);
        let first_cursor = *self.first_cursor.get_or_insert(cursor);
        self.edge_scroll_armed |= cursor != first_cursor;
        let drag_delta = if input.mouse_buttons[MIDDLE_MOUSE_BUTTON] {
            let delta = self.drag_cursor.map_or(Vec2::ZERO, |last| cursor - last);
            self.drag_cursor = Some(cursor);
            delta
        } else {
            self.drag_cursor = None;
            Vec2::ZERO
        };
        if state.user_locked() {
            camera.scroll(Vec2::ZERO, false, dt_seconds);
            return;
        }
        let fast = input.is_key_held(KeyCode::LShift) || input.is_key_held(KeyCode::RShift);
        let controls = state.camera_controls;

        let mut scroll = if controls.scroll {
            key_scroll(input)
        } else {
            Vec2::ZERO
        };
        if controls.scroll && self.edge_scroll_armed && self.drag_cursor.is_none() {
            let size = Vec2::new(
                window_size.0.to_f32().unwrap_or(0.0),
                window_size.1.to_f32().unwrap_or(0.0),
            );
            scroll += edge_scroll(cursor, size);
        }
        camera.scroll(scroll, fast, dt_seconds);

        if controls.yaw {
            let keys = axis(input, KeyCode::E, KeyCode::Q);
            camera.rotate(keys, fast, dt_seconds);
            camera.rotate_degrees(drag_delta.x * DRAG_ROTATE_DEGREES_PER_PIXEL);
        }
        if controls.zoom {
            // Wave B hook: add the mouse wheel here when `render::input::Input` has a wheel
            // value (`Input::scroll_delta`, positive = wheel up = zoom in).
            let keys = axis(input, KeyCode::PageUp, KeyCode::PageDown);
            camera.zoom_by(keys, fast, dt_seconds);
        }
    }
}

/// Returns the cursor position in window pixels.
fn cursor_position(input: &Input) -> Vec2 {
    Vec2::new(
        input.mouse_position.0.to_f32().unwrap_or(0.0),
        input.mouse_position.1.to_f32().unwrap_or(0.0),
    )
}

/// Returns 1 if `positive` is down, -1 if `negative` is down, and 0 for both or none.
fn axis(input: &Input, positive: KeyCode, negative: KeyCode) -> f32 {
    let mut value = 0.0;
    if input.is_key_held(positive) {
        value += 1.0;
    }
    if input.is_key_held(negative) {
        value -= 1.0;
    }
    value
}

/// Returns the key scroll direction: x goes to the right and y goes forward.
fn key_scroll(input: &Input) -> Vec2 {
    let right = axis(input, KeyCode::D, KeyCode::A) + axis(input, KeyCode::Right, KeyCode::Left);
    let forward = axis(input, KeyCode::W, KeyCode::S) + axis(input, KeyCode::Up, KeyCode::Down);
    Vec2::new(right.clamp(-1.0, 1.0), forward.clamp(-1.0, 1.0))
}

/// Returns the edge scroll direction for `cursor` in a window of `size` pixels.
///
/// The result x goes to the right and y goes forward (to the top of the screen). The rules come
/// from `BUser_handleInput` (`0x14058F250`):
///
/// - A cursor in the `ScrollBorderWidth` border of an edge scrolls away from the window.
/// - A cursor on a left or right edge that is also in the `ScrollCornerRegionPercent` part of
///   the window height near the top or bottom also scrolls forward or back.
/// - A cursor on a top or bottom edge that is also in the corner part of the window width near
///   the left or right also scrolls to the side.
pub(crate) fn edge_scroll(cursor: Vec2, size: Vec2) -> Vec2 {
    let border = SCROLL_BORDER_WIDTH;
    let corner = size * (SCROLL_CORNER_REGION_PERCENT / 100.0);
    let horizontal = edge_axis(cursor.x, size.x, border);
    let vertical = edge_axis(cursor.y, size.y, border);
    let (x, y) = match (horizontal, vertical) {
        (0, 0) => (0, 0),
        (x, 0) => (x, edge_axis(cursor.y, size.y, corner.y)),
        (0, y) => (edge_axis(cursor.x, size.x, corner.x), y),
        (x, y) => (x, y),
    };
    // The screen y axis goes down, but forward goes to the top of the screen.
    Vec2::new(f32::from(x), -f32::from(y))
}

/// Returns -1 if `value` is at most `band` from 0, 1 if it is at least `band` from `size`, and 0
/// in the middle.
fn edge_axis(value: f32, size: f32, band: f32) -> i8 {
    if value <= band {
        -1
    } else {
        i8::from(value >= size - band)
    }
}

#[cfg(test)]
mod tests {
    use super::edge_scroll;
    use glam::Vec2;

    const SIZE: Vec2 = Vec2::new(1000.0, 500.0);

    #[test]
    fn the_middle_of_the_window_does_not_scroll() {
        assert_eq!(edge_scroll(Vec2::new(500.0, 250.0), SIZE), Vec2::ZERO);
        assert_eq!(edge_scroll(Vec2::new(11.0, 489.0), SIZE), Vec2::ZERO);
    }

    #[test]
    fn each_edge_border_scrolls_away_from_the_window() {
        assert_eq!(
            edge_scroll(Vec2::new(10.0, 250.0), SIZE),
            Vec2::new(-1.0, 0.0)
        );
        assert_eq!(
            edge_scroll(Vec2::new(990.0, 250.0), SIZE),
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(
            edge_scroll(Vec2::new(500.0, 0.0), SIZE),
            Vec2::new(0.0, 1.0)
        );
        assert_eq!(
            edge_scroll(Vec2::new(500.0, 495.0), SIZE),
            Vec2::new(0.0, -1.0)
        );
    }

    #[test]
    fn the_corner_region_of_an_edge_scrolls_diagonally() {
        // Left edge, in the top 20 percent of the height.
        assert_eq!(
            edge_scroll(Vec2::new(0.0, 100.0), SIZE),
            Vec2::new(-1.0, 1.0)
        );
        // Bottom edge, in the right 20 percent of the width.
        assert_eq!(
            edge_scroll(Vec2::new(800.0, 499.0), SIZE),
            Vec2::new(1.0, -1.0)
        );
        // Top edge, outside the corner region.
        assert_eq!(
            edge_scroll(Vec2::new(201.0, 0.0), SIZE),
            Vec2::new(0.0, 1.0)
        );
    }
}
