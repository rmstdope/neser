use super::super::*;

impl RomBrowserApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_search_panel_egui(
        ctx: &egui::Context,
        query: &str,
        count: usize,
        kb_row: usize,
        kb_col: usize,
        anim: f32,
        display_w: f32,
        display_h: f32,
        controller_connected: bool,
    ) {
        // Dim background with animated alpha.
        let dim_alpha = (180.0 * anim) as u8;
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("search_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(dim_alpha),
        );

        let panel_w = 560.0_f32.min(display_w * 0.55);
        let panel_x = -panel_w + panel_w * anim;
        let panel_bg = egui::Color32::from_rgba_premultiplied(28, 28, 38, 140);
        let accent = theme::SELECTION_COLOR;
        let corner_r = egui::CornerRadius::same(12);

        // Panel background with rounded right corners and shadow.
        let panel_rect =
            egui::Rect::from_min_size(egui::pos2(panel_x, 0.0), egui::vec2(panel_w, display_h));
        let bg_painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("search_panel_bg"),
        ));
        bg_painter.rect_filled(
            panel_rect.expand(6.0),
            corner_r,
            egui::Color32::from_black_alpha(60),
        );
        bg_painter.rect_filled(panel_rect, corner_r, panel_bg);

        // Content area.
        egui::Area::new(egui::Id::new("search_panel_content"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(panel_x + 20.0, 30.0))
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(panel_rect);
                let content_w = panel_w - 40.0;
                ui.set_max_width(content_w);

                // Title.
                ui.label(
                    egui::RichText::new("Search")
                        .color(egui::Color32::WHITE)
                        .size(27.0),
                );
                ui.add_space(12.0);

                // Search query bar.
                let query_display = if query.is_empty() {
                    "Type to search...".to_string()
                } else {
                    format!("{query}|")
                };
                let query_color = if query.is_empty() {
                    egui::Color32::from_rgb(120, 120, 140)
                } else {
                    egui::Color32::WHITE
                };
                let bar_rect = ui.available_rect_before_wrap();
                let bar_rect = egui::Rect::from_min_size(bar_rect.min, egui::vec2(content_w, 44.0));
                ui.painter().rect_filled(
                    bar_rect,
                    egui::CornerRadius::same(8),
                    egui::Color32::from_rgb(36, 36, 48),
                );
                ui.painter().rect_stroke(
                    bar_rect,
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(1.5_f32, accent),
                    egui::StrokeKind::Outside,
                );
                let text_pos = bar_rect.min + egui::vec2(14.0, 12.0);
                ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    &query_display,
                    egui::FontId::proportional(20.0),
                    query_color,
                );
                ui.advance_cursor_after_rect(bar_rect);
                ui.add_space(6.0);

                // Match count.
                ui.label(
                    egui::RichText::new(format!("{count} matches"))
                        .color(egui::Color32::from_rgb(160, 160, 175))
                        .size(14.0),
                );
                ui.add_space(16.0);

                // On-screen keyboard.
                let key_size = ((content_w - 9.0 * 6.0) / 10.0).min(44.0);
                let key_spacing = 6.0;
                let keyboard_w = 10.0 * key_size + 9.0 * key_spacing;

                for (row_idx, row) in Self::SEARCH_KB_ROWS.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // Center each row.
                        let row_w =
                            row.len() as f32 * key_size + (row.len() as f32 - 1.0) * key_spacing;
                        let indent = (keyboard_w - row_w) / 2.0;
                        ui.add_space(indent);

                        for (col_idx, &ch) in row.iter().enumerate() {
                            let is_selected = row_idx == kb_row && col_idx == kb_col;
                            let key_rect = ui.available_rect_before_wrap();
                            let key_rect = egui::Rect::from_min_size(
                                key_rect.min,
                                egui::vec2(key_size, key_size),
                            );

                            let (bg, fg, stroke) = if is_selected {
                                (
                                    accent,
                                    egui::Color32::WHITE,
                                    egui::Stroke::new(2.0_f32, accent),
                                )
                            } else {
                                (
                                    egui::Color32::from_rgb(44, 44, 58),
                                    egui::Color32::from_rgb(200, 200, 215),
                                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 60, 75)),
                                )
                            };

                            ui.painter()
                                .rect_filled(key_rect, egui::CornerRadius::same(6), bg);
                            ui.painter().rect_stroke(
                                key_rect,
                                egui::CornerRadius::same(6),
                                stroke,
                                egui::StrokeKind::Outside,
                            );

                            let label = match ch {
                                ' ' => "SPC".to_string(),
                                '\u{232B}' => "DEL".to_string(),
                                '\u{21B5}' => "OK".to_string(),
                                _ => ch.to_string(),
                            };
                            let font_size = match ch {
                                ' ' | '\u{232B}' | '\u{21B5}' => 12.0,
                                _ => 18.0,
                            };
                            ui.painter().text(
                                key_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &label,
                                egui::FontId::proportional(font_size),
                                fg,
                            );

                            ui.advance_cursor_after_rect(key_rect);
                            if col_idx + 1 < row.len() {
                                ui.add_space(key_spacing);
                            }
                        }
                    });
                    ui.add_space(key_spacing);
                }

                // Footer button legend.
                ui.add_space(12.0);
                Self::render_button_legend(
                    ui,
                    Self::legend_items(true, false, false, controller_connected),
                );
            });
    }
}
