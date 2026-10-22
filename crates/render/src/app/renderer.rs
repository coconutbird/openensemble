//! The window surface, the GPU device and the egui renderer.

use std::sync::Arc;

use egui_wgpu::ScreenDescriptor;
use winit::window::Window;

use super::{Color, RenderContext};

/// The egui integration state.
pub(super) struct EguiState {
    pub(super) ctx: egui::Context,
    pub(super) winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiState {
    pub(super) fn new(window: &Window, renderer: &Renderer) -> Self {
        let ctx = egui::Context::default();
        let winit_state =
            egui_winit::State::new(ctx.clone(), ctx.viewport_id(), window, None, None, None);
        let renderer =
            egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1, false);
        Self {
            ctx,
            winit_state,
            renderer,
        }
    }
}

/// The wgpu renderer of the window.
pub(super) struct Renderer {
    surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    clear_color: wgpu::Color,
}

impl Renderer {
    pub(super) fn new(window: Arc<Window>, vsync: bool) -> Self {
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

    pub(super) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = (width, height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    pub(super) fn set_clear_color(&mut self, color: Color) {
        self.clear_color = wgpu::Color {
            r: f64::from(color.r),
            g: f64::from(color.g),
            b: f64::from(color.b),
            a: f64::from(color.a),
        };
    }

    /// Records and presents one frame.
    ///
    /// The frame contains these passes in sequence: the clear pass, the commands of `render_3d`,
    /// and the egui pass.
    pub(super) fn render_frame<F>(
        &mut self,
        egui_state: &mut EguiState,
        textures_delta: &egui::TexturesDelta,
        shapes: Vec<egui::epaint::ClippedShape>,
        render_3d: F,
    ) -> Result<(), wgpu::SurfaceError>
    where
        F: FnOnce(&mut RenderContext<'_>),
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

        // Custom 3D commands of the application
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
}
