//! The pause menu that the retail `Start` control (F10 on the keyboard) opens.
//!
//! Escape clears the selection (retail `Clear`), so F10 opens the menu. The menu has Resume and
//! Quit. The retail pause menu has more pages (options, objectives); this menu has only the two
//! items. The simulation, the camera and the game input stop while the menu is open.

use render::input::{Input, KeyCode};

/// The open state of the pause menu and the quit request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PauseMenu {
    /// If true, the menu is open and the game is paused.
    open: bool,
    /// If true, the player selected Quit.
    quit: bool,
}

/// The item that the player selected in the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseAction {
    Resume,
    Quit,
}

impl PauseMenu {
    /// Opens or closes the menu on F10. Returns `false` after the player selected Quit.
    pub(crate) fn update(&mut self, input: &Input) -> bool {
        if input.is_key_pressed(KeyCode::F10) {
            self.open = !self.open;
        }
        !self.quit
    }

    /// Returns `true` while the menu is open.
    pub(crate) fn is_open(self) -> bool {
        self.open
    }

    /// Draws the menu when it is open and applies the selected item.
    ///
    /// The modal backdrop blocks the clicks on the other parts of the interface. Escape or a
    /// click outside the menu resumes the game.
    pub(super) fn draw(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut action = None;
        let response = egui::Modal::new(egui::Id::new("client_pause_menu")).show(ctx, |ui| {
            ui.set_width(160.0);
            ui.vertical_centered_justified(|ui| {
                ui.heading("Paused");
                if ui.button("Resume").clicked() {
                    action = Some(PauseAction::Resume);
                }
                if ui.button("Quit").clicked() {
                    action = Some(PauseAction::Quit);
                }
            });
        });
        if action.is_none() && response.should_close() {
            action = Some(PauseAction::Resume);
        }
        self.apply(action);
    }

    fn apply(&mut self, action: Option<PauseAction>) {
        match action {
            Some(PauseAction::Resume) => self.open = false,
            Some(PauseAction::Quit) => self.quit = true,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_closes_the_menu_and_quit_stops_the_client() {
        let mut menu = PauseMenu {
            open: true,
            quit: false,
        };
        menu.apply(None);
        assert!(menu.is_open());
        menu.apply(Some(PauseAction::Resume));
        assert!(!menu.is_open());
        assert!(menu.update(&Input::default()));
        menu.apply(Some(PauseAction::Quit));
        assert!(!menu.update(&Input::default()));
    }
}
