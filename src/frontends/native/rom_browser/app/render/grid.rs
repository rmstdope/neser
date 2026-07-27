use super::super::*;

impl RomBrowserApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_grid_egui(
        ui: &mut egui::Ui,
        entries: &[RomEntry],
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
        cols: usize,
        cover_w: f32,
        cover_h: f32,
        cell_h: f32,
        selected: usize,
        scroll_offset: f32,
        search_query: &str,
        no_roms_hint: &str,
    ) {
        if entries.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                if search_query.is_empty() {
                    ui.label(
                        egui::RichText::new(no_roms_hint)
                            .color(theme::DIM_TEXT)
                            .size(13.0),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!("No games match \"{search_query}\""))
                            .color(theme::DIM_TEXT)
                            .size(13.0),
                    );
                }
            });
            return;
        }

        let panel_rect = ui.available_rect_before_wrap();
        ui.allocate_rect(panel_rect, egui::Sense::hover());
        let painter = ui.painter_at(panel_rect);

        let origin = panel_rect.min;
        let rounding = egui::CornerRadius::same(theme::CORNER_RADIUS as u8);

        for (i, entry) in entries.iter().enumerate() {
            let row = (i / cols) as f32;
            let col = (i % cols) as f32;
            let x = origin.x + theme::GRID_PADDING + col * (cover_w + theme::GRID_SPACING);
            let y = origin.y + theme::GRID_PADDING + row * (cell_h + theme::GRID_SPACING)
                - scroll_offset;

            if y + cell_h < panel_rect.top() || y > panel_rect.bottom() {
                continue;
            }

            let cover_rect =
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(cover_w, cover_h));

            // Selection glow effect (drawn behind the image).
            if i == selected {
                let glow_rect = cover_rect.expand(4.0);
                painter.add(
                    egui::epaint::RectShape::filled(glow_rect, rounding, theme::SELECTION_COLOR)
                        .with_blur_width(theme::SELECTION_GLOW),
                );
            }

            // Cover art or placeholder with rounded corners.
            if let Some(game_id) = entry.metadata_game_id {
                if let Some(&(tex_id, tex_w, tex_h)) = tex_map.get(&game_id) {
                    // Preserve the image's actual aspect ratio within the cell.
                    let img_aspect = tex_w as f32 / tex_h.max(1) as f32;
                    let (draw_w, draw_h) = if img_aspect > cover_w / cover_h {
                        (cover_w, cover_w / img_aspect)
                    } else {
                        (cover_h * img_aspect, cover_h)
                    };
                    let draw_x = x + (cover_w - draw_w) / 2.0;
                    let draw_y = y + (cover_h - draw_h) / 2.0;
                    let img_rect = egui::Rect::from_min_size(
                        egui::pos2(draw_x, draw_y),
                        egui::vec2(draw_w, draw_h),
                    );
                    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                    painter.add(
                        egui::epaint::RectShape::filled(img_rect, rounding, egui::Color32::WHITE)
                            .with_texture(tex_id, uv),
                    );
                } else {
                    painter.rect_filled(cover_rect, rounding, theme::PLACEHOLDER_BG);
                    let max_chars = (cover_w / 8.0) as usize;
                    let short = Self::truncate_label(&entry.display_name, max_chars);
                    painter.text(
                        cover_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &short,
                        egui::FontId::new(10.0, egui::FontFamily::Proportional),
                        theme::DIM_TEXT,
                    );
                }
            } else {
                painter.rect_filled(cover_rect, rounding, theme::PLACEHOLDER_BG);
                let max_chars = (cover_w / 8.0) as usize;
                let short = Self::truncate_label(&entry.display_name, max_chars);
                painter.text(
                    cover_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &short,
                    egui::FontId::new(10.0, egui::FontFamily::Proportional),
                    theme::DIM_TEXT,
                );
            }

            // Favourite heart badge.
            if entry.is_favorite {
                painter.text(
                    egui::pos2(x + cover_w - 16.0, y + 6.0),
                    egui::Align2::LEFT_TOP,
                    "\u{2665}",
                    egui::FontId::new(14.0, egui::FontFamily::Proportional),
                    theme::FAVORITE_COLOR,
                );
            }
        }
    }

    pub(super) fn render_sidebar_egui(
        ui: &mut egui::Ui,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
    ) {
        // Fixed-height area for cover art so text position stays constant.
        let avail_w = ui.available_width();
        let art_area_h = theme::SIDEBAR_ART_HEIGHT;
        let (art_rect, _) =
            ui.allocate_exact_size(egui::vec2(avail_w, art_area_h), egui::Sense::hover());

        if let Some(game_id) = entry.metadata_game_id
            && let Some(&(tex_id, tex_w, tex_h)) = tex_map.get(&game_id)
        {
            let img_aspect = tex_w as f32 / tex_h.max(1) as f32;
            // Fit image within the fixed area, preserving aspect ratio.
            let (draw_w, draw_h) = if img_aspect > avail_w / art_area_h {
                (avail_w, avail_w / img_aspect)
            } else {
                (art_area_h * img_aspect, art_area_h)
            };
            let cx = art_rect.center().x;
            let cy = art_rect.center().y;
            let img_rect =
                egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(draw_w, draw_h));
            let rounding = egui::CornerRadius::same(theme::CORNER_RADIUS as u8);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().add(
                egui::epaint::RectShape::filled(img_rect, rounding, egui::Color32::WHITE)
                    .with_texture(tex_id, uv),
            );
        }
        ui.add_space(8.0);

        ui.label(
            egui::RichText::new(&entry.display_name)
                .color(theme::HEADER_TEXT)
                .size(20.0),
        );
        ui.add_space(6.0);

        // Metadata in a darker rounded frame.
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(22, 22, 28))
            .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS as u8))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                if !entry.genres.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("Genre: {}", entry.genres.join(", ")))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if let Some(ref date) = entry.release_date {
                    ui.label(
                        egui::RichText::new(format!("Released: {date}"))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if let Some(players) = entry.players {
                    ui.label(
                        egui::RichText::new(format!("Players: {players}"))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if let Some(ref rating) = entry.rating {
                    ui.label(
                        egui::RichText::new(format!("Rating: {rating}"))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                ui.label(
                    egui::RichText::new(format!("Mapper: {}", entry.mapper_label))
                        .color(theme::DIM_TEXT)
                        .size(15.0),
                );
                if let Some(ref crc) = entry.crc {
                    ui.label(
                        egui::RichText::new(format!("CRC: {crc}"))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if let Some(ref hw) = entry.hardware {
                    ui.label(
                        egui::RichText::new(format!("Hardware: {hw}"))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if let Some(file_name) = entry.path.file_name() {
                    ui.label(
                        egui::RichText::new(format!("File: {}", file_name.to_string_lossy()))
                            .color(theme::DIM_TEXT)
                            .size(15.0),
                    );
                }
                if entry.is_favorite {
                    ui.label(
                        egui::RichText::new("\u{2665} Favourite")
                            .color(theme::FAVORITE_COLOR)
                            .size(16.0),
                    );
                }
            });

        if let Some(ref overview) = entry.overview {
            ui.add_space(6.0);
            // Reserve space for the button legend at the bottom.
            let legend_reserve = 80.0;
            let max_desc_h = (ui.available_height() - legend_reserve).max(0.0);
            if max_desc_h > 30.0 {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(22, 22, 28))
                    .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS as u8))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        let font = egui::FontId::proportional(15.0);
                        let avail_w = ui.available_width();
                        // Inner margin eats into available width.
                        let text_w = avail_w.max(1.0);
                        let truncated = Self::truncate_text_to_height(
                            ui,
                            overview,
                            &font,
                            text_w,
                            max_desc_h - 20.0,
                        );
                        ui.label(
                            egui::RichText::new(truncated)
                                .color(theme::TEXT_COLOR)
                                .size(15.0),
                        );
                    });
            }
        }
    }
}
