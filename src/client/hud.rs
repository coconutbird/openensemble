//! The egui interface of the client.
//!
//! Each module has one part of the interface:
//!
//! - `top_bar`: the resources, the populations and the game time of the local player, and the
//!   frame-rate line.
//! - `selection_panel`: the selected squads and buildings.
//! - `building_menu`: the train and research commands of the menu building, with the retail
//!   item rules.
//! - `production_queue`: the queue of the menu building.
//! - `pause_menu`: the F10 menu with Resume and Quit.
//!
//! The interface sends the building commands of the local player through the command queue of
//! the session, at the next tick, like the right-click work commands.

mod building_menu;
#[cfg(test)]
mod fixture;
mod pause_menu;
mod production_queue;
mod selection_panel;
mod top_bar;

use pipeline::hw1;
use render::input::Input;
use sim::BuildingCommand;

use super::Client;
use pause_menu::PauseMenu;

/// The state of the interface between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Hud {
    /// The F10 pause menu.
    pause_menu: PauseMenu,
}

impl Hud {
    /// Applies the F10 key. Returns `false` after the player selected Quit in the pause menu.
    pub(crate) fn update_pause(&mut self, input: &Input) -> bool {
        self.pause_menu.update(input)
    }

    /// Returns `true` while the pause menu is open. The game does not advance then.
    pub(crate) fn is_paused(self) -> bool {
        self.pause_menu.is_open()
    }
}

/// The panel background of the interface.
fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_black_alpha(170))
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4.0)
}

/// Returns the display name of a string table ID, or the proto name if the table has no text.
fn display_name(content: &hw1::World, display_name_id: Option<i32>, proto_name: &str) -> String {
    display_name_id
        .and_then(|id| content.resolve_string(id))
        .filter(|text| !text.trim().is_empty())
        .map_or_else(|| proto_name.to_owned(), str::to_owned)
}

impl Client {
    /// Draws the interface of one frame and queues the building command that it gives.
    pub(super) fn draw_hud(&mut self, ctx: &egui::Context) {
        self.draw_top_bar(ctx);
        if let Some(command) = self.draw_game_panels(ctx) {
            self.send_building_command(command);
        }
        self.hud.pause_menu.draw(ctx);
    }

    /// Draws the stats of the local player, the frame-rate line and the load error.
    fn draw_top_bar(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("client_top_bar"))
            .fixed_pos(egui::pos2(8.0, 8.0))
            .interactable(false)
            .show(ctx, |ui| {
                panel_frame().show(ui, |ui| {
                    if let Some(session) = &self.session
                        && let (Some(world), Some(content)) = (session.world(), &session.content)
                        && let Some(player) = world.get_player(self.local_player)
                    {
                        let stats = top_bar::player_stats(&content.database, player);
                        top_bar::draw(ui, &stats, world.game_time_ms);
                    }
                    let line = self.frame_stats.last().map_or_else(
                        || "Measuring frame rate...".to_owned(),
                        |report| report.summary(),
                    );
                    ui.add(
                        egui::Label::new(egui::RichText::new(line).color(egui::Color32::GRAY))
                            .extend(),
                    );
                    if let Some(error) = &self.load_error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
                });
            });
    }

    /// Draws the selection panel and the menu of the menu building. Returns the command of a
    /// menu click.
    fn draw_game_panels(&self, ctx: &egui::Context) -> Option<BuildingCommand> {
        let session = self.session.as_ref()?;
        let world = session.world()?;
        let content = session.content.as_ref()?;
        let database = &content.database;
        let rows = selection_panel::selection_rows(
            world,
            database,
            &self.catalog,
            self.selection.squads(),
            self.selection.units(),
        );
        if !rows.is_empty() {
            egui::Area::new(egui::Id::new("client_selection_panel"))
                .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(8.0, -8.0))
                .show(ctx, |ui| {
                    panel_frame().show(ui, |ui| {
                        selection_panel::draw(ui, &rows, |row| {
                            display_name(content, row.display_name_id, &row.name)
                        });
                    });
                });
        }
        let building_id = self.selection.menu_building()?;
        let building = world.get_building(building_id)?;
        if building.base.player_id != self.local_player {
            return None;
        }
        let items = building_menu::menu_items(world, database, self.local_player, building_id);
        let queue = production_queue::queue_rows(database, &building.production);
        let resource_names = database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.resources.as_ref())
            .map(|resources| {
                resources
                    .entries
                    .iter()
                    .map(|resource| resource.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let building_name = database
            .objects
            .iter()
            .find(|object| {
                object
                    .name
                    .eq_ignore_ascii_case(&building.proto_object_name)
            })
            .and_then(|object| object.display_name_id);
        let title = display_name(content, building_name, &building.proto_object_name);
        egui::Area::new(egui::Id::new("client_building_menu"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-8.0, -8.0))
            .show(ctx, |ui| {
                panel_frame()
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        let command = building_menu::draw(
                            ui,
                            &items,
                            &resource_names,
                            |item| display_name(content, item.display_name_id, &item.target_name),
                            self.local_player,
                            building_id,
                        );
                        ui.separator();
                        production_queue::draw(ui, &queue, |row| {
                            display_name(content, row.display_name_id, &row.name)
                        });
                        command
                    })
                    .inner
            })
            .inner
    }

    /// Queues a building command of the local player for the next simulation tick.
    fn send_building_command(&mut self, command: BuildingCommand) {
        let Some(session) = &mut self.session else {
            return;
        };
        log::debug!(
            "Building command {:?} target {} count {} for {:?}",
            command.building_type,
            command.target_id,
            command.count,
            command.base.recipients
        );
        let exec_time = session.clock.game_time_ms.saturating_add(sim::MS_PER_TICK);
        session.clock.command_queue.enqueue_building(
            command,
            exec_time,
            u64::from(self.local_player),
        );
    }
}
