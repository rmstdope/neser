use super::*;

impl RomBrowserApp {
    pub(super) fn render_frame(&mut self) {
        self.poll_catalog_loading();

        if matches!(self.catalog_state, CatalogState::Ready) {
            self.lazy_load_visible_textures();
        }

        let Some(ref mut gl) = self.gl else { return };

        if let CatalogState::Loading { ref progress, .. } = self.catalog_state {
            let progress_snapshot = progress.clone();
            let (display_w, display_h) = gl.logical_size();
            gl.run_frame(|ui| {
                ui.ctx().set_visuals(egui::Visuals {
                    dark_mode: true,
                    panel_fill: theme::BG_COLOR,
                    window_fill: theme::BG_COLOR,
                    ..egui::Visuals::dark()
                });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme::BG_COLOR))
                    .show_inside(ui, |ui| {
                        Self::render_loading_screen(
                            ui,
                            display_w,
                            display_h,
                            progress_snapshot.as_ref(),
                        );
                    });
            });
            return;
        }

        let (display_w, display_h) = gl.logical_size();
        // Measure true frame-to-frame time (includes vsync wait from last frame).
        let dt = self.last_render_instant.elapsed().as_secs_f32().min(0.1);
        self.last_render_instant = Instant::now();
        let diff = self.scroll_target - self.scroll_offset;
        if diff.abs() < 0.5 {
            self.scroll_offset = self.scroll_target;
        } else {
            self.scroll_offset += diff * (theme::SCROLL_SPEED * dt).min(1.0);
        }
        let scroll_offset = self.scroll_offset;

        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let cover_h = cover_w / theme::COVER_ASPECT;

        let filtered_count = self.filtered_indices.len();
        let total_count = self.catalog.len();
        let selected = self.selected_index;

        let tex_map: HashMap<i64, (egui::TextureId, u32, u32)> = self
            .catalog
            .iter()
            .filter_map(|e| {
                let game_id = e.metadata_game_id?;
                let tex = gl.get_texture(&TextureKey::CoverArt(game_id))?;
                let [width, height] = tex.size();
                Some((game_id, (tex.egui_id, width, height)))
            })
            .collect();

        let display_entries: Vec<RomEntry> = self
            .filtered_indices
            .iter()
            .map(|&idx| self.catalog[idx].clone())
            .collect();

        let search_active = self.search_active;
        let search_query = self.search_query.clone();
        let no_roms_hint = self.no_roms_hint.clone();
        let controller_connected = self
            .gilrs
            .as_ref()
            .is_some_and(|g| g.gamepads().next().is_some());
        let filter_panel_active = self.filter_panel_active;
        let filter_panel_cursor = self.filter_panel_cursor;
        let filter_panel_column = self.filter_panel_column;
        let active_platform = self.active_platform;
        let min_players_filter = self.min_players_filter;

        // Animate filter panel slide with fixed-rate stepping (frame-rate independent).
        let anim_target = if self.filter_panel_active { 1.0 } else { 0.0 };
        let anim_step = dt / 0.30; // complete in ~300ms
        if self.filter_panel_anim < anim_target {
            self.filter_panel_anim = (self.filter_panel_anim + anim_step).min(anim_target);
        } else if self.filter_panel_anim > anim_target {
            self.filter_panel_anim = (self.filter_panel_anim - anim_step).max(anim_target);
        }
        let filter_panel_anim = self.filter_panel_anim;

        // Animate search panel slide with the same timing.
        let search_anim_target = if self.search_active { 1.0 } else { 0.0 };
        if self.search_anim < search_anim_target {
            self.search_anim = (self.search_anim + anim_step).min(search_anim_target);
        } else if self.search_anim > search_anim_target {
            self.search_anim = (self.search_anim - anim_step).max(search_anim_target);
        }
        let search_anim = self.search_anim;
        let search_kb_row = self.search_kb_row;
        let search_kb_col = self.search_kb_col;

        let available_genres = self.available_genres.clone();
        let active_genres = self.active_genres.clone();
        let detail_view_active = self.detail_view_active;
        if detail_view_active {
            self.advance_screenshot_auto_scroll();
        }
        let detail_screenshot_index = self.detail_screenshot_index;
        let show_favorites_only = self.show_favorites_only;
        let selected_entry: Option<RomEntry> = self
            .filtered_indices
            .get(selected)
            .and_then(|&idx| self.catalog.get(idx))
            .cloned();

        // Load screenshot textures on demand when detail view is open.
        let mut screenshot_textures: Vec<(egui::TextureId, u32, u32)> = Vec::new();
        if detail_view_active
            && let Some(ref entry) = selected_entry
            && let Some(game_id) = entry.metadata_game_id
        {
            let gl = self.gl.as_mut().unwrap();
            for (i, path) in entry.screenshot_paths.iter().enumerate() {
                let key = TextureKey::Screenshot(game_id, i);
                if let Some(tex) = gl.get_texture(&key) {
                    let [width, height] = tex.size();
                    screenshot_textures.push((tex.egui_id, width, height));
                } else if let Some(tex) = gl.load_texture_from_file(key, path) {
                    let [width, height] = tex.size();
                    screenshot_textures.push((tex.egui_id, width, height));
                }
            }
        }
        let gl = self.gl.as_mut().unwrap();

        let genre_suffix = if active_genres.is_empty() {
            String::new()
        } else {
            format!("  [{}]", active_genres.join(", "))
        };
        let fav_suffix = if show_favorites_only {
            "  \u{2665}"
        } else {
            ""
        };
        let header_text = if search_query.is_empty() {
            format!(
                "NESER ROM Browser \u{2014} {filtered_count}/{total_count} games{genre_suffix}{fav_suffix}"
            )
        } else {
            format!(
                "NESER ROM Browser \u{2014} {filtered_count}/{total_count} (search: \"{search_query}\"){genre_suffix}{fav_suffix}"
            )
        };

        gl.run_frame(|ui| {
            let mut visuals = egui::Visuals::dark();
            visuals.dark_mode = true;
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            visuals.window_fill = egui::Color32::TRANSPARENT;
            visuals.extreme_bg_color = egui::Color32::from_rgb(10, 10, 15);
            visuals.faint_bg_color = egui::Color32::from_rgb(20, 20, 30);
            // Remove all widget/panel borders for a clean look.
            visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
            visuals.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75));
            visuals.widgets.inactive.fg_stroke =
                egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 180, 195));
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, theme::SELECTION_COLOR);
            visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, theme::SELECTION_COLOR);
            visuals.widgets.active.fg_stroke = egui::Stroke::new(2.0, theme::SELECTION_COLOR);
            visuals.selection.bg_fill = theme::SELECTION_COLOR;
            visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            // Rounded corners on widgets.
            visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
            visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
            visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);
            ui.ctx().set_visuals(visuals);
            ui.ctx().global_style_mut(|s| {
                s.text_styles.insert(
                    egui::TextStyle::Body,
                    egui::FontId::new(14.0, egui::FontFamily::Proportional),
                );
                s.text_styles.insert(
                    egui::TextStyle::Small,
                    egui::FontId::new(11.0, egui::FontFamily::Proportional),
                );
                s.text_styles.insert(
                    egui::TextStyle::Heading,
                    egui::FontId::new(20.0, egui::FontFamily::Monospace),
                );
            });

            // Full-window gradient background.
            let full_rect = ui.max_rect();
            let painter = ui.painter();
            painter.add(egui::Shape::gradient_rect(
                full_rect,
                egui::epaint::Direction::TopDown,
                [theme::BG_COLOR, theme::BG_COLOR_LIGHT],
            ));

            // Right panel is transparent; the floating dark sidebar is drawn inside.
            let sidebar_panel_frame = egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(0))
                .stroke(egui::Stroke::NONE);
            egui::Panel::right("sidebar")
                .exact_size(sidebar_w)
                .resizable(false)
                .frame(sidebar_panel_frame)
                .show_inside(ui, |ui| {
                    // Floating dark rectangle with margin inside the transparent panel.
                    egui::Frame::new()
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::same(12))
                        .outer_margin(egui::Margin {
                            left: 16,
                            right: 16,
                            top: 16,
                            bottom: 16,
                        })
                        .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS as u8))
                        .stroke(egui::Stroke::NONE)
                        .show(ui, |ui| {
                            if let Some(ref entry) = selected_entry {
                                Self::render_sidebar_egui(ui, entry, &tex_map);
                            }

                            // Button legend at the bottom of the sidebar.
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                                Self::render_button_legend(
                                    ui,
                                    Self::legend_items(
                                        search_active,
                                        filter_panel_active,
                                        detail_view_active,
                                        controller_connected,
                                    ),
                                );
                            });
                        });
                });

            let bar_frame = egui::Frame::new()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(8))
                .stroke(egui::Stroke::NONE);
            egui::Panel::top("header")
                .exact_size(theme::HEADER_HEIGHT)
                .frame(bar_frame)
                .show_inside(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&header_text)
                            .color(theme::HEADER_TEXT)
                            .size(20.0),
                    );
                });

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                )
                .show_inside(ui, |ui| {
                    Self::render_grid_egui(
                        ui,
                        &display_entries,
                        &tex_map,
                        cols,
                        cover_w,
                        cover_h,
                        cell_h,
                        selected,
                        scroll_offset,
                        &search_query,
                        &no_roms_hint,
                    );
                });

            if search_active || search_anim > 0.0 {
                Self::render_search_panel_egui(
                    ui.ctx(),
                    &search_query,
                    filtered_count,
                    search_kb_row,
                    search_kb_col,
                    search_anim,
                    display_w,
                    display_h,
                    controller_connected,
                );
            }
            if filter_panel_active || filter_panel_anim > 0.0 {
                Self::render_filter_panel_egui(
                    ui.ctx(),
                    &available_genres,
                    &active_genres,
                    active_platform,
                    min_players_filter,
                    show_favorites_only,
                    controller_connected,
                    filter_panel_cursor,
                    filter_panel_column,
                    filter_panel_anim,
                    display_w,
                    display_h,
                );
            }
            if detail_view_active && let Some(ref entry) = selected_entry {
                Self::render_detail_view_egui(
                    ui.ctx(),
                    entry,
                    &tex_map,
                    &screenshot_textures,
                    detail_screenshot_index,
                    display_w,
                    display_h,
                    controller_connected,
                );
            }
        });
    }

    fn render_loading_screen(
        ui: &mut egui::Ui,
        display_w: f32,
        display_h: f32,
        progress: Option<&EnrichmentProgress>,
    ) {
        let bar_w = (display_w * 0.55).max(320.0);

        ui.vertical_centered(|ui| {
            ui.add_space(display_h / 2.0 - 42.0);
            ui.label(
                egui::RichText::new("NESER ROM Browser")
                    .color(theme::HEADER_TEXT)
                    .size(20.0),
            );
            ui.add_space(8.0);

            if let Some(p) = progress {
                let fraction = if p.total > 0 {
                    p.current as f32 / p.total as f32
                } else {
                    0.0
                };
                let phase_label = match p.phase {
                    EnrichmentPhase::MatchingMetadata => "Matching metadata",
                    EnrichmentPhase::DownloadingImages => "Downloading cover art",
                };
                ui.label(
                    egui::RichText::new(format!("{phase_label}: {} / {}", p.current, p.total))
                        .color(theme::DIM_TEXT)
                        .size(13.0),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(bar_w)
                        .fill(egui::Color32::from_rgb(100, 140, 220)),
                );
                ui.add_space(4.0);
                let max_chars = (bar_w / 8.0) as usize;
                let title = Self::truncate_label(&p.game_title, max_chars);
                ui.label(
                    egui::RichText::new(&title)
                        .color(theme::DIM_TEXT)
                        .size(12.0),
                );
            } else {
                ui.label(
                    egui::RichText::new("Scanning ROM library...")
                        .color(theme::DIM_TEXT)
                        .size(13.0),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .desired_width(bar_w)
                        .fill(egui::Color32::from_rgb(60, 80, 120)),
                );
            }
        });
    }

    /// Truncate text with '...' so its laid-out height fits within `max_height`.
    fn truncate_text_to_height(
        ui: &egui::Ui,
        text: &str,
        font: &egui::FontId,
        wrap_width: f32,
        max_height: f32,
    ) -> String {
        // First check if the full text fits.
        let full_galley =
            ui.painter()
                .layout(text.to_owned(), font.clone(), theme::TEXT_COLOR, wrap_width);
        if full_galley.size().y <= max_height {
            return text.to_owned();
        }

        // Binary search for the longest prefix that fits with "...".
        let chars: Vec<char> = text.chars().collect();
        let mut lo = 0_usize;
        let mut hi = chars.len();
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate: String = chars[..mid].iter().collect();
            let candidate_text = format!("{candidate}...");
            let galley =
                ui.painter()
                    .layout(candidate_text, font.clone(), theme::TEXT_COLOR, wrap_width);
            if galley.size().y <= max_height {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }

        if lo == 0 {
            "...".to_owned()
        } else {
            let prefix: String = chars[..lo].iter().collect();
            format!("{prefix}...")
        }
    }

    /// Choose the button legend for the current UI mode: gamepad button
    /// labels when a controller is connected, keyboard keys otherwise.
    pub(in super::super) fn legend_items(
        search_active: bool,
        filter_panel_active: bool,
        detail_view_active: bool,
        controller_connected: bool,
    ) -> &'static [(&'static str, &'static str)] {
        if search_active {
            if controller_connected {
                &[("A", "Select"), ("Start", "Close")]
            } else {
                &[("Tab", "Close"), ("Enter", "Select"), ("Type", "Search")]
            }
        } else if filter_panel_active {
            if controller_connected {
                &[("↑↓", "Navigate"), ("A", "Toggle"), ("B", "Close")]
            } else {
                &[("↑↓", "Navigate"), ("Enter", "Toggle"), ("Esc", "Close")]
            }
        } else if detail_view_active {
            if controller_connected {
                &[("A", "Launch"), ("Y", "Fav"), ("B", "Back")]
            } else {
                &[("Enter", "Launch"), ("Space", "Fav"), ("Esc", "Back")]
            }
        } else if controller_connected {
            &[
                ("A", "Details"),
                ("B", "Filter"),
                ("Select", "Favorite"),
                ("Start", "Search"),
            ]
        } else {
            &[
                ("Enter", "Details"),
                ("Esc", "Filter"),
                ("Space", "Favorite"),
                ("Tab", "Search"),
            ]
        }
    }

    /// Render a row of pill-shaped button prompts (e.g., `[A] Launch`).
    fn render_button_legend(ui: &mut egui::Ui, items: &[(&str, &str)]) {
        let pill_font = egui::FontId::new(14.0, egui::FontFamily::Monospace);
        let label_font = egui::FontId::new(17.0, egui::FontFamily::Monospace);
        let pill_h = 24.0_f32;
        let outer_h = 30.0_f32;
        let pill_pad_x = 7.0_f32;
        let item_gap = 6.0_f32;
        let label_gap = 8.0_f32;
        let outer_pad_x = 8.0_f32;
        let pill_rounding = egui::CornerRadius::same(12);
        let outer_rounding = egui::CornerRadius::same(15);
        let avail_w = ui.available_width();

        // Lay items out in rows that wrap when they exceed available width.
        let mut cursor_x = 0.0_f32;
        let mut rows: Vec<Vec<(f32, f32, &str, &str)>> = vec![Vec::new()];

        for &(btn, label) in items {
            let btn_galley = ui.painter().layout_no_wrap(
                btn.to_owned(),
                pill_font.clone(),
                egui::Color32::WHITE,
            );
            let label_galley = ui.painter().layout_no_wrap(
                label.to_owned(),
                label_font.clone(),
                egui::Color32::WHITE,
            );
            let pill_w = btn_galley.size().x + pill_pad_x * 2.0;
            let label_w = label_galley.size().x;
            let outer_w = outer_pad_x + pill_w + label_gap + label_w + outer_pad_x;

            if !rows.last().unwrap().is_empty() && cursor_x + outer_w > avail_w {
                rows.push(Vec::new());
                cursor_x = 0.0;
            }
            rows.last_mut().unwrap().push((pill_w, outer_w, btn, label));
            cursor_x += outer_w + item_gap;
        }

        // Render from bottom up (bottom_up layout reverses order).
        for row in rows.iter().rev() {
            let (_, row_rect) = ui.allocate_space(egui::vec2(avail_w, outer_h + 4.0));
            let mut x = row_rect.left();
            let cy = row_rect.center().y;

            for &(pill_w, outer_w, btn, label) in row {
                // Outer wrapper pill.
                let outer_rect = egui::Rect::from_min_size(
                    egui::pos2(x, cy - outer_h / 2.0),
                    egui::vec2(outer_w, outer_h),
                );
                ui.painter()
                    .rect_filled(outer_rect, outer_rounding, theme::LEGEND_ITEM_BG);

                // Inner outlined button pill.
                let pill_x = x + outer_pad_x;
                let pill_rect = egui::Rect::from_min_size(
                    egui::pos2(pill_x, cy - pill_h / 2.0),
                    egui::vec2(pill_w, pill_h),
                );
                let pill_color = Self::button_pill_color(btn);
                ui.painter().rect_stroke(
                    pill_rect,
                    pill_rounding,
                    egui::Stroke::new(1.5, pill_color),
                    egui::StrokeKind::Outside,
                );
                ui.painter().text(
                    pill_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    btn,
                    pill_font.clone(),
                    pill_color,
                );

                // Action label.
                let label_x = pill_x + pill_w + label_gap;
                ui.painter().text(
                    egui::pos2(label_x, cy),
                    egui::Align2::LEFT_CENTER,
                    label,
                    label_font.clone(),
                    theme::BUTTON_PILL_LABEL,
                );

                x += outer_w + item_gap;
            }
        }
    }

    /// Get the outline colour for a button pill based on standard gamepad colours.
    pub(in super::super) fn button_pill_color(btn: &str) -> egui::Color32 {
        match btn {
            "A" => theme::BUTTON_COLOR_A,
            "B" => theme::BUTTON_COLOR_B,
            "X" => theme::BUTTON_COLOR_X,
            "Y" => theme::BUTTON_COLOR_Y,
            "Select" | "Start" => egui::Color32::from_rgb(160, 160, 175),
            // Keyboard keys and other labels: white for contrast against
            // the grey legend item background.
            _ => theme::BUTTON_PILL_TEXT,
        }
    }

    /// Render a styled section header label with a subtle background pill.
    fn render_filter_section_header(ui: &mut egui::Ui, text: &str, bg: egui::Color32) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), bg);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(200, 200, 215),
        );
    }

    /// Paint a small triangle cursor arrow, or an equal-width spacer.
    fn paint_cursor_arrow(ui: &mut egui::Ui, is_cursor: bool, color: egui::Color32) {
        let size = 14.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size + 4.0), egui::Sense::hover());
        if is_cursor {
            let center = rect.center();
            let half = size * 0.35;
            let points = vec![
                egui::pos2(center.x - half * 0.5, center.y - half),
                egui::pos2(center.x + half, center.y),
                egui::pos2(center.x - half * 0.5, center.y + half),
            ];
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
        }
    }

    /// Truncate a label to fit within `max_chars` characters, adding "…" if needed.
    fn truncate_label(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            text.to_string()
        } else {
            let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
            format!("{truncated}…")
        }
    }
}

mod detail;
mod filter;
mod grid;
mod search;
