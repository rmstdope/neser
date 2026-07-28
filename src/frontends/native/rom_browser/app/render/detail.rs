use super::super::*;

impl RomBrowserApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_detail_view_egui(
        ctx: &egui::Context,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
        screenshot_textures: &[(egui::TextureId, u32, u32)],
        screenshot_index: usize,
        display_w: f32,
        display_h: f32,
        controller_connected: bool,
    ) {
        // Dim background.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("detail_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(210),
        );

        let margin = 40.0;
        egui::Window::new("detail_view")
            .id(egui::Id::new("detail_overlay"))
            .fixed_pos(egui::pos2(margin, margin))
            .fixed_size(egui::vec2(
                display_w - margin * 2.0,
                display_h - margin * 2.0,
            ))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .show(ctx, |ui| {
                let frame = egui::Frame::new()
                    .fill(theme::SIDEBAR_BG)
                    .inner_margin(egui::Margin::same(24))
                    .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS as u8));
                frame.show(ui, |ui| {
                    // Title header.
                    ui.label(
                        egui::RichText::new(&entry.display_name)
                            .color(theme::HEADER_TEXT)
                            .size(30.0)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Three-column layout.
                    let avail = ui.available_size();
                    let col_gap = 16.0;
                    let left_frac = 0.30;
                    let mid_frac = 0.35;
                    // right gets the remainder
                    let left_w = avail.x * left_frac;
                    let mid_w = avail.x * mid_frac;
                    let right_w = avail.x - left_w - mid_w - col_gap * 2.0;
                    let panel_h = avail.y;

                    ui.horizontal(|ui| {
                        // ---- LEFT COLUMN: Cover art + metadata ----
                        ui.vertical(|ui| {
                            ui.set_width(left_w);
                            ui.set_max_height(panel_h);

                            // Cover art.
                            let art_max_h = panel_h * 0.65;
                            if let Some(game_id) = entry.metadata_game_id
                                && let Some(&(tex_id, tex_w, tex_h)) = tex_map.get(&game_id)
                            {
                                let img_aspect = tex_w as f32 / tex_h.max(1) as f32;
                                let art_h = (left_w / img_aspect).min(art_max_h);
                                let actual_w = art_h * img_aspect;
                                ui.add(
                                    egui::Image::from_texture(egui::load::SizedTexture::new(
                                        tex_id,
                                        egui::vec2(actual_w, art_h),
                                    ))
                                    .corner_radius(theme::CORNER_RADIUS),
                                );
                            } else {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(left_w, art_max_h * 0.7),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(theme::CORNER_RADIUS as u8),
                                    theme::PLACEHOLDER_BG,
                                );
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &entry.display_name,
                                    egui::FontId::proportional(17.0),
                                    theme::DIM_TEXT,
                                );
                            }

                            ui.add_space(12.0);

                            // Metadata below cover art.
                            let meta_font = egui::FontId::proportional(17.0);
                            if !entry.genres.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Genre: {}",
                                        entry.genres.join(", ")
                                    ))
                                    .color(theme::DIM_TEXT)
                                    .font(meta_font.clone()),
                                );
                            }
                            if let Some(ref date) = entry.release_date {
                                ui.label(
                                    egui::RichText::new(format!("Released: {date}"))
                                        .color(theme::DIM_TEXT)
                                        .font(meta_font.clone()),
                                );
                            }
                            if let Some(players) = entry.players {
                                ui.label(
                                    egui::RichText::new(format!("Players: {players}"))
                                        .color(theme::DIM_TEXT)
                                        .font(meta_font.clone()),
                                );
                            }
                            if let Some(ref rating) = entry.rating {
                                ui.label(
                                    egui::RichText::new(format!("Rating: {rating}"))
                                        .color(theme::DIM_TEXT)
                                        .font(meta_font.clone()),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("Mapper: {}", entry.mapper_label))
                                    .color(theme::DIM_TEXT)
                                    .font(meta_font.clone()),
                            );
                            if let Some(ref crc) = entry.crc {
                                ui.label(
                                    egui::RichText::new(format!("CRC: {crc}"))
                                        .color(theme::DIM_TEXT)
                                        .font(meta_font.clone()),
                                );
                            }
                            if let Some(ref hw) = entry.hardware {
                                ui.label(
                                    egui::RichText::new(format!("Hardware: {hw}"))
                                        .color(theme::DIM_TEXT)
                                        .font(meta_font.clone()),
                                );
                            }
                            if let Some(file_name) = entry.path.file_name() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "File: {}",
                                        file_name.to_string_lossy()
                                    ))
                                    .color(theme::DIM_TEXT)
                                    .font(meta_font),
                                );
                            }
                            if entry.is_favorite {
                                ui.label(
                                    egui::RichText::new("\u{2665} Favourite")
                                        .color(theme::FAVORITE_COLOR)
                                        .size(18.0),
                                );
                            }
                        });

                        ui.add_space(col_gap);

                        // ---- MIDDLE COLUMN: Screenshots stacked ----
                        ui.vertical(|ui| {
                            ui.set_width(mid_w);
                            ui.set_max_height(panel_h);

                            if screenshot_textures.is_empty() {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(mid_w, panel_h * 0.6),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(theme::CORNER_RADIUS as u8),
                                    theme::PLACEHOLDER_BG,
                                );
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "No screenshots",
                                    egui::FontId::proportional(18.0),
                                    theme::DIM_TEXT,
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("detail_screenshots")
                                    .max_height(panel_h)
                                    .show(ui, |ui| {
                                        for (i, &(tex_id, tex_w, tex_h)) in
                                            screenshot_textures.iter().enumerate()
                                        {
                                            let ss_aspect = tex_w as f32 / tex_h.max(1) as f32;
                                            let img_w = mid_w;
                                            let img_h = img_w / ss_aspect;
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(img_w, img_h),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().image(
                                                tex_id,
                                                rect,
                                                egui::Rect::from_min_max(
                                                    egui::pos2(0.0, 0.0),
                                                    egui::pos2(1.0, 1.0),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                            let idx =
                                                screenshot_index.min(screenshot_textures.len() - 1);
                                            if i == idx {
                                                ui.painter().rect_stroke(
                                                    rect.expand(2.0),
                                                    egui::CornerRadius::same(
                                                        theme::CORNER_RADIUS as u8,
                                                    ),
                                                    egui::Stroke::new(2.5, theme::SELECTION_COLOR),
                                                    egui::StrokeKind::Outside,
                                                );
                                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                            }
                                            ui.add_space(8.0);
                                        }
                                    });
                            }
                        });

                        ui.add_space(col_gap);

                        // ---- RIGHT COLUMN: Description + legend ----
                        ui.vertical(|ui| {
                            ui.set_width(right_w);
                            ui.set_max_height(panel_h);

                            if let Some(ref overview) = entry.overview {
                                egui::ScrollArea::vertical()
                                    .id_salt("detail_description")
                                    .max_height(panel_h - 60.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(overview)
                                                .color(theme::TEXT_COLOR)
                                                .size(18.0),
                                        );
                                    });
                            } else {
                                ui.label(
                                    egui::RichText::new("No description available.")
                                        .color(theme::DIM_TEXT)
                                        .size(18.0),
                                );
                            }

                            // Button legend at the bottom.
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                                Self::render_button_legend(
                                    ui,
                                    Self::legend_items(
                                        false,
                                        false,
                                        false,
                                        true,
                                        controller_connected,
                                    ),
                                );
                            });
                        });
                    });
                });
            });
    }
}
