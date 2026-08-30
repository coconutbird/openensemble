//! Input handling and UI for the terrain viewer.
//!
//! Implements the `Application` trait: update (keyboard/mouse), UI (egui), and clear color.

use xcore::app::{Application, FrameContext, Input, KeyCode};
use xcore::prelude::*;

use super::TerrainViewer;
use crate::camera::CameraInput;

fn debug_mode_name(mode: u32) -> &'static str {
    match mode {
        0 => "0: Retail Lit",
        1 => "1: Patch Edges",
        7 => "7: Unique Normal",
        8 => "8: Ambient Occlusion",
        9 => "9: Specular",
        10 => "10: Solid Test",
        11 => "11: Terrain UV Viz",
        12 => "12: GPU Composited Albedo",
        13 => "13: Chunk Orientation",
        14 => "14: GPU Height Map",
        15 => "15: Height/Albedo Alignment",
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
    fn synchronize_camera_presentation(&mut self) -> sim::PlayerPresentationState {
        let Some(simulation) = &self.simulation else {
            return sim::PlayerPresentationState::default();
        };
        self.camera_adapter.synchronize_at_time(
            &mut self.camera,
            &simulation.world,
            1,
            self.render_time_seconds,
        )
    }

    fn update_simulation_controls(&mut self, input: &Input) {
        if input.is_key_pressed(KeyCode::M) {
            self.spawn_player_marines(1);
        }
        if input.is_key_pressed(KeyCode::G) {
            self.move_player_squads_to_camera(1);
        }
    }

    fn spawn_player_marines(&mut self, player_id: sim::PlayerId) {
        let (Some(simulation), Some(content)) = (&mut self.simulation, &self.game_content) else {
            return;
        };
        let base_id = simulation.get_initial_base_id(player_id).or_else(|| {
            simulation
                .world
                .bases()
                .find_map(|(id, base)| (base.player_id == player_id).then_some(*id))
        });
        let Some(base_id) = base_id else {
            log::warn!("Player {player_id} has no base for Marine spawning");
            return;
        };
        let proto_name = sim::entities::squads::marine::MARINE_SQUAD_NAME;
        match sim::spawn_squad_from_base_by_name(
            &mut simulation.world,
            &content.database,
            base_id,
            proto_name,
        ) {
            Ok(squad_id) => {
                log::info!("Spawned Player {player_id} Marine squad {squad_id:?} from {base_id:?}");
            }
            Err(error) => log::warn!("Could not spawn Player {player_id} Marines: {error}"),
        }
    }

    fn move_player_squads_to_camera(&mut self, player_id: sim::PlayerId) {
        let Some(simulation) = &self.simulation else {
            return;
        };
        let recipients = simulation
            .world
            .squads
            .iter()
            .filter_map(|(id, squad)| (squad.base.player_id == player_id).then_some(id))
            .collect::<Vec<_>>();
        let Some(y) = recipients
            .first()
            .and_then(|id| simulation.world.get_squad(*id))
            .map(|squad| squad.base.position.y)
        else {
            log::warn!("Player {player_id} has no squads to move");
            return;
        };
        let target = glam::Vec3::new(self.camera.position.x, y, self.camera.position.z);
        let command = sim::WorkCommand::move_squads(i32::from(player_id), recipients, target);
        let exec_time = self
            .simulation_clock
            .game_time_ms
            .saturating_add(sim::MS_PER_TICK);
        self.simulation_clock
            .command_queue
            .enqueue_work(command, exec_time, u64::from(player_id));
        log::info!(
            "Queued Player {player_id} squad move to ({:.1}, {:.1}, {:.1})",
            target.x,
            target.y,
            target.z
        );
    }

    fn advance_simulation(&mut self, dt_seconds: f32) {
        let TerrainViewer {
            simulation: Some(simulation),
            game_content: Some(content),
            simulation_clock,
            asset_source: Some(source),
            ugx_scene: Some(scene),
            ugx_roster_dirty,
            ..
        } = self
        else {
            return;
        };
        simulation_clock.update_with_scenario(dt_seconds, simulation, &content.database);
        if scene.roster_matches_with_gameplay(&simulation.world, &simulation.gameplay) {
            return;
        }
        let active_proto_names =
            render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
        let loaded_visuals = content.load_visuals_for(source, active_proto_names.iter().copied());
        if loaded_visuals > 0 {
            log::info!("Loaded {loaded_visuals} visuals for the updated sim roster");
        }
        *ugx_roster_dirty |= scene.sync_world_with_gameplay(
            source,
            &simulation.world,
            &simulation.gameplay,
            &content.visuals,
            &content.database.objects,
        );
    }

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
            (KeyCode::Q, 0, "retail-style lit terrain"),
            (KeyCode::Key1, 1, "patch edges"),
            (KeyCode::Key7, 7, "unique-map normal"),
            (KeyCode::Key8, 8, "ambient occlusion"),
            (KeyCode::Key9, 9, "specular map"),
            (KeyCode::Backspace, 10, "solid-color pipeline test"),
            (KeyCode::Key0, 12, "GPU-composited albedo diagnostic"),
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

    fn draw_hint_callouts(&self, ctx: &egui::Context) {
        let (Some(simulation), Some(content)) = (&self.simulation, &self.game_content) else {
            return;
        };
        let screen = ctx.screen_rect();
        if screen.width() <= 0.0 || screen.height() <= 0.0 {
            return;
        }
        let view_projection = self
            .camera
            .view_projection_matrix(screen.width() / screen.height());
        for callout in render::terrain::project_hint_callouts(
            &simulation.world,
            view_projection,
            [screen.width(), screen.height()],
        ) {
            let fallback = format!("String {}", callout.string_id);
            let text = content
                .resolve_string(callout.string_id)
                .unwrap_or(&fallback);
            egui::Area::new(egui::Id::new(("simulation_hint_callout", callout.id)))
                .fixed_pos(egui::pos2(
                    callout.screen_position[0],
                    callout.screen_position[1],
                ))
                .pivot(egui::Align2::CENTER_BOTTOM)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(210))
                        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.set_max_width(280.0);
                            ui.colored_label(egui::Color32::WHITE, text);
                        });
                });
        }
    }

    fn draw_game_timer(&mut self, ctx: &egui::Context) {
        let (Some(simulation), Some(content)) = (&self.simulation, &self.game_content) else {
            return;
        };
        let Some(timer) = self.timer_adapter.synchronize(&simulation.world, 1, true) else {
            return;
        };
        let timer_id = timer.id();
        let label_string_id = timer.label_string_id();
        let time = render::ui::format_game_timer(timer.current_time_ms());
        let fallback = label_string_id.map(|string_id| format!("String {string_id}"));
        let label = label_string_id
            .and_then(|string_id| content.resolve_string(string_id).or(fallback.as_deref()));
        let screen_rect = ctx.screen_rect();
        egui::Area::new(egui::Id::new(("simulation_game_timer", timer_id)))
            .fixed_pos(egui::pos2(screen_rect.width() / 2.0, 52.0))
            .pivot(egui::Align2::CENTER_TOP)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(210))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE))
                    .inner_margin(egui::Margin::symmetric(16, 8))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(label) = label {
                                ui.colored_label(egui::Color32::WHITE, label);
                            }
                            ui.label(
                                egui::RichText::new(time)
                                    .color(egui::Color32::WHITE)
                                    .size(24.0)
                                    .monospace(),
                            );
                        });
                    });
            });
    }

    fn draw_objective_pointers(&self, ctx: &egui::Context) {
        let Some(simulation) = &self.simulation else {
            return;
        };
        let screen = ctx.screen_rect();
        if screen.width() <= 0.0 || screen.height() <= 0.0 {
            return;
        }
        let view_projection = self
            .camera
            .view_projection_matrix(screen.width() / screen.height());
        for pointer in render::terrain::project_objective_pointers(
            &simulation.world,
            1,
            view_projection,
            [screen.width(), screen.height()],
        ) {
            egui::Area::new(egui::Id::new((
                "simulation_objective_pointer",
                pointer.widget_id,
            )))
            .fixed_pos(egui::pos2(
                pointer.screen_position[0],
                pointer.screen_position[1],
            ))
            .pivot(egui::Align2::CENTER_CENTER)
            .interactable(false)
            .show(ctx, |ui| {
                let color = if pointer.force_target_visible {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::YELLOW
                };
                let glyph = if pointer.target_on_screen {
                    "◆"
                } else {
                    "➤"
                };
                ui.label(egui::RichText::new(glyph).color(color).size(24.0));
            });
        }
    }

    fn draw_icon_objects(&self, ctx: &egui::Context) {
        let Some(simulation) = &self.simulation else {
            return;
        };
        let screen = ctx.screen_rect();
        if screen.width() <= 0.0 || screen.height() <= 0.0 {
            return;
        }
        let view_projection = self
            .camera
            .view_projection_matrix(screen.width() / screen.height());
        for icon in render::terrain::project_icon_objects(
            &simulation.world,
            1,
            view_projection,
            [screen.width(), screen.height()],
        ) {
            let [red, green, blue] = icon.color_override.unwrap_or([255; 3]);
            let color = egui::Color32::from_rgb(red, green, blue);
            let glyph = if icon.target_on_screen { "●" } else { "◆" };
            egui::Area::new(egui::Id::new((
                "simulation_icon_object",
                icon.entity_id.as_u32(),
            )))
            .fixed_pos(egui::pos2(icon.screen_position[0], icon.screen_position[1]))
            .pivot(egui::Align2::CENTER_CENTER)
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(glyph).color(color).size(20.0));
            });
        }
    }

    fn draw_screen_fade(&self, ctx: &egui::Context) {
        let Some(simulation) = &self.simulation else {
            return;
        };
        let Some([red, green, blue, alpha]) = render::ui::screen_fade_rgba(&simulation.world)
        else {
            return;
        };
        if alpha == 0 {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("simulation_screen_fade"),
        ));
        painter.rect_filled(
            ctx.screen_rect(),
            0.0,
            egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha),
        );
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
        if let Some(environment) = &self.environment {
            ui.label(format!(
                "Environment: {} ({}², {} mips)",
                environment.path(),
                environment.size(),
                environment.mip_count()
            ));
        }
        if let Some(error) = &self.load_error {
            ui.separator();
            ui.colored_label(egui::Color32::RED, error);
        }
        if let Some(simulation) = &self.simulation {
            ui.separator();
            ui.label(format!(
                "Simulation: tick {}, {} players, {} initial bases, {} squads, {} units/buildings, {} projectiles",
                self.simulation_clock.tick,
                simulation.world.player_count(),
                simulation.initial_base_ids.len(),
                simulation.world.squads.len(),
                simulation.world.units.len(),
                simulation.world.projectiles.len(),
            ));
        }
        if let Some(scene) = &self.ugx_scene {
            ui.label(format!(
                "UGX presentation: {} placements from {} sim entities, {} unique visuals",
                scene.placement_count(),
                scene.simulation_entity_count(),
                scene.unique_visual_count()
            ));
        }
    }

    fn draw_debug_buttons(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(format!("Debug Mode: {}", self.debug_mode));
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("0: Retail Lit (Default)", 0), ("12: GPUComp Albedo", 12)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("1: Patch Edges", 1), ("7: Unique Normal", 7)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[("8: AO", 8), ("9: Specular", 9), ("10: Solid", 10)],
        );
        debug_button_row(
            ui,
            &mut self.debug_mode,
            &[
                ("11: Terrain UV", 11),
                ("13: Orientation", 13),
                ("14: Height", 14),
                ("15: Alignment", 15),
            ],
        );
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
            "  M - Spawn Player 1 Marines at base",
            "  G - Move Player 1 squads to camera X/Z",
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
        if let Some(capture) = &self.capture {
            if capture.is_finished() {
                return false;
            }
            // A capture camera is fixed and must not inherit interactive input
            // or compositor LOD changes from the normal viewer camera.
            return self.load_error.is_none();
        }
        self.render_time_seconds += ctx.delta_time;
        self.update_display_toggles(input);
        self.update_debug_keys(input);
        self.update_compositor_controls(input);
        self.update_bump_power(input);
        self.update_simulation_controls(input);
        self.advance_simulation(ctx.delta_time);
        let camera_state = self.synchronize_camera_presentation();
        self.camera.update(input, ctx.delta_time, camera_state);
        self.update_compositor_lod();
        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        if self.capture.is_some() {
            return;
        }
        self.draw_hud(ctx);
        self.draw_game_timer(ctx);
        self.draw_hint_callouts(ctx);
        self.draw_icon_objects(ctx);
        self.draw_objective_pointers(ctx);
        if self.show_info {
            self.draw_information_window(ctx);
        }
        self.draw_screen_fade(ctx);
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        Color::new(0.4, 0.6, 0.9, 1.0)
    }
}
