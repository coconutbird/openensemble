//! The winit event handler that drives an [`Application`].

use std::sync::Arc;
use std::time::Instant;

use gilrs::Gilrs;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use super::renderer::{EguiState, Renderer};
use super::{Application, FrameContext, WindowConfig};
use crate::input::state::InputState;

/// The engine state that wraps the application.
pub(super) struct Engine<A: Application> {
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

impl<A: Application> Engine<A> {
    pub(super) fn new(config: WindowConfig, app: A) -> Self {
        let gilrs = Gilrs::new().unwrap_or_else(|e| {
            log::warn!("Failed to initialize gamepad support: {e}");
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

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
        let total_time = now.duration_since(self.start_time).as_secs_f32();
        self.last_frame_time = now;

        let window_size = self
            .renderer
            .as_ref()
            .map_or((self.config.width, self.config.height), Renderer::size);

        let ctx = FrameContext {
            delta_time,
            total_time,
            window_size,
        };

        self.input_state.poll_gamepad(&mut self.gilrs);

        let should_continue = self.app.update(self.input_state.input(), &ctx);
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

            // Handle platform output (clipboard, cursor, etc.)
            egui_state
                .winit_state
                .handle_platform_output(window, full_output.platform_output);

            match renderer.render_frame(
                egui_state,
                &full_output.textures_delta,
                full_output.shapes,
                |ctx| self.app.render_3d(ctx),
            ) {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost) => {
                    let size = renderer.size();
                    renderer.resize(size.0, size.1);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    log::error!("Out of memory!");
                    self.app.shutdown();
                    event_loop.exit();
                }
                Err(e) => log::warn!("Surface error: {e:?}"),
            }
        }

        self.input_state.end_frame();
    }
}

impl<A: Application> ApplicationHandler for Engine<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        log::info!("Creating window and renderer...");

        let attributes = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height))
            .with_resizable(self.config.resizable);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("Failed to create window"),
        );

        let renderer = Renderer::new(window.clone(), self.config.vsync);
        let egui_state = EguiState::new(&window, &renderer);

        self.app
            .init_gpu(&renderer.device, &renderer.queue, renderer.config.format);

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.egui_state = Some(egui_state);

        self.app.init();
        self.start_time = Instant::now();
        self.last_frame_time = Instant::now();

        log::info!("Engine initialized successfully!");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // egui gets each event first. The application input ignores the events that egui uses.
        let egui_consumed =
            if let (Some(window), Some(egui_state)) = (&self.window, &mut self.egui_state) {
                egui_state
                    .winit_state
                    .on_window_event(window, &event)
                    .consumed
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
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
