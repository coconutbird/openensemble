//! Platform-neutral input for applications.
//!
//! The engine converts window and gamepad events into an [`Input`] and lends it to
//! [`Application::update`](crate::app::Application::update) each frame.

use std::collections::HashSet;

pub(crate) mod state;

/// Input state passed to the application each frame
#[derive(Debug, Clone, Default)]
pub struct Input {
    /// Keys currently held down
    pub keys_held: HashSet<KeyCode>,
    /// Keys pressed this frame
    pub keys_pressed: HashSet<KeyCode>,
    /// Keys released this frame
    pub keys_released: HashSet<KeyCode>,
    /// Mouse position (x, y) in physical pixels from the top-left corner of the window
    pub mouse_position: (f64, f64),
    /// Mouse buttons held (left, middle, right)
    pub mouse_buttons: [bool; 3],
    /// Mouse buttons pressed this frame (left, middle, right)
    pub mouse_buttons_pressed: [bool; 3],
    /// Mouse buttons released this frame (left, middle, right)
    pub mouse_buttons_released: [bool; 3],
    /// Mouse wheel movement in this frame, in lines. A positive value is a wheel turn up,
    /// away from the user.
    pub scroll_delta: f32,
    /// Gamepad state (if connected)
    pub gamepad: GamepadState,
}

impl Input {
    /// Check if a key is currently held
    #[must_use]
    pub fn is_key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    /// Check if a key was just pressed this frame
    #[must_use]
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Check if a key was just released this frame
    #[must_use]
    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    /// Check if a mouse button is currently held
    #[must_use]
    pub fn is_mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_buttons[button.index()]
    }

    /// Check if a mouse button was pressed this frame
    #[must_use]
    pub fn is_mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons_pressed[button.index()]
    }

    /// Check if a mouse button was released this frame
    ///
    /// A release arrives even when the user interface uses the event, so that a drag that
    /// ends over a window still ends. A button that the user interface got the press of
    /// does not give a release.
    #[must_use]
    pub fn is_mouse_released(&self, button: MouseButton) -> bool {
        self.mouse_buttons_released[button.index()]
    }
}

/// Platform-agnostic mouse buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left button
    Left,
    /// Middle button (wheel click)
    Middle,
    /// Right button
    Right,
}

impl MouseButton {
    /// Index of the button in the mouse button arrays of [`Input`]
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

/// Gamepad/controller input state
#[derive(Debug, Clone, Default)]
pub struct GamepadState {
    /// Whether a gamepad is connected
    pub connected: bool,
    /// Left stick X axis (-1.0 to 1.0)
    pub left_stick_x: f32,
    /// Left stick Y axis (-1.0 to 1.0)
    pub left_stick_y: f32,
    /// Right stick X axis (-1.0 to 1.0)
    pub right_stick_x: f32,
    /// Right stick Y axis (-1.0 to 1.0)
    pub right_stick_y: f32,
    /// Left trigger (0.0 to 1.0)
    pub left_trigger: f32,
    /// Right trigger (0.0 to 1.0)
    pub right_trigger: f32,
    /// Buttons currently held
    pub buttons_held: HashSet<GamepadButton>,
    /// Buttons pressed this frame
    pub buttons_pressed: HashSet<GamepadButton>,
}

impl GamepadState {
    /// Check if a button is currently held
    #[must_use]
    pub fn is_button_held(&self, button: GamepadButton) -> bool {
        self.buttons_held.contains(&button)
    }

    /// Check if a button was just pressed this frame
    #[must_use]
    pub fn is_button_pressed(&self, button: GamepadButton) -> bool {
        self.buttons_pressed.contains(&button)
    }
}

/// Platform-agnostic gamepad buttons (Xbox-style naming)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GamepadButton {
    /// A button (Xbox) / Cross (`PlayStation`)
    South,
    /// B button (Xbox) / Circle (`PlayStation`)
    East,
    /// X button (Xbox) / Square (`PlayStation`)
    West,
    /// Y button (Xbox) / Triangle (`PlayStation`)
    North,
    /// Left bumper (LB)
    LeftBumper,
    /// Right bumper (RB)
    RightBumper,
    /// Left stick click (L3)
    LeftStick,
    /// Right stick click (R3)
    RightStick,
    /// Start / Menu button
    Start,
    /// Select / View button
    Select,
    /// D-pad up
    DPadUp,
    /// D-pad down
    DPadDown,
    /// D-pad left
    DPadLeft,
    /// D-pad right
    DPadRight,
}

/// Platform-agnostic key codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    /// Letter key `A`.
    A,
    /// Letter key `B`.
    B,
    /// Letter key `C`.
    C,
    /// Letter key `D`.
    D,
    /// Letter key `E`.
    E,
    /// Letter key `F`.
    F,
    /// Letter key `G`.
    G,
    /// Letter key `H`.
    H,
    /// Letter key `I`.
    I,
    /// Letter key `J`.
    J,
    /// Letter key `K`.
    K,
    /// Letter key `L`.
    L,
    /// Letter key `M`.
    M,
    /// Letter key `N`.
    N,
    /// Letter key `O`.
    O,
    /// Letter key `P`.
    P,
    /// Letter key `Q`.
    Q,
    /// Letter key `R`.
    R,
    /// Letter key `S`.
    S,
    /// Letter key `T`.
    T,
    /// Letter key `U`.
    U,
    /// Letter key `V`.
    V,
    /// Letter key `W`.
    W,
    /// Letter key `X`.
    X,
    /// Letter key `Y`.
    Y,
    /// Letter key `Z`.
    Z,
    /// Number row key `0`.
    Key0,
    /// Number row key `1`.
    Key1,
    /// Number row key `2`.
    Key2,
    /// Number row key `3`.
    Key3,
    /// Number row key `4`.
    Key4,
    /// Number row key `5`.
    Key5,
    /// Number row key `6`.
    Key6,
    /// Number row key `7`.
    Key7,
    /// Number row key `8`.
    Key8,
    /// Number row key `9`.
    Key9,
    /// Function key `F1`.
    F1,
    /// Function key `F2`.
    F2,
    /// Function key `F3`.
    F3,
    /// Function key `F4`.
    F4,
    /// Function key `F5`.
    F5,
    /// Function key `F6`.
    F6,
    /// Function key `F7`.
    F7,
    /// Function key `F8`.
    F8,
    /// Function key `F9`.
    F9,
    /// Function key `F10`.
    F10,
    /// Function key `F11`.
    F11,
    /// Function key `F12`.
    F12,
    /// `Escape` key.
    Escape,
    /// Space bar.
    Space,
    /// `Enter` key.
    Enter,
    /// `Tab` key.
    Tab,
    /// `Backspace` key.
    Backspace,
    /// `Delete` key.
    Delete,
    /// `Insert` key.
    Insert,
    /// `Home` key.
    Home,
    /// `End` key.
    End,
    /// `Page Up` key.
    PageUp,
    /// `Page Down` key.
    PageDown,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Left `Shift` key.
    LShift,
    /// Right `Shift` key.
    RShift,
    /// Left `Ctrl` key.
    LCtrl,
    /// Right `Ctrl` key.
    RCtrl,
    /// Left `Alt` key.
    LAlt,
    /// Right `Alt` key.
    RAlt,
}
