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
    /// This is called after `update()` and before `render()`
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
    /// Window title text.
    pub title: String,
    /// Initial window width in pixels.
    pub width: u32,
    /// Initial window height in pixels.
    pub height: u32,
    /// If `true`, the user can resize the window.
    pub resizable: bool,
    /// If `true`, presentation waits for vertical sync.
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
    /// Creates a configuration with the given title and size in pixels; other fields use defaults.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            ..Default::default()
        }
    }
}
