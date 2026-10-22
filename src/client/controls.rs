//! Keyboard and mouse camera controls of the client.
//!
//! The bindings are the retail `Keyboard` config of `controllerconfigs.xml`. The values come from
//! `BUser_handleInput` (`0x14058F250`) and `BUser_updateCameraControls` (`0x140580220`):
//!
//! - W, A, S, D and the arrow keys scroll (`Translation`). The mouse at a window edge also
//!   scrolls (`MouseTranslation`).
//! - Insert and Delete turn the view (`PanLeft`, `PanRight`): a rotate input of +0.5 or -0.5.
//! - Alt and a mouse move turn the view (`Pan`): a rotate input of
//!   `0.2 * (1920 / window width) * dx`.
//! - Alt and the wheel zoom (`ZoomUp`, `ZoomDown`): a zoom input of +1 or -1 for each wheel
//!   step. Page Down and Page Up zoom in and out (`ZoomUpAlternate`, `ZoomDownAlternate`).
//! - Home resets the yaw, the pitch and the zoom (`ResetCamera`, `BUser_syncCameraState`).
//! - Shift is the fast modifier (`SpeedModifier`).
//!
//! The camera multiplies the rotate and zoom inputs by the frame time and the retail rates.

use glam::Vec2;
use num_traits::ToPrimitive;
use render::input::{Input, KeyCode};
use sim::PlayerPresentationState;

use super::camera::RtsCamera;
use super::mouse::cursor_position;

/// `ScrollBorderWidth`: the width of the screen edge that scrolls, in pixels.
const SCROLL_BORDER_WIDTH: f32 = 10.0;
/// `ScrollCornerRegionPercent`: the part of an edge near a corner that also scrolls along the
/// other axis, in percent of the window size.
const SCROLL_CORNER_REGION_PERCENT: f32 = 20.0;
/// The rotate input of `PanLeft` and `PanRight`.
const PAN_KEY_INPUT: f32 = 0.5;
/// The rotate input of `Pan` for each pixel of mouse move in a 1920-pixel-wide window.
const PAN_MOUSE_INPUT_PER_PIXEL: f32 = 0.2;
/// The window width that `PAN_MOUSE_INPUT_PER_PIXEL` refers to, in pixels.
const PAN_MOUSE_REFERENCE_WIDTH: f32 = 1920.0;

/// The mouse state that the camera controls keep between frames.
#[derive(Debug, Default)]
pub(crate) struct CameraControls {
    /// The cursor position of the first frame. Edge scroll starts when the cursor moves away
    /// from it, because the cursor position is (0, 0) until the window gets a cursor event.
    first_cursor: Option<Vec2>,
    /// If true, the cursor moved in the window and edge scroll is active.
    edge_scroll_armed: bool,
    /// The cursor position of the last frame.
    last_cursor: Option<Vec2>,
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
        let mouse_delta = self.last_cursor.map_or(Vec2::ZERO, |last| cursor - last);
        self.last_cursor = Some(cursor);
        if state.user_locked() {
            camera.scroll(Vec2::ZERO, false, dt_seconds);
            return;
        }
        let fast = input.is_key_held(KeyCode::LShift) || input.is_key_held(KeyCode::RShift);
        let alt = input.is_key_held(KeyCode::LAlt) || input.is_key_held(KeyCode::RAlt);
        let controls = state.camera_controls;
        let size = Vec2::new(
            window_size.0.to_f32().unwrap_or(0.0),
            window_size.1.to_f32().unwrap_or(0.0),
        );

        let mut scroll = if controls.scroll {
            key_scroll(input)
        } else {
            Vec2::ZERO
        };
        // Not decided by the engine: the client stops the edge scroll while Alt turns the view
        // with the mouse, so that the turn does not also scroll at the window edge.
        if controls.scroll && self.edge_scroll_armed && !alt {
            scroll += edge_scroll(cursor, size);
        }
        camera.scroll(scroll, fast, dt_seconds);

        if controls.yaw {
            camera.rotate(
                rotate_input(input, alt, mouse_delta.x, size.x),
                fast,
                dt_seconds,
            );
        }
        if controls.zoom {
            camera.zoom_by(zoom_input(input, alt), fast, dt_seconds);
        }
        if input.is_key_pressed(KeyCode::Home) {
            camera.reset_view(controls.yaw, controls.zoom);
        }
    }
}

/// Returns the rotate input of one frame. A positive value turns the view to the right.
///
/// Retail adds the input to the yaw, which turns the view to the left, so the sign is the
/// opposite of the retail input. Retail takes the `Pan` input of the last mouse event of the
/// frame; the client uses the cursor move of the whole frame.
fn rotate_input(input: &Input, alt: bool, mouse_dx: f32, window_width: f32) -> f32 {
    let keys = PAN_KEY_INPUT * axis(input, KeyCode::Delete, KeyCode::Insert);
    if keys != 0.0 {
        return keys;
    }
    if alt && window_width > 0.0 {
        -PAN_MOUSE_INPUT_PER_PIXEL * (PAN_MOUSE_REFERENCE_WIDTH / window_width) * mouse_dx
    } else {
        0.0
    }
}

/// Returns the zoom input of one frame. A positive value moves the eye nearer.
///
/// Without Alt the wheel is `SubSelectNext` and `SubSelectPrev`, which the client does not
/// model, so the wheel zooms only with Alt.
fn zoom_input(input: &Input, alt: bool) -> f32 {
    let wheel = if alt { input.scroll_delta } else { 0.0 };
    wheel + axis(input, KeyCode::PageDown, KeyCode::PageUp)
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
    use super::{edge_scroll, rotate_input, zoom_input};
    use glam::Vec2;
    use render::input::{Input, KeyCode};

    const SIZE: Vec2 = Vec2::new(1000.0, 500.0);

    fn keys(held: &[KeyCode]) -> Input {
        Input {
            keys_held: held.iter().copied().collect(),
            ..Input::default()
        }
    }

    #[test]
    fn insert_and_delete_turn_at_half_input_and_alt_mouse_scales_with_the_width() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // PanLeft (+0.5 retail yaw input) turns to the left: -0.5 in the client sign.
        assert!(close(
            rotate_input(&keys(&[KeyCode::Insert]), false, 0.0, 1920.0),
            -0.5
        ));
        assert!(close(
            rotate_input(&keys(&[KeyCode::Delete]), false, 0.0, 1920.0),
            0.5
        ));
        // Pan: 0.2 * (1920 / 960) * 10 pixels = 4, opposite sign.
        assert!(close(rotate_input(&keys(&[]), true, 10.0, 960.0), -4.0));
        assert!(close(rotate_input(&keys(&[]), false, 10.0, 960.0), 0.0));
    }

    #[test]
    fn the_wheel_zooms_only_with_alt_and_page_down_zooms_in() {
        let mut input = keys(&[]);
        input.scroll_delta = 2.0;
        assert!(zoom_input(&input, false).abs() < f32::EPSILON);
        assert!((zoom_input(&input, true) - 2.0).abs() < f32::EPSILON);
        assert!((zoom_input(&keys(&[KeyCode::PageDown]), false) - 1.0).abs() < f32::EPSILON);
        assert!((zoom_input(&keys(&[KeyCode::PageUp]), false) + 1.0).abs() < f32::EPSILON);
    }

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
