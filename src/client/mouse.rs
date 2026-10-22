//! The mouse and key bindings of the selection, like `BUser_handleInputGame` (`0x140590320`).
//!
//! Retail `controllerconfigs.xml` (config `Keyboard`):
//!
//! - Left click (`Selection`): on release, a drag of more than 1 pixel on both axes selects
//!   with the box (and stops if the selection is not empty); else `BUser_selectAtCursor` with
//!   replace.
//! - Left hold (`MultiSelect`) and Shift + left hold (`MultiSelectAdd`): the drag box selects
//!   each frame. The Shift drag keeps the selection of the drag start.
//! - Shift + left click (`AddToSelection`): on press, toggles the hovered squad.
//! - Double click (`DoubleClickSelect`), Shift + double click (`DoubleClickAdd`) and Ctrl +
//!   left click (`SelectAllType`): `BUser_doubleClickSelect`. The release after it does not
//!   click.
//! - Escape (`Clear`), Q (`ScreenSelect`) and E (`GlobalSelect`).

use glam::Vec2;
use num_traits::ToPrimitive;
use render::input::{Input, KeyCode, MouseButton};
use sim::EntityId;

use super::hover::Hover;
use super::select::{
    ClickCycle, DRAG_SELECT_MIN_SIZE, SelectContext, double_click_select, drag_select, select_all,
    select_at_cursor, toggle_select_hovered,
};
use super::selection::Selection;

/// The longest time between the presses of a double click, in seconds.
///
/// The engine takes the double click from the input system. This value is the Windows default
/// double-click time.
const DOUBLE_CLICK_SECONDS: f32 = 0.5;
/// The largest cursor move between the presses of a double click, in pixels.
///
/// This value is the Windows default double-click rectangle (4 pixels).
const DOUBLE_CLICK_DISTANCE: f32 = 4.0;
/// A release drag selects when both sides are more than this, in pixels.
const RELEASE_DRAG_MIN: f32 = 1.0;

/// The edges of one mouse button in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ButtonEdges {
    pub(crate) pressed: bool,
    pub(crate) held: bool,
    pub(crate) released: bool,
}

/// The pointer and modifier state of one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PointerFrame {
    /// The cursor position, in physical pixels.
    pub(crate) cursor: Vec2,
    pub(crate) shift: bool,
    pub(crate) ctrl: bool,
    /// The left button.
    pub(crate) left: ButtonEdges,
    /// The render time of the frame, in seconds.
    pub(crate) time_seconds: f32,
}

impl PointerFrame {
    /// Reads the pointer state of `input`.
    pub(crate) fn from_input(input: &Input, time_seconds: f32) -> Self {
        Self {
            cursor: cursor_position(input),
            shift: input.is_key_held(KeyCode::LShift) || input.is_key_held(KeyCode::RShift),
            ctrl: input.is_key_held(KeyCode::LCtrl) || input.is_key_held(KeyCode::RCtrl),
            left: ButtonEdges {
                pressed: input.is_mouse_pressed(MouseButton::Left),
                held: input.is_mouse_held(MouseButton::Left),
                released: input.is_mouse_released(MouseButton::Left),
            },
            time_seconds,
        }
    }
}

/// Returns the cursor position in physical pixels.
pub(crate) fn cursor_position(input: &Input) -> Vec2 {
    Vec2::new(
        input.mouse_position.0.to_f32().unwrap_or(0.0),
        input.mouse_position.1.to_f32().unwrap_or(0.0),
    )
}

/// The selection state that the mouse keeps between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct MouseSelect {
    /// The cursor at the left press (`user+5744`), while the button is down.
    press: Option<Vec2>,
    /// If true, a drag box is active (`user+5752`).
    dragging: bool,
    /// If true, the press had Shift: a release does not click (`AddToSelection`).
    shift_press: bool,
    /// The selected units at the start of a Shift drag (`sel+16`).
    saved: Vec<EntityId>,
    /// If true, a double click or a Ctrl click used the press (`user+11976 & 0x80`).
    skip_release: bool,
    /// The time and the cursor of the last left press, for the double click.
    last_press: Option<(f32, Vec2)>,
    /// The overlapping-squad cycle of repeated clicks.
    cycle: ClickCycle,
}

impl MouseSelect {
    /// Returns the drag box corners, if a drag box is active.
    pub(crate) fn drag_box(&self, cursor: Vec2) -> Option<(Vec2, Vec2)> {
        self.press
            .filter(|_| self.dragging)
            .map(|press| (press, cursor))
    }

    /// Applies the left button and the selection keys of one frame.
    pub(crate) fn update(
        &mut self,
        pointer: &PointerFrame,
        input: &Input,
        context: &SelectContext<'_>,
        hover: &Hover,
        selection: &mut Selection,
    ) {
        // `Clear` (Escape) is in `Client::update`.
        if input.is_key_pressed(KeyCode::Q) {
            select_all(context, selection, false);
        }
        if input.is_key_pressed(KeyCode::E) {
            select_all(context, selection, true);
        }
        if pointer.left.pressed {
            self.press_left(pointer, context, hover, selection);
        }
        if pointer.left.held && !pointer.left.pressed {
            self.hold_left(pointer, context, selection);
        }
        if pointer.left.released {
            self.release_left(pointer, context, hover, selection);
        }
    }

    fn press_left(
        &mut self,
        pointer: &PointerFrame,
        context: &SelectContext<'_>,
        hover: &Hover,
        selection: &mut Selection,
    ) {
        let double = self.last_press.is_some_and(|(time, cursor)| {
            pointer.time_seconds - time <= DOUBLE_CLICK_SECONDS
                && cursor.distance(pointer.cursor) <= DOUBLE_CLICK_DISTANCE
        });
        self.press = Some(pointer.cursor);
        self.dragging = false;
        self.shift_press = pointer.shift;
        self.skip_release = false;
        if double {
            double_click_select(context, selection, hover, pointer.shift, pointer.shift);
            self.skip_release = true;
            self.last_press = None;
            return;
        }
        self.last_press = Some((pointer.time_seconds, pointer.cursor));
        if pointer.ctrl {
            double_click_select(context, selection, hover, pointer.shift, false);
            self.skip_release = true;
        } else if pointer.shift {
            toggle_select_hovered(context, selection, hover);
        }
    }

    fn hold_left(
        &mut self,
        pointer: &PointerFrame,
        context: &SelectContext<'_>,
        selection: &mut Selection,
    ) {
        let Some(press) = self.press else {
            return;
        };
        if self.skip_release {
            return;
        }
        if !self.dragging {
            if (pointer.cursor - press).abs().min_element() < DRAG_SELECT_MIN_SIZE {
                return;
            }
            self.dragging = true;
            if self.shift_press {
                self.saved = selection.units().to_vec();
            }
        }
        drag_select(context, selection, press, pointer.cursor, &self.saved);
    }

    fn release_left(
        &mut self,
        pointer: &PointerFrame,
        context: &SelectContext<'_>,
        hover: &Hover,
        selection: &mut Selection,
    ) {
        // A release whose press went to the interface does nothing.
        let Some(press) = self.press.take() else {
            return;
        };
        let dragged = std::mem::take(&mut self.dragging);
        let saved = std::mem::take(&mut self.saved);
        if dragged && (pointer.cursor - press).abs().min_element() > RELEASE_DRAG_MIN {
            drag_select(context, selection, press, pointer.cursor, &saved);
            if self.shift_press || !selection.is_empty() {
                return;
            }
        }
        if self.shift_press || std::mem::take(&mut self.skip_release) {
            return;
        }
        select_at_cursor(context, selection, hover, true, &mut self.cycle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drag_box_needs_a_press_and_a_drag() {
        let mut mouse = MouseSelect::default();
        assert_eq!(mouse.drag_box(Vec2::ONE), None);
        mouse.press = Some(Vec2::ZERO);
        assert_eq!(mouse.drag_box(Vec2::ONE), None);
        mouse.dragging = true;
        assert_eq!(mouse.drag_box(Vec2::ONE), Some((Vec2::ZERO, Vec2::ONE)));
    }
}
