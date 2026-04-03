//! Input handling and UI for the terrain viewer.
//!
//! Implements the `Application` trait: update (keyboard/mouse), UI (egui), and clear color.

use render::terrain::TessellationMode;
use xcore::app::{Application, FrameContext, Input, KeyCode};
use xcore::prelude::*;

use super::TerrainViewer;
use crate::camera::CameraInput;

impl Application for TerrainViewer {
    fn init(&mut self) {
        log::info!("Terrain Viewer initialized");
        self.load_terrain();
    }

    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool {
        if input.is_key_pressed(KeyCode::Escape) {
            return false;
        }

        if input.is_key_pressed(KeyCode::Tab) {
            self.show_info = !self.show_info;
        }

        if input.is_key_pressed(KeyCode::F) {
            self.wireframe = !self.wireframe;
        }

        // Debug mode toggle: Q for mode 0 (runtime splatting), 0 for mode 12 (GPU composited)
        if input.is_key_pressed(KeyCode::Q) {
            self.debug_mode = 0;
            log::info!("Debug mode: 0 (runtime splatting - HWDE ground truth)");
        }
        if input.is_key_pressed(KeyCode::Key1) {
            self.debug_mode = 1;
            log::info!("Debug mode: 1 (alpha values)");
        }
        if input.is_key_pressed(KeyCode::Key2) {
            self.debug_mode = 2;
            log::info!("Debug mode: 2 (in-chunk UVs)");
        }
        if input.is_key_pressed(KeyCode::Key3) {
            self.debug_mode = 3;
            log::info!("Debug mode: 3 (raw atlas)");
        }
        if input.is_key_pressed(KeyCode::Key4) {
            self.debug_mode = 4;
            log::info!("Debug mode: 4 (terrain UVs)");
        }
        if input.is_key_pressed(KeyCode::Key5) {
            self.debug_mode = 5;
            log::info!("Debug mode: 5 (pre-composited albedo - correct blending)");
        }
        if input.is_key_pressed(KeyCode::Key6) {
            self.debug_mode = 6;
            log::info!("Debug mode: 6 (layer IDs as colors)");
        }
        if input.is_key_pressed(KeyCode::Key7) {
            self.debug_mode = 7;
            log::info!("Debug mode: 7 (chunk grid positions)");
        }
        if input.is_key_pressed(KeyCode::Key8) {
            self.debug_mode = 8;
            log::info!("Debug mode: 8 (chunk_idx + layer0)");
        }
        if input.is_key_pressed(KeyCode::Key0) {
            self.debug_mode = 12;
            log::info!("Debug mode: 12 (GPU composited - default)");
        }
        if input.is_key_pressed(KeyCode::Key9) {
            self.debug_mode = 9;
            log::info!(
                "Debug mode: 9 (Alpha - terrain holes/transparency, white=solid, black=hole)"
            );
        }
        if input.is_key_pressed(KeyCode::Backspace) {
            self.debug_mode = 10;
            log::info!("Debug mode: 10 (Direct texture array test - left=layer0, right=layer1)");
        }

        // Toggle GPU compositing: C key
        if input.is_key_pressed(KeyCode::C) {
            self.use_gpu_compositing = !self.use_gpu_compositing;
            if self.use_gpu_compositing {
                // Mark all chunks as dirty so they get composited
                if let Some(compositor) = &mut self.compositor {
                    compositor.mark_all_dirty();
                }
            }
            log::info!(
                "GPU compositing: {}",
                if self.use_gpu_compositing {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }

        // Cycle compositor debug mode: V key (0=normal, 1=UV, 2=chunkID, 3=alpha, 4=layer0, 5=base_uv)
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
                "Compositor debug mode: {} ({})",
                self.compositor_debug_mode,
                mode_name
            );
            // Re-composite all chunks with the new debug mode
            if let Some(compositor) = &mut self.compositor {
                compositor.mark_all_dirty();
            }
        }

        // Bump power (normal map strength) adjustment: B to decrease, N to increase
        if input.is_key_pressed(KeyCode::B) {
            self.bump_power = (self.bump_power - 0.25).max(0.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }
        if input.is_key_pressed(KeyCode::N) {
            self.bump_power = (self.bump_power + 0.25).min(4.0);
            log::info!("Bump power: {:.2}", self.bump_power);
        }

        // Toggle tessellation mode (T key) - toggles between GPU and None
        // (CPU mode is skipped because it takes ~30 seconds)
        if input.is_key_pressed(KeyCode::T) {
            // Save camera state before reloading
            let saved_camera_pos = self.camera.position;
            let saved_camera_yaw = self.camera.yaw;
            let saved_camera_pitch = self.camera.pitch;

            // Toggle between GPU and None only (skip slow CPU tessellation)
            self.tessellation_mode = match self.tessellation_mode {
                TessellationMode::Gpu => TessellationMode::None,
                TessellationMode::None => TessellationMode::Gpu,
                TessellationMode::Cpu => TessellationMode::Gpu, // Skip CPU, go to GPU
            };
            log::info!(
                "Tessellation mode: {} (reloading terrain...)",
                self.tessellation_mode.name()
            );
            // Clear GPU resources so they get recreated with new mesh
            self.gpu = None;
            // Reload terrain with new tessellation setting
            self.load_terrain();

            // Restore camera state after reloading
            self.camera.position = saved_camera_pos;
            self.camera.yaw = saved_camera_yaw;
            self.camera.pitch = saved_camera_pitch;

            if let Some(ref scene) = self.scene {
                log::info!(
                    "Terrain reloaded: {} vertices, {} triangles",
                    scene.mesh.positions.len(),
                    scene.mesh.indices.len() / 3
                );
            }
        }

        self.camera.update(input, ctx.delta_time);

        // Update LOD levels based on camera position (only when GPU compositing is enabled)
        if self.use_gpu_compositing
            && !self.chunk_centers.is_empty()
            && let Some(compositor) = &mut self.compositor
        {
            let camera_pos = [
                self.camera.position.x,
                self.camera.position.y,
                self.camera.position.z,
            ];
            let lod_changed =
                compositor.update_lod(camera_pos, &self.chunk_centers, &self.lod_config);
            if lod_changed {
                // LOD changed - compositor will mark dirty chunks automatically
                log::debug!(
                    "LOD updated: {} dirty chunks",
                    compositor.dirty_chunk_count()
                );
            }
        }

        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        // Always-visible HUD overlay at top-center of screen
        {
            let debug_name = match self.debug_mode {
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
                17 => "17: CPU Blend",
                _ => "Unknown",
            };
            let comp_debug_name = match self.compositor_debug_mode {
                0 => "Normal",
                1 => "UV Gradient",
                2 => "Chunk ID",
                3 => "Alpha Viz",
                4 => "Layer 0 Only",
                _ => "Unknown",
            };
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
                                format!("Mode: {}  |  Comp: {} (V)", debug_name, comp_debug_name),
                            );
                        });
                });
        }

        if self.show_info {
            egui::Window::new("Terrain Info")
                .default_pos([10.0, 10.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Camera: ({:.1}, {:.1}, {:.1})",
                        self.camera.position.x, self.camera.position.y, self.camera.position.z
                    ));

                    if let Some(scene) = &self.scene {
                        ui.separator();
                        ui.label(format!("Vertices: {}", scene.mesh.positions.len()));
                        ui.label(format!("Triangles: {}", scene.mesh.indices.len() / 3));
                        ui.label(format!(
                            "World Size: {:.0} x {:.0} x {:.0}",
                            scene.mesh.size().x,
                            scene.mesh.size().y,
                            scene.mesh.size().z
                        ));
                        ui.label(format!("Tessellation: {}", self.tessellation_mode.name()));
                        ui.label(format!(
                            "GPU Compositing: {} (C to toggle)",
                            if self.use_gpu_compositing {
                                "ON"
                            } else {
                                "OFF"
                            }
                        ));
                    }

                    if let Some(err) = &self.load_error {
                        ui.separator();
                        ui.colored_label(egui::Color32::RED, err);
                    }

                    ui.separator();
                    ui.label(format!("Debug Mode: {}", self.debug_mode));
                    ui.horizontal(|ui| {
                        if ui.button("0: GPUComp").clicked() {
                            self.debug_mode = 12;
                        }
                        if ui.button("1: Alpha").clicked() {
                            self.debug_mode = 1;
                        }
                        if ui.button("2: UV").clicked() {
                            self.debug_mode = 2;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("3: Atlas").clicked() {
                            self.debug_mode = 3;
                        }
                        if ui.button("4: TerrUV").clicked() {
                            self.debug_mode = 4;
                        }
                        if ui.button("5: Comp").clicked() {
                            self.debug_mode = 5;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("6: LayerID").clicked() {
                            self.debug_mode = 6;
                        }
                        if ui.button("7: ChunkPos").clicked() {
                            self.debug_mode = 7;
                        }
                        if ui.button("8: L1Info").clicked() {
                            self.debug_mode = 8;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("9: XTT").clicked() {
                            self.debug_mode = 9;
                        }
                        if ui.button("10: TexTest").clicked() {
                            self.debug_mode = 10;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("11: TerrUV").clicked() {
                            self.debug_mode = 11;
                        }
                        if ui.button("12: GPUComp").clicked() {
                            self.debug_mode = 12;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("13: L0 Only").clicked() {
                            self.debug_mode = 13;
                        }
                        if ui.button("14: L1 ID").clicked() {
                            self.debug_mode = 14;
                        }
                        if ui.button("15: L1 Only").clicked() {
                            self.debug_mode = 15;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("16: Rock").clicked() {
                            self.debug_mode = 16;
                        }
                        if ui.button("17: CPU Blend").clicked() {
                            self.debug_mode = 17;
                        }
                    });

                    ui.separator();
                    ui.label("Controls:");
                    ui.label("  WASD - Move");
                    ui.label("  Space/Ctrl - Up/Down");
                    ui.label("  Arrows - Look");
                    ui.label("  Shift - Fast");
                    ui.label("  Tab - Toggle info");
                    ui.label("  F - Toggle wireframe");
                    ui.label("  T - Toggle tessellation");
                    ui.label("  Escape - Quit");
                });
        }
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        // Sky blue clear color
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
}
