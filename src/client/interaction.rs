//! The frame update of the hover, the selection and the right-click order, and the drag box
//! draw.

use glam::Vec2;
use openensemble::session::{ScenarioSession, world_camera};
use render::app::FrameContext;
use render::input::{Input, KeyCode, MouseButton};

use super::Client;
use super::hover::{HoverContext, update_hover};
use super::mouse::PointerFrame;
use super::orders::work_command;
use super::select::{SelectContext, rebuild_selection};

/// The drag box colour of a UNSC player (`0xFF1776C7`, `BUser_dragSelect`).
const UNSC_DRAG_BOX_COLOR: [u8; 3] = [0x17, 0x76, 0xC7];
/// The drag box colour of a Covenant player (`0xFF45298C`).
const COVENANT_DRAG_BOX_COLOR: [u8; 3] = [0x45, 0x29, 0x8C];
/// The drag box line width, in points.
///
/// This value is not a retail value.
const DRAG_BOX_LINE_WIDTH: f32 = 1.0;

/// Returns the drag box colour for the civilization name `civ`.
///
/// Retail picks one of the two colours from a table by civilization; the client takes the
/// Covenant colour for the `Covenant` civilization and the UNSC colour for all others.
pub(crate) fn drag_box_color(civ: Option<&str>) -> [u8; 3] {
    if civ.is_some_and(|civ| civ.eq_ignore_ascii_case("Covenant")) {
        COVENANT_DRAG_BOX_COLOR
    } else {
        UNSC_DRAG_BOX_COLOR
    }
}

impl Client {
    /// Updates the hover, applies the selection input and sends the right-click order.
    pub(super) fn update_interaction(&mut self, input: &Input, ctx: &FrameContext) {
        let Some(session) = &mut self.session else {
            return;
        };
        let Some(simulation) = &session.simulation else {
            return;
        };
        let world = &simulation.world;
        let camera = world_camera(&self.camera, ctx.window_size);
        let team = world
            .get_player(self.local_player)
            .map_or(0, |player| player.team_id);
        let pointer = PointerFrame::from_input(input, self.render_time_seconds);
        self.cursor = pointer.cursor;
        let hover_context = HoverContext {
            world,
            gameplay: &simulation.gameplay,
            catalog: &self.catalog,
            camera: &camera,
            player: self.local_player,
            team,
        };
        update_hover(
            &hover_context,
            pointer.cursor,
            self.selection.squads(),
            &mut self.hover,
        );
        let select_context = SelectContext {
            world,
            catalog: &self.catalog,
            player: self.local_player,
            camera: &camera,
        };
        self.mouse.update(
            &pointer,
            input,
            &select_context,
            &self.hover,
            &mut self.selection,
        );
        // `DoWork` and `DoWorkQueue` (Shift): the command executes on the next tick.
        if !input.is_mouse_pressed(MouseButton::Right) {
            return;
        }
        let queue = input.is_key_held(KeyCode::LShift) || input.is_key_held(KeyCode::RShift);
        let Some(command) = work_command(
            world,
            self.local_player,
            self.selection.squads(),
            &self.hover,
            queue,
        ) else {
            return;
        };
        log::debug!(
            "Work command: {} squads to {:?}, target {:?}, queue {queue}",
            self.selection.squads().len(),
            self.hover.point,
            self.hover.entity.filter(|_| self.hover.kind.has_target())
        );
        let exec_time = session.clock.game_time_ms.saturating_add(sim::MS_PER_TICK);
        session
            .clock
            .command_queue
            .enqueue_work(command, exec_time, u64::from(self.local_player));
    }

    /// Sorts the selected squads and removes the dead units, after the simulation step.
    pub(super) fn refresh_selection(&mut self) {
        let Some(world) = self.session.as_ref().and_then(ScenarioSession::world) else {
            return;
        };
        rebuild_selection(world, &self.catalog, &mut self.selection);
    }

    /// Draws the drag box of the selection in the player colour.
    pub(super) fn draw_drag_box(&self, ctx: &egui::Context) {
        let Some((start, end)) = self.mouse.drag_box(self.cursor) else {
            return;
        };
        // The input positions are physical pixels; egui uses points.
        let scale = ctx.pixels_per_point();
        let to_point = |pixel: Vec2| egui::pos2(pixel.x / scale, pixel.y / scale);
        let rect = egui::Rect::from_two_pos(to_point(start), to_point(end));
        let [red, green, blue] = self.drag_box_color;
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("client_drag_box"),
        ))
        .rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(
                DRAG_BOX_LINE_WIDTH,
                egui::Color32::from_rgb(red, green, blue),
            ),
            egui::StrokeKind::Inside,
        );
    }
}
