//! OpenEnsemble Renderer
//!
//! Rendering subsystem using wgpu (pure Rust).
//! Provides a fully abstracted window and rendering system with egui integration.

pub mod terrain;

use egui_wgpu::ScreenDescriptor;
use gilrs::{Axis, Button, Gilrs};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};
use xcore::app::{
    Application, FrameContext, GamepadButton, GamepadState, Input, KeyCode, WindowConfig,
};
use xcore::prelude::*;

// Re-export wgpu for applications that need direct GPU access
pub use wgpu;

/// Context provided to applications for 3D rendering.
///
/// This provides access to wgpu resources for custom rendering.
pub struct RenderContext<'a> {
    /// The wgpu device for creating GPU resources
    pub device: &'a wgpu::Device,
    /// The wgpu queue for submitting commands
    pub queue: &'a wgpu::Queue,
    /// The current frame's texture view to render to
    pub view: &'a wgpu::TextureView,
    /// The command encoder for this frame
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The surface texture format
    pub format: wgpu::TextureFormat,
    /// Current window size (width, height)
    pub size: (u32, u32),
}

/// Trait for applications that need custom 3D rendering.
///
/// Implement this trait alongside `Application` to get access to wgpu resources.
pub trait Application3D: Application {
    /// Called once when the renderer is ready, to create GPU resources.
    fn init_gpu(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _format: wgpu::TextureFormat,
    ) {
    }

    /// Called when the window is resized, to recreate size-dependent resources.
    fn resize_gpu(&mut self, _device: &wgpu::Device, _width: u32, _height: u32) {}

    /// Called each frame to perform custom 3D rendering.
    /// This is called after the clear pass and before the egui pass.
    fn render_3d(&mut self, _ctx: &mut RenderContext) {}
}

/// Run an application with the given window configuration
pub fn run<A: Application + 'static>(config: WindowConfig, app: A) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut engine = Engine::new(config, app);
    event_loop.run_app(&mut engine)?;

    Ok(())
}

/// Run a 3D application with the given window configuration
pub fn run_3d<A: Application3D + 'static>(config: WindowConfig, app: A) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut engine = Engine3D::new(config, app);
    event_loop.run_app(&mut engine)?;

    Ok(())
}

/// Internal engine state that wraps the user's application
struct Engine<A: Application> {
    config: WindowConfig,
    app: A,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    egui_state: Option<EguiState>,
    input_state: InputState,
    gilrs: Gilrs,
    start_time: Instant,
    last_frame_time: Instant,
}

/// Egui integration state
struct EguiState {
    ctx: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl<A: Application> Engine<A> {
    fn new(config: WindowConfig, app: A) -> Self {
        let gilrs = Gilrs::new().unwrap_or_else(|e| {
            log::warn!("Failed to initialize gamepad support: {}", e);
            // Create a dummy gilrs that won't find any gamepads
            Gilrs::new().expect("Failed to initialize gilrs twice")
        });
        Self {
            config,
            app,
            window: None,
            renderer: None,
            egui_state: None,
            input_state: InputState::new(),
            gilrs,
            start_time: Instant::now(),
            last_frame_time: Instant::now(),
        }
    }
}

impl<A: Application> ApplicationHandler for Engine<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            log::info!("Creating window and renderer...");

            // Build window attributes
            let mut attributes = WindowAttributes::default()
                .with_title(&self.config.title)
                .with_inner_size(PhysicalSize::new(self.config.width, self.config.height))
                .with_resizable(self.config.resizable);

            if !self.config.resizable {
                attributes = attributes.with_resizable(false);
            }

            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .expect("Failed to create window"),
            );

            // Create renderer
            let renderer = Renderer::new(window.clone(), self.config.vsync);

            // Initialize egui
            let egui_ctx = egui::Context::default();
            let egui_winit_state = egui_winit::State::new(
                egui_ctx.clone(),
                egui_ctx.viewport_id(),
                &window,
                None,
                None,
                None,
            );
            let egui_renderer =
                egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1, false);
            let egui_state = EguiState {
                ctx: egui_ctx,
                winit_state: egui_winit_state,
                renderer: egui_renderer,
            };

            self.window = Some(window);
            self.renderer = Some(renderer);
            self.egui_state = Some(egui_state);

            // Initialize the application
            self.app.init();
            self.start_time = Instant::now();
            self.last_frame_time = Instant::now();

            log::info!("Engine initialized successfully!");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Pass events to egui first
        let egui_consumed =
            if let (Some(window), Some(egui_state)) = (&self.window, &mut self.egui_state) {
                let response = egui_state.winit_state.on_window_event(window, &event);
                response.consumed
            } else {
                false
            };

        // Only handle input if egui didn't consume it
        if !egui_consumed {
            self.input_state.handle_event(&event);
        }

        match &event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested, shutting down...");
                self.app.shutdown();
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(physical_size.width, physical_size.height);
                }
                self.app
                    .on_resize(physical_size.width, physical_size.height);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
                let total_time = now.duration_since(self.start_time).as_secs_f32();
                self.last_frame_time = now;

                let window_size = self
                    .renderer
                    .as_ref()
                    .map(|r| r.size())
                    .unwrap_or((self.config.width, self.config.height));

                let ctx = FrameContext {
                    delta_time,
                    total_time,
                    window_size,
                };

                // Poll gamepad and build input for the app
                self.input_state.poll_gamepad(&mut self.gilrs);
                let input = self.input_state.build_input();

                // Update the application
                let should_continue = self.app.update(&input, &ctx);
                if !should_continue {
                    log::info!("Application requested exit");
                    self.app.shutdown();
                    event_loop.exit();
                    return;
                }

                // Render
                if let (Some(renderer), Some(window), Some(egui_state)) =
                    (&mut self.renderer, &self.window, &mut self.egui_state)
                {
                    let clear_color = self.app.render(&ctx);
                    renderer.set_clear_color(clear_color);

                    // Begin egui frame
                    let raw_input = egui_state.winit_state.take_egui_input(window);
                    egui_state.ctx.begin_pass(raw_input);

                    // Let the app build UI
                    self.app.ui(&egui_state.ctx);

                    // End egui frame
                    let full_output = egui_state.ctx.end_pass();

                    // Handle platform output (clipboard, cursor, etc.)
                    egui_state
                        .winit_state
                        .handle_platform_output(window, full_output.platform_output);

                    // Render with egui
                    match renderer.render_with_egui(
                        egui_state,
                        full_output.textures_delta,
                        full_output.shapes,
                    ) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let size = renderer.size();
                            renderer.resize(size.0, size.1);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log::error!("Out of memory!");
                            self.app.shutdown();
                            event_loop.exit();
                        }
                        Err(e) => log::warn!("Surface error: {:?}", e),
                    }
                }

                // Clear per-frame input state
                self.input_state.end_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Internal engine state for 3D applications
struct Engine3D<A: Application3D> {
    config: WindowConfig,
    app: A,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    egui_state: Option<EguiState>,
    input_state: InputState,
    gilrs: Gilrs,
    start_time: Instant,
    last_frame_time: Instant,
    gpu_initialized: bool,
}

impl<A: Application3D> Engine3D<A> {
    fn new(config: WindowConfig, app: A) -> Self {
        let gilrs = Gilrs::new().unwrap_or_else(|e| {
            log::warn!("Failed to initialize gamepad support: {}", e);
            Gilrs::new().expect("Failed to initialize gilrs twice")
        });
        Self {
            config,
            app,
            window: None,
            renderer: None,
            egui_state: None,
            input_state: InputState::new(),
            gilrs,
            start_time: Instant::now(),
            last_frame_time: Instant::now(),
            gpu_initialized: false,
        }
    }
}

impl<A: Application3D> ApplicationHandler for Engine3D<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            log::info!("Creating window and renderer...");

            let mut attributes = WindowAttributes::default()
                .with_title(&self.config.title)
                .with_inner_size(PhysicalSize::new(self.config.width, self.config.height))
                .with_resizable(self.config.resizable);

            if !self.config.resizable {
                attributes = attributes.with_resizable(false);
            }

            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .expect("Failed to create window"),
            );

            let renderer = Renderer::new(window.clone(), self.config.vsync);

            let egui_ctx = egui::Context::default();
            let egui_winit_state = egui_winit::State::new(
                egui_ctx.clone(),
                egui_ctx.viewport_id(),
                &window,
                None,
                None,
                None,
            );
            let egui_renderer =
                egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1, false);
            let egui_state = EguiState {
                ctx: egui_ctx,
                winit_state: egui_winit_state,
                renderer: egui_renderer,
            };

            // Initialize GPU resources for 3D rendering
            self.app
                .init_gpu(&renderer.device, &renderer.queue, renderer.config.format);
            self.gpu_initialized = true;

            self.window = Some(window);
            self.renderer = Some(renderer);
            self.egui_state = Some(egui_state);

            self.app.init();
            self.start_time = Instant::now();
            self.last_frame_time = Instant::now();

            log::info!("Engine initialized successfully!");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let egui_consumed =
            if let (Some(window), Some(egui_state)) = (&self.window, &mut self.egui_state) {
                let response = egui_state.winit_state.on_window_event(window, &event);
                response.consumed
            } else {
                false
            };

        if !egui_consumed {
            self.input_state.handle_event(&event);
        }

        match &event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested, shutting down...");
                self.app.shutdown();
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(physical_size.width, physical_size.height);
                    self.app.resize_gpu(
                        &renderer.device,
                        physical_size.width,
                        physical_size.height,
                    );
                }
                self.app
                    .on_resize(physical_size.width, physical_size.height);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
                let total_time = now.duration_since(self.start_time).as_secs_f32();
                self.last_frame_time = now;

                let window_size = self
                    .renderer
                    .as_ref()
                    .map(|r| r.size())
                    .unwrap_or((self.config.width, self.config.height));

                let ctx = FrameContext {
                    delta_time,
                    total_time,
                    window_size,
                };

                // Poll gamepad and build input
                self.input_state.poll_gamepad(&mut self.gilrs);
                let input = self.input_state.build_input();

                let should_continue = self.app.update(&input, &ctx);
                if !should_continue {
                    log::info!("Application requested exit");
                    self.app.shutdown();
                    event_loop.exit();
                    return;
                }

                if let (Some(renderer), Some(window), Some(egui_state)) =
                    (&mut self.renderer, &self.window, &mut self.egui_state)
                {
                    let clear_color = self.app.render(&ctx);
                    renderer.set_clear_color(clear_color);

                    let raw_input = egui_state.winit_state.take_egui_input(window);
                    egui_state.ctx.begin_pass(raw_input);
                    self.app.ui(&egui_state.ctx);
                    let full_output = egui_state.ctx.end_pass();

                    egui_state
                        .winit_state
                        .handle_platform_output(window, full_output.platform_output);

                    // Render with 3D callback
                    match renderer.render_with_egui_and_3d(
                        egui_state,
                        full_output.textures_delta,
                        full_output.shapes,
                        |ctx| self.app.render_3d(ctx),
                    ) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let size = renderer.size();
                            renderer.resize(size.0, size.1);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log::error!("Out of memory!");
                            self.app.shutdown();
                            event_loop.exit();
                        }
                        Err(e) => log::warn!("Surface error: {:?}", e),
                    }
                }

                self.input_state.end_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Internal input state tracker
struct InputState {
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
    fn new() -> Self {
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

    fn handle_event(&mut self, event: &WindowEvent) {
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
    fn poll_gamepad(&mut self, gilrs: &mut Gilrs) {
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

    fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.gamepad_buttons_pressed.clear();
    }

    fn build_input(&self) -> Input {
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

/// The wgpu renderer
struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    clear_color: wgpu::Color,
}

impl Renderer {
    fn new(window: Arc<Window>, vsync: bool) -> Self {
        pollster::block_on(Self::new_async(window, vsync))
    }

    async fn new_async(window: Arc<Window>, vsync: bool) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

        log::info!("Using GPU: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("OpenEnsemble Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            size: (size.width, size.height),
            clear_color: wgpu::Color::BLACK,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = (width, height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn set_clear_color(&mut self, color: Color) {
        self.clear_color = wgpu::Color {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        };
    }

    fn render_with_egui(
        &mut self,
        egui_state: &mut EguiState,
        textures_delta: egui::TexturesDelta,
        shapes: Vec<egui::epaint::ClippedShape>,
    ) -> std::result::Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Clear pass
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }

        // Update egui textures
        for (id, image_delta) in &textures_delta.set {
            egui_state
                .renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Tessellate shapes
        let pixels_per_point = egui_state.ctx.pixels_per_point();
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point,
        };
        let paint_jobs = egui_state.ctx.tessellate(shapes, pixels_per_point);

        // Update egui buffers
        egui_state.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Render egui
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // egui-wgpu requires a 'static lifetime render pass, so we use forget_lifetime
            let mut render_pass = render_pass.forget_lifetime();

            egui_state
                .renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        // Free textures
        for id in &textures_delta.free {
            egui_state.renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn render_with_egui_and_3d<F>(
        &mut self,
        egui_state: &mut EguiState,
        textures_delta: egui::TexturesDelta,
        shapes: Vec<egui::epaint::ClippedShape>,
        render_3d: F,
    ) -> std::result::Result<(), wgpu::SurfaceError>
    where
        F: FnOnce(&mut RenderContext),
    {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Clear pass
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }

        // 3D rendering pass
        {
            let mut render_ctx = RenderContext {
                device: &self.device,
                queue: &self.queue,
                view: &view,
                encoder: &mut encoder,
                format: self.config.format,
                size: self.size,
            };
            render_3d(&mut render_ctx);
        }

        // Update egui textures
        for (id, image_delta) in &textures_delta.set {
            egui_state
                .renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        // Tessellate shapes
        let pixels_per_point = egui_state.ctx.pixels_per_point();
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point,
        };
        let paint_jobs = egui_state.ctx.tessellate(shapes, pixels_per_point);

        // Update egui buffers
        egui_state.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Render egui
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let mut render_pass = render_pass.forget_lifetime();
            egui_state
                .renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        // Free textures
        for id in &textures_delta.free {
            egui_state.renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the wgpu queue
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get the surface format
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }
}
