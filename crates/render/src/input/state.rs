//! Conversion of window and gamepad events into an [`Input`].

use gilrs::{Axis, Button, Gilrs};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};

use super::{GamepadButton, Input, KeyCode};

/// The axis value below which a gamepad stick reads as zero.
const STICK_DEADZONE: f32 = 0.15;

/// Keeps the [`Input`] of the current frame up to date from window and gamepad events.
///
/// The tracker converts each event when it arrives. Keys and buttons without a
/// platform-neutral code do not change the input.
pub(crate) struct InputState {
    input: Input,
}

impl InputState {
    pub(crate) fn new() -> Self {
        Self {
            input: Input::default(),
        }
    }

    /// Returns the input of the current frame.
    pub(crate) fn input(&self) -> &Input {
        &self.input
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    self.key_input(key, event.state, event.repeat);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse_position = (position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let index = match button {
                    MouseButton::Left => Some(0),
                    MouseButton::Middle => Some(1),
                    MouseButton::Right => Some(2),
                    _ => None,
                };
                if let Some(index) = index {
                    self.input.mouse_buttons[index] = *state == ElementState::Pressed;
                }
            }
            _ => {}
        }
    }

    fn key_input(&mut self, key: WinitKeyCode, state: ElementState, repeat: bool) {
        let Some(key) = convert_key(key) else {
            return;
        };
        match state {
            ElementState::Pressed => {
                if !repeat {
                    self.input.keys_pressed.insert(key);
                }
                self.input.keys_held.insert(key);
            }
            ElementState::Released => {
                self.input.keys_held.remove(&key);
                self.input.keys_released.insert(key);
            }
        }
    }

    /// Applies all pending gamepad events.
    pub(crate) fn poll_gamepad(&mut self, gilrs: &mut Gilrs) {
        while let Some(event) = gilrs.next_event() {
            match event.event {
                gilrs::EventType::Connected => {
                    log::info!("Gamepad connected: {:?}", gilrs.gamepad(event.id).name());
                    self.input.gamepad.connected = true;
                }
                gilrs::EventType::Disconnected => {
                    log::info!("Gamepad disconnected");
                    self.gamepad_disconnected();
                }
                gilrs::EventType::ButtonPressed(button, _) => self.gamepad_button_pressed(button),
                gilrs::EventType::ButtonReleased(button, _) => {
                    self.gamepad_button_released(button);
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    self.gamepad_axis_changed(axis, value);
                }
                gilrs::EventType::ButtonChanged(button, value, _) => {
                    self.gamepad_button_changed(button, value);
                }
                _ => {}
            }
        }

        if !self.input.gamepad.connected {
            for (_id, gamepad) in gilrs.gamepads() {
                if gamepad.is_connected() {
                    log::info!("Found gamepad: {}", gamepad.name());
                    self.input.gamepad.connected = true;
                    break;
                }
            }
        }
    }

    fn gamepad_disconnected(&mut self) {
        let gamepad = &mut self.input.gamepad;
        gamepad.connected = false;
        gamepad.left_stick_x = 0.0;
        gamepad.left_stick_y = 0.0;
        gamepad.right_stick_x = 0.0;
        gamepad.right_stick_y = 0.0;
        gamepad.left_trigger = 0.0;
        gamepad.right_trigger = 0.0;
        gamepad.buttons_held.clear();
    }

    fn gamepad_button_pressed(&mut self, button: Button) {
        let Some(button) = convert_gamepad_button(button) else {
            return;
        };
        let gamepad = &mut self.input.gamepad;
        if !gamepad.buttons_held.contains(&button) {
            gamepad.buttons_pressed.insert(button);
        }
        gamepad.buttons_held.insert(button);
    }

    fn gamepad_button_released(&mut self, button: Button) {
        if let Some(button) = convert_gamepad_button(button) {
            self.input.gamepad.buttons_held.remove(&button);
        }
    }

    fn gamepad_axis_changed(&mut self, axis: Axis, value: f32) {
        let value = if value.abs() < STICK_DEADZONE {
            0.0
        } else {
            value
        };
        let gamepad = &mut self.input.gamepad;
        match axis {
            Axis::LeftStickX => gamepad.left_stick_x = value,
            Axis::LeftStickY => gamepad.left_stick_y = value,
            Axis::RightStickX => gamepad.right_stick_x = value,
            Axis::RightStickY => gamepad.right_stick_y = value,
            _ => {}
        }
    }

    /// Reads the analog triggers. Other button values do not change the input.
    fn gamepad_button_changed(&mut self, button: Button, value: f32) {
        match button {
            Button::LeftTrigger2 => self.input.gamepad.left_trigger = value,
            Button::RightTrigger2 => self.input.gamepad.right_trigger = value,
            _ => {}
        }
    }

    /// Clears the input that applies to one frame only. The sets keep their capacity.
    pub(crate) fn end_frame(&mut self) {
        self.input.keys_pressed.clear();
        self.input.keys_released.clear();
        self.input.gamepad.buttons_pressed.clear();
    }
}

/// Convert gilrs button to our platform-agnostic button
fn convert_gamepad_button(button: Button) -> Option<GamepadButton> {
    Some(match button {
        Button::South => GamepadButton::South,
        Button::East => GamepadButton::East,
        Button::West => GamepadButton::West,
        Button::North => GamepadButton::North,
        Button::LeftTrigger => GamepadButton::LeftBumper,
        Button::RightTrigger => GamepadButton::RightBumper,
        Button::LeftThumb => GamepadButton::LeftStick,
        Button::RightThumb => GamepadButton::RightStick,
        Button::Start => GamepadButton::Start,
        Button::Select => GamepadButton::Select,
        Button::DPadUp => GamepadButton::DPadUp,
        Button::DPadDown => GamepadButton::DPadDown,
        Button::DPadLeft => GamepadButton::DPadLeft,
        Button::DPadRight => GamepadButton::DPadRight,
        _ => return None,
    })
}

/// Convert winit key code to our platform-agnostic key code
fn convert_key(key: WinitKeyCode) -> Option<KeyCode> {
    Some(match key {
        WinitKeyCode::KeyA => KeyCode::A,
        WinitKeyCode::KeyB => KeyCode::B,
        WinitKeyCode::KeyC => KeyCode::C,
        WinitKeyCode::KeyD => KeyCode::D,
        WinitKeyCode::KeyE => KeyCode::E,
        WinitKeyCode::KeyF => KeyCode::F,
        WinitKeyCode::KeyG => KeyCode::G,
        WinitKeyCode::KeyH => KeyCode::H,
        WinitKeyCode::KeyI => KeyCode::I,
        WinitKeyCode::KeyJ => KeyCode::J,
        WinitKeyCode::KeyK => KeyCode::K,
        WinitKeyCode::KeyL => KeyCode::L,
        WinitKeyCode::KeyM => KeyCode::M,
        WinitKeyCode::KeyN => KeyCode::N,
        WinitKeyCode::KeyO => KeyCode::O,
        WinitKeyCode::KeyP => KeyCode::P,
        WinitKeyCode::KeyQ => KeyCode::Q,
        WinitKeyCode::KeyR => KeyCode::R,
        WinitKeyCode::KeyS => KeyCode::S,
        WinitKeyCode::KeyT => KeyCode::T,
        WinitKeyCode::KeyU => KeyCode::U,
        WinitKeyCode::KeyV => KeyCode::V,
        WinitKeyCode::KeyW => KeyCode::W,
        WinitKeyCode::KeyX => KeyCode::X,
        WinitKeyCode::KeyY => KeyCode::Y,
        WinitKeyCode::KeyZ => KeyCode::Z,
        WinitKeyCode::Digit0 => KeyCode::Key0,
        WinitKeyCode::Digit1 => KeyCode::Key1,
        WinitKeyCode::Digit2 => KeyCode::Key2,
        WinitKeyCode::Digit3 => KeyCode::Key3,
        WinitKeyCode::Digit4 => KeyCode::Key4,
        WinitKeyCode::Digit5 => KeyCode::Key5,
        WinitKeyCode::Digit6 => KeyCode::Key6,
        WinitKeyCode::Digit7 => KeyCode::Key7,
        WinitKeyCode::Digit8 => KeyCode::Key8,
        WinitKeyCode::Digit9 => KeyCode::Key9,
        WinitKeyCode::F1 => KeyCode::F1,
        WinitKeyCode::F2 => KeyCode::F2,
        WinitKeyCode::F3 => KeyCode::F3,
        WinitKeyCode::F4 => KeyCode::F4,
        WinitKeyCode::F5 => KeyCode::F5,
        WinitKeyCode::F6 => KeyCode::F6,
        WinitKeyCode::F7 => KeyCode::F7,
        WinitKeyCode::F8 => KeyCode::F8,
        WinitKeyCode::F9 => KeyCode::F9,
        WinitKeyCode::F10 => KeyCode::F10,
        WinitKeyCode::F11 => KeyCode::F11,
        WinitKeyCode::F12 => KeyCode::F12,
        WinitKeyCode::Escape => KeyCode::Escape,
        WinitKeyCode::Space => KeyCode::Space,
        WinitKeyCode::Enter => KeyCode::Enter,
        WinitKeyCode::Tab => KeyCode::Tab,
        WinitKeyCode::Backspace => KeyCode::Backspace,
        WinitKeyCode::Delete => KeyCode::Delete,
        WinitKeyCode::Insert => KeyCode::Insert,
        WinitKeyCode::Home => KeyCode::Home,
        WinitKeyCode::End => KeyCode::End,
        WinitKeyCode::PageUp => KeyCode::PageUp,
        WinitKeyCode::PageDown => KeyCode::PageDown,
        WinitKeyCode::ArrowLeft => KeyCode::Left,
        WinitKeyCode::ArrowRight => KeyCode::Right,
        WinitKeyCode::ArrowUp => KeyCode::Up,
        WinitKeyCode::ArrowDown => KeyCode::Down,
        WinitKeyCode::ShiftLeft => KeyCode::LShift,
        WinitKeyCode::ShiftRight => KeyCode::RShift,
        WinitKeyCode::ControlLeft => KeyCode::LCtrl,
        WinitKeyCode::ControlRight => KeyCode::RCtrl,
        WinitKeyCode::AltLeft => KeyCode::LAlt,
        WinitKeyCode::AltRight => KeyCode::RAlt,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_sequence_tracks_held_pressed_and_released_keys() {
        let mut state = InputState::new();

        state.key_input(WinitKeyCode::KeyW, ElementState::Pressed, false);
        state.key_input(WinitKeyCode::KeyW, ElementState::Pressed, true);
        state.key_input(WinitKeyCode::NumpadAdd, ElementState::Pressed, false);
        let input = state.input();
        assert!(input.is_key_held(KeyCode::W));
        assert!(input.is_key_pressed(KeyCode::W));
        assert_eq!(input.keys_held.len(), 1);
        assert_eq!(input.keys_pressed.len(), 1);
        let pressed_capacity = input.keys_pressed.capacity();

        state.end_frame();
        assert!(state.input().keys_pressed.is_empty());
        assert_eq!(state.input().keys_pressed.capacity(), pressed_capacity);
        assert!(state.input().is_key_held(KeyCode::W));

        state.key_input(WinitKeyCode::KeyW, ElementState::Pressed, true);
        assert!(!state.input().is_key_pressed(KeyCode::W));
        assert!(state.input().is_key_held(KeyCode::W));

        state.key_input(WinitKeyCode::KeyW, ElementState::Released, false);
        state.key_input(WinitKeyCode::NumpadAdd, ElementState::Released, false);
        assert!(!state.input().is_key_held(KeyCode::W));
        assert!(state.input().is_key_released(KeyCode::W));
        assert_eq!(state.input().keys_released.len(), 1);

        state.end_frame();
        assert!(state.input().keys_released.is_empty());
        assert!(state.input().keys_held.is_empty());
    }

    #[test]
    fn gamepad_sequence_tracks_buttons_axes_and_disconnection() {
        let mut state = InputState::new();
        state.input.gamepad.connected = true;

        state.gamepad_button_pressed(Button::South);
        state.gamepad_button_pressed(Button::LeftTrigger2);
        state.gamepad_axis_changed(Axis::LeftStickX, 0.1);
        state.gamepad_axis_changed(Axis::LeftStickY, -0.5);
        state.gamepad_button_changed(Button::RightTrigger2, 0.75);
        let gamepad = &state.input().gamepad;
        assert!(gamepad.is_button_held(GamepadButton::South));
        assert!(gamepad.is_button_pressed(GamepadButton::South));
        assert_eq!(gamepad.buttons_held.len(), 1);
        assert!(gamepad.left_stick_x.abs() < f32::EPSILON);
        assert!((gamepad.left_stick_y + 0.5).abs() < f32::EPSILON);
        assert!((gamepad.right_trigger - 0.75).abs() < f32::EPSILON);

        state.end_frame();
        state.gamepad_button_pressed(Button::South);
        assert!(
            !state
                .input()
                .gamepad
                .is_button_pressed(GamepadButton::South)
        );

        state.gamepad_button_released(Button::South);
        state.gamepad_button_pressed(Button::East);
        state.gamepad_disconnected();
        let gamepad = &state.input().gamepad;
        assert!(!gamepad.connected);
        assert!(gamepad.buttons_held.is_empty());
        assert!(gamepad.is_button_pressed(GamepadButton::East));
        assert!(gamepad.left_stick_y.abs() < f32::EPSILON);
        assert!(gamepad.right_trigger.abs() < f32::EPSILON);

        state.end_frame();
        assert!(state.input().gamepad.buttons_pressed.is_empty());
    }
}
