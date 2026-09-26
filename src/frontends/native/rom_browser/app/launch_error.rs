//! The strip above the game grid that says why the last game could not start (nr-auv).
//!
//! It shows the reason for every failed start, one at a time: a newer failure replaces it, a
//! game that starts clears it, and a mouse user can close it with its ✕. It never takes a key
//! or button press: the browser's keyboard and gamepad handling does not know it exists.

use super::*;

impl RomBrowserApp {
    /// Shows `lines` (a bold first line, then the reason) in the strip, replacing any earlier
    /// one. The browser comes back on the grid, with the same game selected, so the strip is
    /// in view.
    pub fn set_launch_error(&mut self, lines: (String, String)) {
        self.launch_error = Some(lines);
        self.detail_view_active = false;
    }

    /// Hides the strip (a game started, or its ✕ was clicked).
    pub fn clear_launch_error(&mut self) {
        self.launch_error = None;
        self.launch_strip_height = 0.0;
    }

    /// Records the strip's height as just drawn; when it changed (the strip appeared, wrapped
    /// differently or went away) the selected game is scrolled back into view, so the strip
    /// never hides it.
    pub(super) fn apply_launch_strip_height(
        &mut self,
        height: f32,
        display_w: f32,
        display_h: f32,
    ) {
        if height == self.launch_strip_height {
            return;
        }
        self.launch_strip_height = height;
        self.scroll_to_show_selected(display_w, display_h);
    }

    /// The height the game grid has to show games in, below the header and the strip.
    pub(super) fn grid_height(&self, display_h: f32) -> f32 {
        display_h - theme::HEADER_HEIGHT - self.launch_strip_height
    }

    /// Draws the strip as a top panel below the header, returning its height and whether its
    /// ✕ was clicked. The text wraps, so a narrow window makes the strip taller and moves the
    /// grid down rather than cutting anything off.
    pub(super) fn render_launch_error_strip(
        ui: &mut egui::Ui,
        (title, reason): &(String, String),
    ) -> (f32, bool) {
        let mut closed = false;
        let frame = egui::Frame::new()
            .fill(theme::STRIP_BG)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .stroke(egui::Stroke::NONE);
        let panel = egui::Panel::top("launch_error")
            .frame(frame)
            .show_inside(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let close = egui::Button::new(
                        egui::RichText::new("✕").color(theme::STRIP_TEXT).size(16.0),
                    )
                    .frame(false);
                    if ui.add(close).clicked() {
                        closed = true;
                    }
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(title)
                                    .strong()
                                    .color(theme::STRIP_TEXT)
                                    .size(15.0),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(reason)
                                    .color(theme::STRIP_TEXT)
                                    .size(14.0),
                            )
                            .wrap(),
                        );
                    });
                });
            });
        (panel.response.rect.height(), closed)
    }
}
