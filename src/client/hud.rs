//! The egui interface of the client.
//!
//! The interface shows the frame-rate line and the load error.

use super::Client;

impl Client {
    /// Draws the interface of one frame.
    pub(super) fn draw_hud(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("client_frame_rate"))
            .fixed_pos(egui::pos2(8.0, 8.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(160))
                    .inner_margin(egui::Margin::same(6))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        let line = self.frame_stats.last().map_or_else(
                            || "Measuring frame rate...".to_owned(),
                            |report| report.summary(),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(line).color(egui::Color32::WHITE))
                                .extend(),
                        );
                        if let Some(error) = &self.load_error {
                            ui.colored_label(egui::Color32::RED, error);
                        }
                    });
            });
    }
}
