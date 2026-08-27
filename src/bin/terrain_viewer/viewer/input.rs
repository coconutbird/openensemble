//! Input handling and UI for the terrain viewer.
//!
//! Implements the `Application` trait: update (keyboard/mouse), UI (egui), and clear color.

use xcore::app::{Application, FrameContext, Input, KeyCode};
use xcore::prelude::*;

use super::TerrainViewer;
use crate::camera::CameraInput;

fn debug_mode_name(mode: u32) -> &'static str {
    match mode {
        0 => "0: Normal",
        1 => "1: Alpha Values",
        2 => "2: In-Chunk UVs",
        3 => "3: Raw Atlas",
        4 => "4: Terrain UVs",
        5 => "5: Pre-Composited",
        6 => "6: Layer IDs",
        7 => "7: Chunk Grid",
        8 => "8: L1 Info",
        9 => "9: XTT Albedo",
        10 => "10: Tex Test",
        11 => "11: Terrain UV Viz",
        12 => "12: GPU Composited",
        13 => "13: Layer 0 Only",
        14 => "14: Layer 1 ID",
        15 => "15: Layer 1 Only",
        16 => "16: Rock",
        17 => "17: Reserved",
        _ => "Unknown",
    }
}

fn compositor_mode_name(mode: u32) -> &'static str {
    match mode {
        0 => "Normal",
        1 => "UV Gradient",
        2 => "Chunk ID",
        3 => "Alpha Viz",
        4 => "Layer 0 Only",
        _ => "Unknown",
    }
}

fn debug_button_row(ui: &mut egui::Ui, debug_mode: &mut u32, buttons: &[(&str, u32)]) {
    ui.horizontal(|ui| {
        for &(label, mode) in buttons {
            if ui.button(label).clicked() {
                *debug_mode = mode;
            }
        }
    });
}

impl TerrainViewer {
    fn update_display_toggles(&mut self, input: &Input) {
        if input.is_key_pressed(KeyCode::Tab) {
            self.show_info = !self.show_info;
        }
        if input.is_key_pressed(KeyCode::F) {
            self.wireframe = !self.wireframe;
        }
    }

    fn update_debug_keys(&mut self, input: &Input) {
        let bindings = [
            (KeyCode::Q, 0, "oracle unique-map material"),
            (KeyCode::Key1, 1, "alpha values"),
            (KeyCode::Key2, 2, "in-chunk UVs"),
            (KeyCode::Key3, 3, "raw atlas"),
            (KeyCode::Key4, 4, "terrain UVs"),
            (KeyCode::Key5, 5, "pre-composited albedo - correct blending"),
            (KeyCode::Key6, 6, "layer IDs as colors"),
            (KeyCode::Key7, 7, "chunk grid positions"),
            (KeyCode::Key8, 8, "chunk_idx + layer0"),
            (KeyCode::Key0, 12, "GPU composited - default"),
            (
                KeyCode::Key9,
                9,
                "Alpha - terrain holes/transparency, white=solid, black=hole",
            ),
            (
                KeyCode::Backspace,
                10,
                "Direct texture array test - left=layer0, right=layer1",
            ),
        ];
        for (key, mode, description) in bindings {
            if input.is_key_pressed(key) {
                self.debug_mode = mode;
                log::info!("Debug mode: {mode} ({description})");
            }
        }
    }

    fn update_compositor_controls(&mut self, input: &Input) {
        if input.is_key_pressed(KeyCode::V) {
            self.compositor_debug_mode = (self.compositor_debug_mode + 1) % 6;
            let mode_name = match self.compositor_debug_mode {
                0 => "normal compositing",
                1 => "UV gradient (R=u, G=v)",
                2 => "chunk ID color (R=gridX, G=gridZ)",
                3 => "alpha visualization (R/G/B)",
                4 => "layer 0 only (base texture)",
                5 => "base_uv/16 (should match Mode 11)",
                _ => "unknown",
            };
            log::info!(
                "Compositor debug mode: {} ({mode_name})",
                self.compositor_debug_mode
            );
            if let Some(compositor) = &mut self.compositor {
                compositor.mark_all_dirty();
            }
        }
    }

    fn update_bump_power(&mut self, input: &Input) {
        if input.is_key_pressed(KeyCode::B) {
            self.bump_power = (self.bump_power - 0.25).max(0.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }
        if input.is_key_pressed(KeyCode::N) {
            self.bump_power = (self.bump_power + 0.25).min(4.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }
    }

    fn update_compositor_lod(&mut self) {
        if self.chunk_centers.is_empty() {
            return;
        }
        let Some(compositor) = &mut self.compositor else {
            return;
        };
        let camera_position = [
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
        ];
        if compositor.update_lod(camera_position, &self.chunk_centers, &self.lod_config) {
            log::debug!(
                "LOD updated: {} dirty chunks",
                compositor.dirty_chunk_count()
            );
        }
    }

    fn draw_hud(&self, ctx: &egui::Context) {
        let debug_name = debug_mode_name(self.debug_mode);
        let compositor_name = compositor_mode_name(self.compositor_debug_mode);
        let screen_rect = ctx.screen_rect();
        egui::Area::new(egui::Id::new("hud_overlay"))
            .fixed_pos(egui::pos2(screen_rect.width() / 2.0 - 160.0, 8.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(180))
                    .inner_margin(egui::Margin::same(8))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.colored_label(
                            egui::Color32::WHITE,
                            format!("Mode: {debug_name}  |  Comp: {compositor_name} (V)"),
                        );
                    });
            });
    }

    fn draw_scene_information(&self, ui: &mut egui::Ui) {
        ui.label(format!(
            "Camera: ({:.1}, {:.1}, {:.1})",
            self.camera.position.x, self.camera.position.y, self.camera.position.z
        ));
        if let Some(scene) = &self.scene {
            let size = scene.mesh.size();
            ui.separator();
            ui.label(format!("Vertices: {}", scene.mesh.positions.len()));
            ui.label(format!("Triangles: {}", scene.mesh.indices.len() / 3));
            ui.label(format!(
                "World Size: {:.0} x {:.0} x {:.0}",
                size.x, size.y, size.z
            ));
            ui.label("Terrain path: packed GPU patches");
            ui.label("Material: oracle unique-map compositor");
        }
        if let Some(error) = &self.load_error {
            ui.separator();
            ui.colored_label(egui::Color32::RED, error);
        }
    }

    fn draw_debug_buttons(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(format!("Debug Mode: {}", self.debug_mode));
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("0: GPUComp", 12), ("1: Alpha", 1), ("2: UV", 2)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("3: Atlas", 3), ("4: TerrUV", 4), ("5: Comp", 5)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("6: LayerID", 6), ("7: ChunkPos", 7), ("8: L1Info", 8)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("9: XTT", 9), ("10: TexTest", 10)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("11: TerrUV", 11), ("12: GPUComp", 12)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("13: L0 Only", 13), ("14: L1 ID", 14), ("15: L1 Only", 15)],
        );
        debug_button_row(ui, &mut self.debug_mode, &[("16: Rock", 16)]);
    }

    fn draw_controls(ui: &mut egui::Ui) {
        ui.separator();
        ui.label("Controls:");
        for control in [
            "  WASD - Move",
            "  Space/Ctrl - Up/Down",
            "  Arrows - Look",
            "  Shift - Fast",
            "  Tab - Toggle info",
            "  F - Toggle wireframe",
            "  Escape - Quit",
        ] {
            ui.label(control);
        }
    }

    fn draw_information_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Terrain Info")
            .default_pos([10.0, 10.0])
            .show(ctx, |ui| {
                self.draw_scene_information(ui);
                self.draw_debug_buttons(ui);
                Self::draw_controls(ui);
            });
    }
}

impl Application for TerrainViewer {
    fn init(&mut self) {
        log::info!("Terrain Viewer initialized");
        self.load_terrain();
    }

    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool {
        if input.is_key_pressed(KeyCode::Escape) {
            return false;
        }
        self.update_display_toggles(input);
        self.update_debug_keys(input);
        self.update_compositor_controls(input);
        self.update_bump_power(input);
        self.camera.update(input, ctx.delta_time);
        self.update_compositor_lod();
        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        self.draw_hud(ctx);
        if self.show_info {
            self.draw_information_window(ctx);
        }
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
}
