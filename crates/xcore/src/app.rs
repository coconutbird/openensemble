//! Application trait for game loop abstraction

use crate::math::Color;

/// Input state passed to the application each frame
#[derive(Debug, Clone, Default)]
pub struct Input {
    /// Keys currently held down
    pub keys_held: std::collections::HashSet<KeyCode>,
    /// Keys pressed this frame
    pub keys_pressed: std::collections::HashSet<KeyCode>,
    /// Keys released this frame
    pub keys_released: std::collections::HashSet<KeyCode>,
    /// Mouse position (x, y)
    pub mouse_position: (f64, f64),
    /// Mouse buttons held (left, middle, right)
    pub mouse_buttons: [bool; 3],
    /// Gamepad state (if connected)
    pub gamepad: GamepadState,
}

impl Input {
    /// Check if a key is currently held
    pub fn is_key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    /// Check if a key was just pressed this frame
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Check if a key was just released this frame
    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
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
    pub buttons_held: std::collections::HashSet<GamepadButton>,
    /// Buttons pressed this frame
    pub buttons_pressed: std::collections::HashSet<GamepadButton>,
}

impl GamepadState {
    /// Check if a button is currently held
    pub fn is_button_held(&self, button: GamepadButton) -> bool {
        self.buttons_held.contains(&button)
    }

    /// Check if a button was just pressed this frame
    pub fn is_button_pressed(&self, button: GamepadButton) -> bool {
        self.buttons_pressed.contains(&button)
    }
}

/// Platform-agnostic gamepad buttons (Xbox-style naming)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GamepadButton {
    /// A button (Xbox) / Cross (PlayStation)
    South,
    /// B button (Xbox) / Circle (PlayStation)
    East,
    /// X button (Xbox) / Square (PlayStation)
    West,
    /// Y button (Xbox) / Triangle (PlayStation)
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
    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // Numbers
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    // Special keys
    Escape,
    Space,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    // Modifiers
    LShift,
    RShift,
    LCtrl,
    RCtrl,
    LAlt,
    RAlt,
}

/// Context provided to the application during rendering
pub struct FrameContext {
    /// Time elapsed since last frame in seconds
    pub delta_time: f32,
    /// Total time elapsed since start in seconds
    pub total_time: f32,
    /// Current window size (width, height)
    pub window_size: (u32, u32),
}

/// Trait that applications implement to receive engine callbacks
pub trait Application {
    /// Called once when the application starts
    fn init(&mut self) {}

    /// Called each frame to update game logic
    /// Return `false` to exit the application
    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool;

    /// Called each frame to build the UI using egui
    /// This is called after update() and before render()
    fn ui(&mut self, _ctx: &egui::Context) {}

    /// Called each frame to render
    /// Return the clear color for the frame
    fn render(&mut self, _ctx: &FrameContext) -> Color {
        Color::BLACK
    }

    /// Called when the window is resized
    fn on_resize(&mut self, _width: u32, _height: u32) {}

    /// Called when the application is about to exit
    fn shutdown(&mut self) {}
}

/// Configuration for creating a window
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub vsync: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "OpenEnsemble".to_string(),
            width: 1280,
            height: 720,
            resizable: true,
            vsync: true,
        }
    }
}

impl WindowConfig {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            ..Default::default()
        }
    }
}
