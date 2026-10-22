//! The application contract and the window loop.
//!
//! An application implements [`Application`]. [`run`] opens a window, creates the GPU device
//! and egui, and calls the application each frame.

mod engine;
mod renderer;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::input::Input;
use engine::Engine;

/// A color with RGBA float channels (0.0 - 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red channel (0.0 - 1.0).
    pub r: f32,
    /// Green channel (0.0 - 1.0).
    pub g: f32,
    /// Blue channel (0.0 - 1.0).
    pub b: f32,
    /// Alpha channel (0.0 - 1.0); 1.0 is opaque.
    pub a: f32,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Creates a color from float channels (0.0 - 1.0).
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// The frame data that the engine gives to the application.
pub struct FrameContext {
    /// Time since the previous frame, in seconds.
    pub delta_time: f32,
    /// Time since the start, in seconds.
    pub total_time: f32,
    /// Current window size (width, height).
    pub window_size: (u32, u32),
}

/// The GPU resources that the engine lends to [`Application::render_3d`].
pub struct RenderContext<'a> {
    /// The wgpu device that creates GPU resources.
    pub device: &'a wgpu::Device,
    /// The wgpu queue that receives commands.
    pub queue: &'a wgpu::Queue,
    /// The view of the texture of the current frame.
    pub view: &'a wgpu::TextureView,
    /// The command encoder of the current frame.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The surface texture format.
    pub format: wgpu::TextureFormat,
    /// Current window size (width, height).
    pub size: (u32, u32),
}

/// The callbacks that an application gives to the engine.
///
/// The engine calls the callbacks in this sequence:
///
/// 1. [`init_gpu`](Self::init_gpu) and then [`init`](Self::init), one time.
/// 2. Each frame: [`update`](Self::update), [`render`](Self::render), [`ui`](Self::ui) and
///    [`render_3d`](Self::render_3d).
/// 3. [`shutdown`](Self::shutdown), one time.
///
/// All callbacks other than [`update`](Self::update) have empty default implementations.
pub trait Application {
    /// Called one time after the GPU device and egui are ready, before [`init`](Self::init).
    ///
    /// Create GPU resources here.
    fn init_gpu(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _format: wgpu::TextureFormat,
    ) {
    }

    /// Called one time when the application starts.
    fn init(&mut self) {}

    /// Called each frame to update the application.
    ///
    /// Return `false` to exit the application.
    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool;

    /// Called each frame after [`update`](Self::update) to get the clear color of the frame.
    fn render(&mut self, _ctx: &FrameContext) -> Color {
        Color::BLACK
    }

    /// Called each frame after [`render`](Self::render) to build the egui user interface.
    fn ui(&mut self, _ctx: &egui::Context) {}

    /// Called each frame to record custom GPU commands.
    ///
    /// The engine records these commands after the clear pass and before the egui pass.
    fn render_3d(&mut self, _ctx: &mut RenderContext<'_>) {}

    /// Called when the window size changes, after the surface changes size.
    ///
    /// Recreate the GPU resources that depend on the window size here. The engine calls
    /// [`on_resize`](Self::on_resize) after this callback.
    fn resize_gpu(&mut self, _device: &wgpu::Device, _width: u32, _height: u32) {}

    /// Called when the window size changes.
    fn on_resize(&mut self, _width: u32, _height: u32) {}

    /// Called when the application is about to exit.
    fn shutdown(&mut self) {}
}

/// The configuration of the application window.
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

/// Runs an application in a window with the given configuration.
///
/// # Errors
///
/// Returns an error if the event loop cannot be created or run.
pub fn run<A: Application + 'static>(config: WindowConfig, app: A) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut engine = Engine::new(config, app);
    event_loop.run_app(&mut engine)?;

    Ok(())
}
