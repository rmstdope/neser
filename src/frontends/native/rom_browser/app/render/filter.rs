use super::super::*;

impl RomBrowserApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_filter_panel_egui(
        ctx: &egui::Context,
        available_genres: &[String],
        active_genres: &[String],
        active_platform: Option<Platform>,
        min_players_filter: Option<u32>,
        cursor: usize,
        column: usize,
        anim: f32,
        display_w: f32,
        display_h: f32,
    ) {
        // Dim background with animated alpha.
        let dim_alpha = (180.0 * anim) as u8;
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("filter_panel_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(dim_alpha),
        );

        let panel_w = 600.0_f32.min(display_w * 0.6);
        // Slide in from the left: at anim=0 fully off-screen, at anim=1 left-aligned.
        let panel_x = -panel_w + panel_w * anim;

        // Panel background colors.
        let panel_bg = egui::Color32::from_rgba_premultiplied(28, 28, 38, 140);
        let section_bg = egui::Color32::from_rgba_premultiplied(36, 36, 48, 140);
        let accent = theme::SELECTION_COLOR;
        let corner_r = egui::CornerRadius::same(12);

        // Paint the full-height panel background with rounded right corners.
        let panel_rect =
            egui::Rect::from_min_size(egui::pos2(panel_x, 0.0), egui::vec2(panel_w, display_h));
        let panel_painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("filter_panel_bg"),
        ));
        // Shadow.
        panel_painter.rect_filled(
            panel_rect.expand(4.0),
            egui::CornerRadius {
                nw: 0,
                ne: 16,
                se: 16,
                sw: 0,
            },
            egui::Color32::from_black_alpha(80),
        );
        // Main panel.
        panel_painter.rect_filled(
            panel_rect,
            egui::CornerRadius {
                nw: 0,
                ne: 12,
                se: 12,
                sw: 0,
            },
            panel_bg,
        );

        // Content area: clips to panel bounds so text doesn't overflow.
        let content_rect = egui::Rect::from_min_size(
            egui::pos2(panel_x + 24.0, 32.0),
            egui::vec2(panel_w - 48.0, display_h - 64.0),
        );
        egui::Area::new(egui::Id::new("filter_panel_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(content_rect.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_width(content_rect.width());
                ui.set_height(content_rect.height());
                ui.set_clip_rect(panel_rect);

                // Title.
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Filters")
                        .color(theme::HEADER_TEXT)
                        .size(27.0)
                        .strong(),
                );
                ui.add_space(12.0);

                // Thin accent line.
                let line_rect = ui.available_rect_before_wrap();
                let line_y = line_rect.min.y;
                ui.painter().line_segment(
                    [
                        egui::pos2(line_rect.min.x, line_y),
                        egui::pos2(line_rect.min.x + panel_w - 48.0, line_y),
                    ],
                    egui::Stroke::new(1.0, accent.linear_multiply(0.5)),
                );
                ui.add_space(16.0);

                // Three-column layout with custom widths: Platform(25%) | Players(20%) | Genre(55%)
                let total_w = ui.available_width();
                let spacing = ui.spacing().item_spacing.x;
                let usable = total_w - spacing * 2.0;
                let col0_w = usable * 0.25;
                let col1_w = usable * 0.20;
                let col2_w = usable * 0.55;
                let top = ui.cursor().min;
                let col_h = ui.available_height();

                let col0_rect = egui::Rect::from_min_size(top, egui::vec2(col0_w, col_h));
                let col1_rect = egui::Rect::from_min_size(
                    egui::pos2(top.x + col0_w + spacing, top.y),
                    egui::vec2(col1_w, col_h),
                );
                let col2_rect = egui::Rect::from_min_size(
                    egui::pos2(top.x + col0_w + spacing + col1_w + spacing, top.y),
                    egui::vec2(col2_w, col_h),
                );

                // ── Column 0: Platform ──
                ui.scope_builder(egui::UiBuilder::new().max_rect(col0_rect), |ui| {
                    Self::render_filter_section_header(ui, "PLATFORM", section_bg);
                    ui.add_space(8.0);

                    for (i, plat) in Self::PLATFORMS.iter().enumerate() {
                        let is_active = active_platform == Some(*plat);
                        let is_cursor = column == 0 && i == cursor;

                        let item_rect = ui
                            .horizontal(|ui| {
                                Self::paint_cursor_arrow(ui, is_cursor, accent);
                                let _ = ui
                                    .radio(is_active, egui::RichText::new(plat.label()).size(20.0));
                            })
                            .response
                            .rect;

                        if is_cursor {
                            ui.painter().rect_filled(
                                item_rect.expand2(egui::vec2(4.0, 1.0)),
                                corner_r,
                                accent.linear_multiply(0.12),
                            );
                        }
                    }

                    // Legend at bottom of Platform column.
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        ui.add_space(8.0);
                        Self::render_button_legend(ui, &[("A", "Select"), ("B", "Close")]);
                    });
                });

                // ── Column 1: Players ──
                ui.scope_builder(egui::UiBuilder::new().max_rect(col1_rect), |ui| {
                    Self::render_filter_section_header(ui, "PLAYERS", section_bg);
                    ui.add_space(8.0);

                    for (i, (value, label)) in Self::PLAYER_OPTIONS.iter().enumerate() {
                        let is_active = min_players_filter == *value;
                        let is_cursor = column == 1 && i == cursor;

                        let item_rect = ui
                            .horizontal(|ui| {
                                Self::paint_cursor_arrow(ui, is_cursor, accent);
                                let _ = ui.radio(is_active, egui::RichText::new(*label).size(20.0));
                            })
                            .response
                            .rect;

                        if is_cursor {
                            ui.painter().rect_filled(
                                item_rect.expand2(egui::vec2(4.0, 1.0)),
                                corner_r,
                                accent.linear_multiply(0.12),
                            );
                        }
                    }
                });

                // ── Column 2: Genre ──
                ui.scope_builder(egui::UiBuilder::new().max_rect(col2_rect), |ui| {
                    Self::render_filter_section_header(ui, "GENRE", section_bg);
                    ui.add_space(8.0);

                    let col_w = ui.available_width();
                    let max_chars = ((col_w - 60.0) / 11.0).max(5.0) as usize;

                    egui::ScrollArea::vertical()
                        .id_salt("genre_scroll")
                        .max_height(ui.available_height() - 8.0)
                        .show(ui, |ui| {
                            for (i, genre) in available_genres.iter().enumerate() {
                                let is_active = active_genres.contains(genre);
                                let is_cursor = column == 2 && i == cursor;

                                let display_name = Self::truncate_label(genre, max_chars);

                                let mut checked = is_active;
                                let item_rect = ui
                                    .horizontal(|ui| {
                                        Self::paint_cursor_arrow(ui, is_cursor, accent);
                                        ui.checkbox(
                                            &mut checked,
                                            egui::RichText::new(&display_name).size(20.0),
                                        );
                                    })
                                    .response
                                    .rect;

                                if is_cursor {
                                    ui.painter().rect_filled(
                                        item_rect.expand2(egui::vec2(4.0, 1.0)),
                                        corner_r,
                                        accent.linear_multiply(0.12),
                                    );
                                    ui.scroll_to_rect(item_rect, Some(egui::Align::Center));
                                }
                            }
                        });
                });
            });
    }
}
