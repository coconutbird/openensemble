use std::collections::HashSet;

use gilrs::{Axis, Button, Gilrs};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use xcore::app::{GamepadButton, GamepadState, Input, KeyCode};

/// Internal input state tracker
pub(super) struct InputState {
    keys_held: HashSet<WinitKeyCode>,
    keys_pressed: HashSet<WinitKeyCode>,
    keys_released: HashSet<WinitKeyCode>,
    mouse_x: f64,
    mouse_y: f64,
    mouse_buttons: [bool; 3],
    // Gamepad state
    gamepad_connected: bool,
    gamepad_left_stick: (f32, f32),
    gamepad_right_stick: (f32, f32),
    gamepad_triggers: (f32, f32),
    gamepad_buttons_held: HashSet<Button>,
    gamepad_buttons_pressed: HashSet<Button>,
}

impl InputState {
    pub(super) fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_buttons: [false; 3],
            gamepad_connected: false,
            gamepad_left_stick: (0.0, 0.0),
            gamepad_right_stick: (0.0, 0.0),
            gamepad_triggers: (0.0, 0.0),
            gamepad_buttons_held: HashSet::new(),
            gamepad_buttons_pressed: HashSet::new(),
        }
    }

    pub(super) fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                self.keys_pressed.insert(key);
                            }
                            self.keys_held.insert(key);
                        }
                        ElementState::Released => {
                            self.keys_held.remove(&key);
                            self.keys_released.insert(key);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x;
                self.mouse_y = position.y;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let idx = match button {
                    MouseButton::Left => Some(0),
                    MouseButton::Middle => Some(1),
                    MouseButton::Right => Some(2),
                    _ => None,
                };
                if let Some(idx) = idx {
                    self.mouse_buttons[idx] = *state == ElementState::Pressed;
                }
            }
            _ => {}
        }
    }

    /// Poll gamepad events and update state
    pub(super) fn poll_gamepad(&mut self, gilrs: &mut Gilrs) {
        // Process all pending events
        while let Some(event) = gilrs.next_event() {
            match event.event {
                gilrs::EventType::Connected => {
                    log::info!("Gamepad connected: {:?}", gilrs.gamepad(event.id).name());
                    self.gamepad_connected = true;
                }
                gilrs::EventType::Disconnected => {
                    log::info!("Gamepad disconnected");
                    self.gamepad_connected = false;
                    self.gamepad_left_stick = (0.0, 0.0);
                    self.gamepad_right_stick = (0.0, 0.0);
                    self.gamepad_triggers = (0.0, 0.0);
                    self.gamepad_buttons_held.clear();
                }
                gilrs::EventType::ButtonPressed(button, _) => {
                    if !self.gamepad_buttons_held.contains(&button) {
                        self.gamepad_buttons_pressed.insert(button);
                    }
                    self.gamepad_buttons_held.insert(button);
                }
                gilrs::EventType::ButtonReleased(button, _) => {
                    self.gamepad_buttons_held.remove(&button);
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    // Apply deadzone
                    let value = if value.abs() < 0.15 { 0.0 } else { value };
                    match axis {
                        Axis::LeftStickX => self.gamepad_left_stick.0 = value,
                        Axis::LeftStickY => self.gamepad_left_stick.1 = value,
                        Axis::RightStickX => self.gamepad_right_stick.0 = value,
                        Axis::RightStickY => self.gamepad_right_stick.1 = value,
                        _ => {}
                    }
                }
                gilrs::EventType::ButtonChanged(button, value, _) => {
                    // Handle triggers as analog
                    match button {
                        Button::LeftTrigger2 => self.gamepad_triggers.0 = value,
                        Button::RightTrigger2 => self.gamepad_triggers.1 = value,
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Check if any gamepad is connected
        if !self.gamepad_connected {
            for (_id, gamepad) in gilrs.gamepads() {
                if gamepad.is_connected() {
                    log::info!("Found gamepad: {}", gamepad.name());
                    self.gamepad_connected = true;
                    break;
                }
            }
        }
    }

    pub(super) fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.gamepad_buttons_pressed.clear();
    }

    pub(super) fn build_input(&self) -> Input {
        Input {
            keys_held: self
                .keys_held
                .iter()
                .filter_map(|k| convert_key(*k))
                .collect(),
            keys_pressed: self
                .keys_pressed
                .iter()
                .filter_map(|k| convert_key(*k))
                .collect(),
            keys_released: self
                .keys_released
                .iter()
                .filter_map(|k| convert_key(*k))
                .collect(),
            mouse_position: (self.mouse_x, self.mouse_y),
            mouse_buttons: self.mouse_buttons,
            gamepad: GamepadState {
                connected: self.gamepad_connected,
                left_stick_x: self.gamepad_left_stick.0,
                left_stick_y: self.gamepad_left_stick.1,
                right_stick_x: self.gamepad_right_stick.0,
                right_stick_y: self.gamepad_right_stick.1,
                left_trigger: self.gamepad_triggers.0,
                right_trigger: self.gamepad_triggers.1,
                buttons_held: self
                    .gamepad_buttons_held
                    .iter()
                    .filter_map(|b| convert_gamepad_button(*b))
                    .collect(),
                buttons_pressed: self
                    .gamepad_buttons_pressed
                    .iter()
                    .filter_map(|b| convert_gamepad_button(*b))
                    .collect(),
            },
        }
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
