//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! GL-backed window with egui rendering and accepts a ROM selection that
//! transitions the application into emulation mode.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use super::renderer::{BrowserGl, TextureKey};
use super::theme;
use crate::platform::app_context::SharedAppContext;
use crate::platform::catalog::RomEntry;
use crate::platform::catalog::favorites::Favorites;

use crate::platform::catalog::EnrichmentPhase;
use crate::platform::catalog::EnrichmentProgress;

/// Messages sent from the background catalog loading thread.
enum CatalogMessage {
    Progress(EnrichmentProgress),
    Done(Vec<RomEntry>),
}

/// Tracks the catalog loading state.
enum CatalogState {
    /// Not started yet.
    Idle,
    /// Background thread is loading and enriching the catalog.
    Loading {
        receiver: mpsc::Receiver<CatalogMessage>,
        progress: Option<EnrichmentProgress>,
    },
    /// Catalog is loaded and ready.
    Ready,
}

/// Result from running the ROM browser.
#[derive(Debug)]
pub enum BrowserResult {
    /// User selected a ROM to launch.
    RomSelected(PathBuf),
    /// User closed the browser window without selecting.
    Closed,
}

/// ROM browser winit application.
///
/// Manages the browser window with GL rendering and handles user interactions
/// until a ROM is selected or the window is closed.
pub struct RomBrowserApp {
    app_context: SharedAppContext,
    gl: Option<BrowserGl>,
    result: BrowserResult,
    default_width: u32,
    default_height: u32,
    fullscreen: bool,
    catalog: Vec<RomEntry>,
    /// Indices into `catalog` that match the current filter (search + genre).
    filtered_indices: Vec<usize>,
    selected_index: usize,
    /// Current scroll offset (logical pixels from top).
    scroll_offset: f32,
    /// Target scroll offset for smooth scrolling.
    scroll_target: f32,
    /// Tracks whether cover art textures have been loaded.
    textures_loaded: bool,
    /// Search overlay state.
    search_active: bool,
    search_query: String,
    /// Genre filter overlay state.
    genre_filter_active: bool,
    /// All available genres (collected from catalog).
    available_genres: Vec<String>,
    /// Currently active genre filters (genre names that must match).
    active_genres: Vec<String>,
    /// Cursor position in the genre filter list.
    genre_cursor: usize,
    /// Detail view overlay active.
    detail_view_active: bool,
    /// Persistent favorites manager.
    favorites: Favorites,
    /// When true, show only favorited ROMs.
    show_favorites_only: bool,
    /// Tracks catalog loading progress.
    catalog_state: CatalogState,
    /// Tracks current modifier key state.
    modifiers: winit::keyboard::ModifiersState,
}

impl RomBrowserApp {
    /// Create a new ROM browser application.
    pub fn new(app_context: SharedAppContext) -> Self {
        let (default_height, fullscreen, favorites_path) = {
            let ctx = app_context.borrow();
            let config = ctx.config();
            (
                config.frontend.window_height,
                config.frontend.fullscreen,
                config.frontend.resolved_favorites_path(),
            )
        };
        // Default browser window: use configured height with 16:9 ratio.
        let default_width = (default_height as f64 * 16.0 / 9.0) as u32;

        Self {
            app_context,
            gl: None,
            result: BrowserResult::Closed,
            default_width,
            default_height,
            fullscreen,
            catalog: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            scroll_offset: 0.0,
            scroll_target: 0.0,
            textures_loaded: false,
            search_active: false,
            search_query: String::new(),
            genre_filter_active: false,
            available_genres: Vec::new(),
            active_genres: Vec::new(),
            genre_cursor: 0,
            detail_view_active: false,
            favorites: Favorites::load(&favorites_path),
            show_favorites_only: false,
            catalog_state: CatalogState::Idle,
            modifiers: winit::keyboard::ModifiersState::empty(),
        }
    }

    /// Set the ROM catalog to display.
    pub fn set_catalog(&mut self, mut catalog: Vec<RomEntry>) {
        // Apply stored favorites to catalog entries.
        for entry in &mut catalog {
            entry.is_favorite = self.favorites.contains(&entry.path);
        }

        // Collect all unique genres from the catalog.
        let mut genres: Vec<String> = catalog
            .iter()
            .flat_map(|e| e.genres.iter().cloned())
            .collect();
        genres.sort();
        genres.dedup();
        self.available_genres = genres;

        self.catalog = catalog;
        self.rebuild_filtered();
    }

    /// Rebuild the filtered index list based on current search query, genre, and favorites filter.
    fn rebuild_filtered(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .catalog
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                // Favorites filter.
                if self.show_favorites_only && !e.is_favorite {
                    return false;
                }
                // Text search filter.
                if !query.is_empty()
                    && !e.search_key.contains(&query)
                    && !e.display_name.to_lowercase().contains(&query)
                {
                    return false;
                }
                // Genre filter: entry must have at least one of the active genres.
                if !self.active_genres.is_empty()
                    && !e.genres.iter().any(|g| self.active_genres.contains(g))
                {
                    return false;
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        // Clamp selection.
        if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }
        self.scroll_offset = 0.0;
        self.scroll_target = 0.0;
    }

    /// Get the catalog entry for the currently selected filtered item.
    fn selected_entry(&self) -> Option<&RomEntry> {
        let &catalog_idx = self.filtered_indices.get(self.selected_index)?;
        self.catalog.get(catalog_idx)
    }

    /// Toggle favorite status for the currently selected ROM.
    fn toggle_favorite(&mut self) {
        if let Some(&catalog_idx) = self.filtered_indices.get(self.selected_index)
            && let Some(entry) = self.catalog.get_mut(catalog_idx)
        {
            let new_status = self.favorites.toggle(&entry.path);
            entry.is_favorite = new_status;
            if let Err(e) = self.favorites.save() {
                crate::platform::debugging::log_info(format!("Failed to save favorites: {e}"));
            }
            // If showing favorites only and we just unfavorited, rebuild filter.
            if self.show_favorites_only && !new_status {
                self.rebuild_filtered();
            }
        }
    }

    /// Run the ROM browser using the provided event loop and return the result.
    ///
    /// Uses `run_app_on_demand` so the event loop can be reused afterwards.
    pub fn run(mut self, event_loop: &mut EventLoop<()>) -> Result<BrowserResult, String> {
        use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
        event_loop
            .run_app_on_demand(&mut self)
            .map_err(|e| format!("Browser event loop error: {e}"))?;
        Ok(self.result)
    }

    /// Check if the background catalog loading thread has finished.
    fn poll_catalog_loading(&mut self) {
        // Drain all pending messages — update progress or finalize catalog.
        loop {
            let msg = if let CatalogState::Loading { ref receiver, .. } = self.catalog_state {
                receiver.try_recv().ok()
            } else {
                break;
            };
            match msg {
                Some(CatalogMessage::Progress(p)) => {
                    if let CatalogState::Loading {
                        ref mut progress, ..
                    } = self.catalog_state
                    {
                        *progress = Some(p);
                    }
                }
                Some(CatalogMessage::Done(catalog)) => {
                    self.set_catalog(catalog);
                    self.catalog_state = CatalogState::Ready;
                    break;
                }
                None => break,
            }
        }
    }

    fn render_frame(&mut self) {
        self.poll_catalog_loading();

        if !self.textures_loaded && matches!(self.catalog_state, CatalogState::Ready) {
            self.load_cover_textures();
            self.textures_loaded = true;
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
        let dt = gl.delta_time();
        self.scroll_offset +=
            (self.scroll_target - self.scroll_offset) * (theme::SCROLL_SPEED * dt).min(1.0);
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
                Some((game_id, (tex.egui_id, tex.width, tex.height)))
            })
            .collect();

        let display_entries: Vec<RomEntry> = self
            .filtered_indices
            .iter()
            .map(|&idx| self.catalog[idx].clone())
            .collect();

        let search_active = self.search_active;
        let search_query = self.search_query.clone();
        let genre_filter_active = self.genre_filter_active;
        let available_genres = self.available_genres.clone();
        let active_genres = self.active_genres.clone();
        let genre_cursor = self.genre_cursor;
        let detail_view_active = self.detail_view_active;
        let show_favorites_only = self.show_favorites_only;
        let selected_entry: Option<RomEntry> = self
            .filtered_indices
            .get(selected)
            .and_then(|&idx| self.catalog.get(idx))
            .cloned();

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
            visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
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

            egui::Panel::bottom("footer")
                .exact_size(theme::FOOTER_HEIGHT)
                .frame(bar_frame)
                .show_inside(ui, |ui| {
                    let footer_text = if search_active {
                        "Type to search  |  Esc: Close  |  Enter: Launch"
                    } else if genre_filter_active {
                        "Up/Down: Navigate  |  Enter: Toggle  |  Esc: Close"
                    } else if detail_view_active {
                        "Enter: Launch  |  Esc: Back"
                    } else {
                        "Enter: Launch  |  /: Search  |  g: Genre  |  d: Details  |  f: Fav  |  F: Filter Favs  |  Esc: Quit"
                    };
                    ui.label(
                        egui::RichText::new(footer_text)
                            .color(theme::DIM_TEXT)
                            .size(11.0),
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
                    );
                });

            if search_active {
                Self::render_search_overlay_egui(
                    ui.ctx(),
                    &search_query,
                    filtered_count,
                    display_w,
                );
            }
            if genre_filter_active && !available_genres.is_empty() {
                Self::render_genre_filter_egui(
                    ui.ctx(),
                    &available_genres,
                    &active_genres,
                    genre_cursor,
                    display_w,
                    display_h,
                );
            }
            if detail_view_active
                && let Some(ref entry) = selected_entry
            {
                Self::render_detail_view_egui(ui.ctx(), entry, &tex_map, display_w, display_h);
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
                let title = if p.game_title.len() > max_chars && max_chars > 3 {
                    format!("{}...", &p.game_title[..max_chars - 3])
                } else {
                    p.game_title.clone()
                };
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

    #[allow(clippy::too_many_arguments)]
    fn render_grid_egui(
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
    ) {
        if entries.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                if search_query.is_empty() {
                    ui.label(
                        egui::RichText::new("No ROMs found. Add ROM files to ~/.neser/roms/")
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
                    let short = if entry.display_name.len() > max_chars && max_chars > 3 {
                        format!("{}...", &entry.display_name[..max_chars - 3])
                    } else {
                        entry.display_name.clone()
                    };
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
                let short = if entry.display_name.len() > max_chars && max_chars > 3 {
                    format!("{}...", &entry.display_name[..max_chars - 3])
                } else {
                    entry.display_name.clone()
                };
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

    fn render_sidebar_egui(
        ui: &mut egui::Ui,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
    ) {
        // Fixed-height area for cover art so text position stays constant.
        let avail_w = ui.available_width();
        let art_area_h = theme::SIDEBAR_ART_HEIGHT;
        let (art_rect, _) =
            ui.allocate_exact_size(egui::vec2(avail_w, art_area_h), egui::Sense::hover());

        if let Some(game_id) = entry.metadata_game_id {
            if let Some(&(tex_id, tex_w, tex_h)) = tex_map.get(&game_id) {
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
                            .size(12.0),
                    );
                }
                if let Some(ref date) = entry.release_date {
                    ui.label(
                        egui::RichText::new(format!("Released: {date}"))
                            .color(theme::DIM_TEXT)
                            .size(12.0),
                    );
                }
                if let Some(players) = entry.players {
                    ui.label(
                        egui::RichText::new(format!("Players: {players}"))
                            .color(theme::DIM_TEXT)
                            .size(12.0),
                    );
                }
                if let Some(ref rating) = entry.rating {
                    ui.label(
                        egui::RichText::new(format!("Rating: {rating}"))
                            .color(theme::DIM_TEXT)
                            .size(12.0),
                    );
                }
                ui.label(
                    egui::RichText::new(format!("Mapper: {}", entry.mapper_label))
                        .color(theme::DIM_TEXT)
                        .size(12.0),
                );
                if let Some(ref crc) = entry.crc {
                    ui.label(
                        egui::RichText::new(format!("CRC: {crc}"))
                            .color(theme::DIM_TEXT)
                            .size(12.0),
                    );
                }
                if let Some(ref hw) = entry.hardware {
                    ui.label(
                        egui::RichText::new(format!("Hardware: {hw}"))
                            .color(theme::DIM_TEXT)
                            .size(12.0),
                    );
                }
                if entry.is_favorite {
                    ui.label(
                        egui::RichText::new("\u{2665} Favourite")
                            .color(theme::FAVORITE_COLOR)
                            .size(13.0),
                    );
                }
            });

        if let Some(ref overview) = entry.overview {
            ui.add_space(6.0);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(22, 22, 28))
                .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS as u8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(overview)
                            .color(theme::TEXT_COLOR)
                            .size(12.0),
                    );
                });
        }
    }

    fn render_search_overlay_egui(ctx: &egui::Context, query: &str, count: usize, display_w: f32) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("search_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(120),
        );

        let search_w = (display_w * 0.5).clamp(300.0, 600.0);
        let search_x = (display_w - search_w) / 2.0;

        egui::Window::new("search")
            .id(egui::Id::new("search_overlay"))
            .fixed_pos(egui::pos2(search_x, 80.0))
            .fixed_size(egui::vec2(search_w, 48.0))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("Search: {query}\u{258C}    ({count} matches)"))
                        .color(theme::HEADER_TEXT)
                        .size(14.0),
                );
            });
    }

    fn render_genre_filter_egui(
        ctx: &egui::Context,
        available_genres: &[String],
        active_genres: &[String],
        genre_cursor: usize,
        display_w: f32,
        display_h: f32,
    ) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("genre_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(120),
        );

        let panel_w = 300.0_f32.min(display_w * 0.4);
        let panel_h = (available_genres.len() as f32 * 26.0 + 60.0).min(display_h * 0.8);
        let panel_x = (display_w - panel_w) / 2.0;
        let panel_y = (display_h - panel_h) / 2.0;

        egui::Window::new("genre_filter")
            .id(egui::Id::new("genre_overlay"))
            .fixed_pos(egui::pos2(panel_x, panel_y))
            .fixed_size(egui::vec2(panel_w, panel_h))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Filter by Genre")
                        .color(theme::HEADER_TEXT)
                        .size(14.0),
                );
                ui.separator();

                for (i, genre) in available_genres.iter().enumerate() {
                    let is_active = active_genres.contains(genre);
                    let marker = if is_active { "[x] " } else { "[ ] " };
                    let is_cursor = i == genre_cursor;

                    let color = if is_cursor {
                        theme::SELECTION_COLOR
                    } else if is_active {
                        theme::SELECTED_TEXT
                    } else {
                        theme::TEXT_COLOR
                    };

                    let label = if is_cursor {
                        format!("> {marker}{genre}")
                    } else {
                        format!("  {marker}{genre}")
                    };
                    ui.label(egui::RichText::new(&label).color(color).size(12.0));
                }
            });
    }

    fn render_detail_view_egui(
        ctx: &egui::Context,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
        display_w: f32,
        display_h: f32,
    ) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("detail_dim"),
        ));
        painter.rect_filled(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(display_w * 2.0, 10000.0)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(200),
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
                let avail = ui.available_size();
                let boxart_w = avail.x * 0.35;

                ui.horizontal(|ui| {
                    // Left: cover art.
                    if let Some(game_id) = entry.metadata_game_id
                        && let Some(&(tex_id, _, _)) = tex_map.get(&game_id)
                    {
                        let art_h = boxart_w / theme::COVER_ASPECT;
                        ui.add(
                            egui::Image::from_texture(egui::load::SizedTexture::new(
                                tex_id,
                                egui::vec2(boxart_w, art_h),
                            ))
                            .corner_radius(theme::CORNER_RADIUS),
                        );
                    }

                    // Right: metadata.
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.display_name)
                                .color(theme::HEADER_TEXT)
                                .size(18.0),
                        );
                        ui.separator();

                        if !entry.genres.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("Genre: {}", entry.genres.join(", ")))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        if let Some(ref date) = entry.release_date {
                            ui.label(
                                egui::RichText::new(format!("Released: {date}"))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        if let Some(players) = entry.players {
                            ui.label(
                                egui::RichText::new(format!("Players: {players}"))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        if let Some(ref rating) = entry.rating {
                            ui.label(
                                egui::RichText::new(format!("Rating: {rating}"))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("Mapper: {}", entry.mapper_label))
                                .color(theme::DIM_TEXT)
                                .size(13.0),
                        );
                        if let Some(ref crc) = entry.crc {
                            ui.label(
                                egui::RichText::new(format!("CRC: {crc}"))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        if let Some(ref hw) = entry.hardware {
                            ui.label(
                                egui::RichText::new(format!("Hardware: {hw}"))
                                    .color(theme::DIM_TEXT)
                                    .size(13.0),
                            );
                        }
                        if entry.is_favorite {
                            ui.label(
                                egui::RichText::new("\u{2665} Favourite")
                                    .color(theme::FAVORITE_COLOR)
                                    .size(14.0),
                            );
                        }

                        if let Some(ref overview) = entry.overview {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(overview)
                                    .color(theme::TEXT_COLOR)
                                    .size(13.0),
                            );
                        }

                        ui.separator();
                        ui.label(
                            egui::RichText::new("Enter: Launch  |  Esc: Back to grid")
                                .color(theme::DIM_TEXT)
                                .size(12.0),
                        );
                    });
                });
            });
    }

    /// Load cover art textures for all catalog entries that have boxart paths.
    fn load_cover_textures(&mut self) {
        let Some(ref mut gl) = self.gl else { return };
        for entry in &self.catalog {
            if let (Some(game_id), Some(path)) = (entry.metadata_game_id, &entry.boxart_path) {
                let key = TextureKey::CoverArt(game_id);
                gl.load_texture_from_file(key, path);
            }
        }
    }

    /// Ensure the selected cell is visible by adjusting scroll target.
    fn ensure_selected_visible(&mut self) {
        let Some(ref gl) = self.gl else { return };
        let (display_w, display_h) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let grid_height = display_h - theme::HEADER_HEIGHT - theme::FOOTER_HEIGHT;

        let row = self.selected_index / cols;
        let cell_top = theme::GRID_PADDING + row as f32 * (cell_h + theme::GRID_SPACING);
        let cell_bottom = cell_top + cell_h + theme::GRID_SPACING;

        if cell_top < self.scroll_target {
            self.scroll_target = cell_top - theme::GRID_PADDING;
        }
        if cell_bottom > self.scroll_target + grid_height {
            self.scroll_target = cell_bottom - grid_height + theme::GRID_PADDING;
        }
        self.scroll_target = self.scroll_target.max(0.0);
    }

    /// Get the current number of grid columns based on window size.
    fn current_cols(&self) -> usize {
        let Some(ref gl) = self.gl else { return 1 };
        let (display_w, _) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, _) = theme::grid_layout(grid_area_w);
        cols
    }

    /// Move selection up by one row in the grid.
    fn navigate_up(&mut self) {
        let cols = self.current_cols();
        if self.selected_index >= cols {
            self.selected_index -= cols;
        } else {
            self.selected_index = 0;
        }
        self.ensure_selected_visible();
    }

    /// Move selection down by one row in the grid.
    fn navigate_down(&mut self) {
        let cols = self.current_cols();
        let count = self.filtered_indices.len();
        let new_idx = self.selected_index + cols;
        if new_idx < count {
            self.selected_index = new_idx;
        } else if self.selected_index < count.saturating_sub(1) {
            self.selected_index = count - 1;
        }
        self.ensure_selected_visible();
    }
}

impl ApplicationHandler for RomBrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl.is_some() {
            return;
        }

        match BrowserGl::new(
            event_loop,
            self.default_width,
            self.default_height,
            self.fullscreen,
        ) {
            Ok(gl) => {
                self.gl = Some(gl);
                // Spawn background thread for catalog loading + enrichment.
                if matches!(self.catalog_state, CatalogState::Idle) {
                    let (search_paths, rebuild, metadata_db_path, image_cache_path) = {
                        let ctx = self.app_context.borrow();
                        let config = ctx.config();
                        (
                            config.frontend.cartridge_search_paths.clone(),
                            config.frontend.rebuild_cartridge_catalog,
                            config.frontend.resolved_metadata_db_path(),
                            config.frontend.resolved_image_cache_path(),
                        )
                    };
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        match crate::platform::catalog::load_catalog(&search_paths, rebuild) {
                            Ok(mut catalog) => {
                                let tx2 = tx.clone();
                                crate::platform::catalog::enrich_catalog(
                                    &mut catalog,
                                    &metadata_db_path,
                                    &image_cache_path,
                                    move |progress| {
                                        let _ = tx2.send(CatalogMessage::Progress(progress));
                                    },
                                );
                                catalog.sort_by(|a, b| {
                                    a.display_name
                                        .to_lowercase()
                                        .cmp(&b.display_name.to_lowercase())
                                });
                                let _ = tx.send(CatalogMessage::Done(catalog));
                            }
                            Err(e) => {
                                eprintln!("Failed to load ROM catalog: {e}");
                                let _ = tx.send(CatalogMessage::Done(Vec::new()));
                            }
                        }
                    });
                    self.catalog_state = CatalogState::Loading {
                        receiver: rx,
                        progress: None,
                    };
                }
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            Err(e) => {
                eprintln!("Failed to create browser GL context: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.result = BrowserResult::Closed;
                event_loop.exit();
            }

            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut gl) = self.gl {
                    gl.notify_resize(physical_size.width, physical_size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                use winit::keyboard::{Key, NamedKey};

                if self.search_active {
                    // Search mode input handling.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.search_active = false;
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.search_query.pop();
                            self.rebuild_filtered();
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(entry) = self.selected_entry() {
                                self.result = BrowserResult::RomSelected(entry.path.clone());
                                event_loop.exit();
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.navigate_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.navigate_down();
                        }
                        Key::Character(ref ch) => {
                            self.search_query.push_str(ch.as_str());
                            self.rebuild_filtered();
                        }
                        _ => {}
                    }
                } else if self.genre_filter_active {
                    // Genre filter mode input handling.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.genre_filter_active = false;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if self.genre_cursor > 0 {
                                self.genre_cursor -= 1;
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if self.genre_cursor + 1 < self.available_genres.len() {
                                self.genre_cursor += 1;
                            }
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            if let Some(genre) =
                                self.available_genres.get(self.genre_cursor).cloned()
                            {
                                if let Some(pos) =
                                    self.active_genres.iter().position(|g| *g == genre)
                                {
                                    self.active_genres.remove(pos);
                                } else {
                                    self.active_genres.push(genre);
                                }
                                self.rebuild_filtered();
                            }
                        }
                        _ => {}
                    }
                } else if self.detail_view_active {
                    // Detail view mode.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.detail_view_active = false;
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(entry) = self.selected_entry() {
                                self.result = BrowserResult::RomSelected(entry.path.clone());
                                event_loop.exit();
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Normal browsing mode.
                    let ctrl = self
                        .modifiers
                        .contains(winit::keyboard::ModifiersState::CONTROL)
                        || self
                            .modifiers
                            .contains(winit::keyboard::ModifiersState::SUPER);
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            if !self.search_query.is_empty() || !self.active_genres.is_empty() {
                                self.search_query.clear();
                                self.active_genres.clear();
                                self.rebuild_filtered();
                            } else {
                                self.result = BrowserResult::Closed;
                                event_loop.exit();
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.navigate_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.navigate_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                                self.ensure_selected_visible();
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if self.selected_index + 1 < self.filtered_indices.len() {
                                self.selected_index += 1;
                                self.ensure_selected_visible();
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(entry) = self.selected_entry() {
                                self.result = BrowserResult::RomSelected(entry.path.clone());
                                event_loop.exit();
                            }
                        }
                        Key::Character(ref ch) if ch.as_str() == "/" => {
                            self.search_active = true;
                        }
                        Key::Character(ref ch)
                            if (ch.as_str() == "f" || ch.as_str() == "F") && ctrl =>
                        {
                            // Toggle fullscreen.
                            if let Some(ref gl) = self.gl {
                                let window = gl.window();
                                if window.fullscreen().is_some() {
                                    window.set_fullscreen(None);
                                } else {
                                    window.set_fullscreen(Some(
                                        winit::window::Fullscreen::Borderless(None),
                                    ));
                                }
                            }
                        }
                        Key::Character(ref ch)
                            if (ch.as_str() == "q" || ch.as_str() == "Q") && ctrl =>
                        {
                            self.result = BrowserResult::Closed;
                            event_loop.exit();
                        }
                        Key::Character(ref ch) if ch.as_str() == "g" && !ctrl => {
                            self.genre_filter_active = true;
                            self.genre_cursor = 0;
                        }
                        Key::Character(ref ch) if ch.as_str() == "d" && !ctrl => {
                            if self.selected_entry().is_some() {
                                self.detail_view_active = true;
                            }
                        }
                        Key::Character(ref ch) if ch.as_str() == "f" && !ctrl => {
                            self.toggle_favorite();
                        }
                        Key::Character(ref ch) if ch.as_str() == "F" && !ctrl => {
                            self.show_favorites_only = !self.show_favorites_only;
                            self.rebuild_filtered();
                        }
                        _ => {}
                    }
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                if let Some(ref mut gl) = self.gl {
                    let _ = gl.on_window_event(&event);
                }
            }

            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch { .. }
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Focused(_)
            | WindowEvent::Ime(_) => {
                if let Some(ref mut gl) = self.gl {
                    let _ = gl.on_window_event(&event);
                }
            }

            WindowEvent::RedrawRequested => {
                self.render_frame();
                if let Some(ref gl) = self.gl {
                    gl.window().request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::IntoSharedAppContext;

    #[test]
    fn browser_result_default_is_closed() {
        let result = BrowserResult::Closed;
        assert!(matches!(result, BrowserResult::Closed));
    }

    #[test]
    fn browser_result_rom_selected_holds_path() {
        let path = PathBuf::from("/roms/game.nes");
        let result = BrowserResult::RomSelected(path.clone());
        match result {
            BrowserResult::RomSelected(p) => assert_eq!(p, path),
            BrowserResult::Closed => panic!("expected RomSelected"),
        }
    }

    fn make_entry(name: &str) -> RomEntry {
        RomEntry {
            path: PathBuf::from(format!("{name}.nes")),
            display_name: name.to_string(),
            search_key: name.to_lowercase(),
            mapper_label: "-".to_string(),
            mapper: None,
            hardware: None,
            crc: None,
            recording_duration: None,
            metadata_game_id: None,
            genres: Vec::new(),
            overview: None,
            release_date: None,
            players: None,
            rating: None,
            boxart_path: None,
            screenshot_paths: Vec::new(),
            is_favorite: false,
        }
    }

    /// Create a minimal RomBrowserApp for testing (without GL or AppContext).
    fn test_browser(entries: Vec<RomEntry>) -> RomBrowserApp {
        let dir = tempfile::TempDir::new().unwrap();
        let fav_path = dir.path().join("favorites.json");
        let mut app = RomBrowserApp {
            app_context: crate::platform::app_context::AppContext::new().into_shared(),
            gl: None,
            result: BrowserResult::Closed,
            default_width: 1280,
            default_height: 720,
            fullscreen: false,
            catalog: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            scroll_offset: 0.0,
            scroll_target: 0.0,
            textures_loaded: false,
            search_active: false,
            search_query: String::new(),
            genre_filter_active: false,
            available_genres: Vec::new(),
            active_genres: Vec::new(),
            genre_cursor: 0,
            detail_view_active: false,
            favorites: Favorites::load(&fav_path),
            show_favorites_only: false,
            catalog_state: CatalogState::Ready,
            modifiers: winit::keyboard::ModifiersState::empty(),
        };
        app.set_catalog(entries);
        app
    }

    #[test]
    fn rebuild_filtered_shows_all_when_no_query() {
        let app = test_browser(vec![
            make_entry("Super Mario Bros"),
            make_entry("Zelda"),
            make_entry("Metroid"),
        ]);
        assert_eq!(app.filtered_indices.len(), 3);
        assert_eq!(app.filtered_indices, vec![0, 1, 2]);
    }

    #[test]
    fn rebuild_filtered_narrows_by_search() {
        let mut app = test_browser(vec![
            make_entry("Super Mario Bros"),
            make_entry("Zelda"),
            make_entry("Super Metroid"),
        ]);
        app.search_query = "super".to_string();
        app.rebuild_filtered();
        assert_eq!(app.filtered_indices.len(), 2);
        assert_eq!(app.filtered_indices, vec![0, 2]);
    }

    #[test]
    fn rebuild_filtered_empty_result() {
        let mut app = test_browser(vec![make_entry("Mario"), make_entry("Zelda")]);
        app.search_query = "castlevania".to_string();
        app.rebuild_filtered();
        assert!(app.filtered_indices.is_empty());
    }

    #[test]
    fn selected_entry_returns_correct_rom() {
        let app = test_browser(vec![
            make_entry("Game A"),
            make_entry("Game B"),
            make_entry("Game C"),
        ]);
        assert_eq!(app.selected_entry().unwrap().display_name, "Game A");
    }

    #[test]
    fn selected_entry_after_filter_returns_filtered_item() {
        let mut app = test_browser(vec![
            make_entry("Alpha"),
            make_entry("Beta"),
            make_entry("Gamma"),
        ]);
        app.search_query = "beta".to_string();
        app.rebuild_filtered();
        // selected_index 0 in filtered = "Beta" (catalog index 1).
        assert_eq!(app.selected_entry().unwrap().display_name, "Beta");
    }

    #[test]
    fn selection_clamps_after_filter_reduces_results() {
        let mut app = test_browser(vec![make_entry("A"), make_entry("B"), make_entry("C")]);
        app.selected_index = 2; // last item
        app.search_query = "a".to_string();
        app.rebuild_filtered();
        // Only 1 match, selection should clamp to 0.
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.selected_entry().unwrap().display_name, "A");
    }

    fn make_entry_with_genres(name: &str, genres: Vec<&str>) -> RomEntry {
        let mut entry = make_entry(name);
        entry.genres = genres.into_iter().map(String::from).collect();
        entry
    }

    #[test]
    fn set_catalog_collects_available_genres() {
        let app = test_browser(vec![
            make_entry_with_genres("Mario", vec!["Platform", "Action"]),
            make_entry_with_genres("Zelda", vec!["Adventure", "Action"]),
            make_entry_with_genres("Tetris", vec!["Puzzle"]),
        ]);
        assert_eq!(
            app.available_genres,
            vec!["Action", "Adventure", "Platform", "Puzzle"]
        );
    }

    #[test]
    fn genre_filter_narrows_results() {
        let mut app = test_browser(vec![
            make_entry_with_genres("Mario", vec!["Platform"]),
            make_entry_with_genres("Zelda", vec!["Adventure"]),
            make_entry_with_genres("Contra", vec!["Platform", "Shooter"]),
        ]);
        app.active_genres = vec!["Platform".to_string()];
        app.rebuild_filtered();
        assert_eq!(app.filtered_indices.len(), 2);
        assert_eq!(app.filtered_indices, vec![0, 2]);
    }

    #[test]
    fn genre_and_search_combined() {
        let mut app = test_browser(vec![
            make_entry_with_genres("Super Mario", vec!["Platform"]),
            make_entry_with_genres("Super Contra", vec!["Platform", "Shooter"]),
            make_entry_with_genres("Zelda", vec!["Adventure"]),
        ]);
        app.active_genres = vec!["Platform".to_string()];
        app.search_query = "super".to_string();
        app.rebuild_filtered();
        assert_eq!(app.filtered_indices.len(), 2);
        assert_eq!(app.filtered_indices, vec![0, 1]);
    }

    #[test]
    fn detail_view_opens_when_entry_selected() {
        let app = test_browser(vec![make_entry("Castlevania")]);
        assert!(!app.detail_view_active);
        // Simulate pressing 'd' — selected_entry() returns Some so detail opens
        let mut app = app;
        app.detail_view_active = app.selected_entry().is_some();
        assert!(app.detail_view_active);
    }

    #[test]
    fn detail_view_does_not_open_when_catalog_empty() {
        let app = test_browser(vec![]);
        assert!(!app.detail_view_active);
        let mut app = app;
        if app.selected_entry().is_some() {
            app.detail_view_active = true;
        }
        assert!(!app.detail_view_active);
    }

    #[test]
    fn toggle_favorite_marks_entry() {
        let mut app = test_browser(vec![make_entry("Zelda"), make_entry("Mario")]);
        assert!(!app.catalog[0].is_favorite);
        app.toggle_favorite();
        assert!(app.catalog[0].is_favorite);
        // Toggle off.
        app.toggle_favorite();
        assert!(!app.catalog[0].is_favorite);
    }

    #[test]
    fn show_favorites_only_filters_non_favorites() {
        let mut app = test_browser(vec![
            make_entry("Zelda"),
            make_entry("Mario"),
            make_entry("Contra"),
        ]);
        // Favorite only Mario (index 1).
        app.selected_index = 1;
        app.toggle_favorite();
        // Enable favorites filter.
        app.show_favorites_only = true;
        app.rebuild_filtered();
        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.catalog[app.filtered_indices[0]].display_name, "Mario");
    }

    #[test]
    fn toggle_favorite_rebuilds_filter_when_showing_favorites() {
        let mut app = test_browser(vec![make_entry("Zelda"), make_entry("Mario")]);
        // Favorite both.
        app.toggle_favorite();
        app.selected_index = 1;
        app.toggle_favorite();
        // Enable favorites filter.
        app.show_favorites_only = true;
        app.rebuild_filtered();
        assert_eq!(app.filtered_indices.len(), 2);
        // Unfavorite Zelda (selected = 0).
        app.selected_index = 0;
        app.toggle_favorite();
        // Should auto-rebuild and now only Mario remains.
        assert_eq!(app.filtered_indices.len(), 1);
    }

    #[test]
    fn poll_catalog_loading_receives_catalog() {
        let mut app = test_browser(vec![]);
        assert!(app.catalog.is_empty());

        let (tx, rx) = mpsc::channel();
        app.catalog_state = CatalogState::Loading {
            receiver: rx,
            progress: None,
        };

        // Before send: poll does nothing.
        app.poll_catalog_loading();
        assert!(app.catalog.is_empty());

        // Send catalog from "background thread".
        tx.send(CatalogMessage::Done(vec![
            make_entry("Zelda"),
            make_entry("Mario"),
        ]))
        .unwrap();
        app.poll_catalog_loading();

        assert_eq!(app.catalog.len(), 2);
        assert!(matches!(app.catalog_state, CatalogState::Ready));
    }

    #[test]
    fn poll_catalog_loading_tracks_progress() {
        let mut app = test_browser(vec![]);

        let (tx, rx) = mpsc::channel();
        app.catalog_state = CatalogState::Loading {
            receiver: rx,
            progress: None,
        };

        tx.send(CatalogMessage::Progress(EnrichmentProgress {
            current: 3,
            total: 10,
            game_title: "Zelda".to_string(),
            phase: EnrichmentPhase::MatchingMetadata,
        }))
        .unwrap();
        app.poll_catalog_loading();

        if let CatalogState::Loading { ref progress, .. } = app.catalog_state {
            let p = progress.as_ref().unwrap();
            assert_eq!(p.current, 3);
            assert_eq!(p.total, 10);
            assert_eq!(p.game_title, "Zelda");
        } else {
            panic!("Expected CatalogState::Loading");
        }
    }
}
