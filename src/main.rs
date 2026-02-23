//! OpenEnsemble - Halo Wars Phoenix Engine Reimplementation
//!
//! A Rust implementation of the Phoenix Engine from Halo Wars (2008).

use anyhow::Result;
use xcore::prelude::*;

/// The main game application
struct Game {
    show_demo_window: bool,
    show_settings: bool,
    player_name: String,
    volume: f32,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            show_demo_window: false,
            show_settings: false,
            player_name: "Spartan".to_string(),
            volume: 0.75,
        }
    }
}

impl Application for Game {
    fn init(&mut self) {
        log::info!("Game initialized!");
    }

    fn update(&mut self, input: &Input, _ctx: &FrameContext) -> bool {
        // Exit on Escape
        if input.is_key_pressed(KeyCode::Escape) {
            return false;
        }
        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        // Main menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Game").clicked() {
                        log::info!("New Game clicked!");
                        ui.close_menu();
                    }
                    if ui.button("Load Game").clicked() {
                        log::info!("Load Game clicked!");
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_settings, "Settings");
                    ui.checkbox(&mut self.show_demo_window, "Demo Window");
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        log::info!("OpenEnsemble - Halo Wars Phoenix Engine Reimplementation");
                        ui.close_menu();
                    }
                });
            });
        });

        // Settings window
        if self.show_settings {
            egui::Window::new("Settings")
                .open(&mut self.show_settings)
                .show(ctx, |ui| {
                    ui.heading("Game Settings");
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Player Name:");
                        ui.text_edit_singleline(&mut self.player_name);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Volume:");
                        ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0));
                    });

                    ui.separator();
                    if ui.button("Apply").clicked() {
                        log::info!(
                            "Settings applied: name={}, volume={}",
                            self.player_name,
                            self.volume
                        );
                    }
                });
        }

        // Demo window (for testing egui features)
        if self.show_demo_window {
            egui::Window::new("egui Demo").show(ctx, |ui| {
                ui.heading("Welcome to egui!");
                ui.label("This is a demo of the egui immediate mode GUI.");
                ui.separator();
                ui.label("Use View > Demo Window to toggle this.");
            });
        }

        // Status bar at bottom
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("OpenEnsemble v0.1.0");
                ui.separator();
                ui.label(format!("Player: {}", self.player_name));
            });
        });
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        // Dark blue background (Halo-ish)
        Color::new(0.05, 0.05, 0.15, 1.0)
    }

    fn shutdown(&mut self) {
        log::info!("Game shutting down!");
    }
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("OpenEnsemble starting...");

    // Configure and run
    let config = WindowConfig::new("OpenEnsemble - Halo Wars Engine", 1280, 720);
    render::run(config, Game::default())?;

    Ok(())
}
