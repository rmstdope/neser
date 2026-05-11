//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! GL-backed window with egui rendering and accepts a ROM selection that
//! transitions the application into emulation mode.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use gilrs::{Axis, EventType, Gilrs, GilrsBuilder};
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
#[derive(Debug, Clone)]
pub enum BrowserResult {
    /// User selected a ROM to launch.
    RomSelected(PathBuf),
    /// User closed the browser window without selecting.
    Closed,
}

/// Threshold for converting analog stick axes to digital D-pad presses.
const AXIS_DEAD_ZONE: f32 = 0.5;

/// Delay before the first repeat fires (ms).
const REPEAT_DELAY_MS: u128 = 400;
/// Interval between subsequent repeats (ms).
const REPEAT_INTERVAL_MS: u128 = 80;

/// Tracks analog stick state for digital D-pad conversion in the browser.
#[derive(Default)]
struct GamepadAxisState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

/// Logical direction for unified repeat tracking (buttons + axes).
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
enum RepeatDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Tracks held directional inputs for auto-repeat.
#[derive(Default)]
struct GamepadRepeatState {
    /// Currently held directions and when they were first pressed.
    held: HashMap<RepeatDirection, Instant>,
    /// Last time a repeat fired for each direction.
    last_repeat: HashMap<RepeatDirection, Instant>,
}

/// Actions that can be triggered by gamepad input.
enum BrowserAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Search,
    Favorite,
    Detail,
    GenreFilter,
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
    /// Gamepad input via gilrs.
    gilrs: Option<Gilrs>,
    /// Tracks analog stick state for digital D-pad conversion.
    gamepad_axis: GamepadAxisState,
    /// Tracks held D-pad buttons for auto-repeat.
    gamepad_repeat: GamepadRepeatState,
    /// Sender for texture decode requests (game_id, path).
    texture_request_tx: mpsc::Sender<(i64, PathBuf)>,
    /// Receiver for decoded texture results (game_id, width, height, pixels).
    texture_result_rx: mpsc::Receiver<(i64, u32, u32, Vec<u8>)>,
    /// Game IDs that have been requested but not yet received.
    texture_pending: Vec<i64>,
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

        // Spawn a background thread for image decoding.
        let (request_tx, request_rx) = mpsc::channel::<(i64, PathBuf)>();
        let (result_tx, result_rx) = mpsc::channel::<(i64, u32, u32, Vec<u8>)>();
        std::thread::Builder::new()
            .name("texture-decoder".into())
            .spawn(move || {
                while let Ok((game_id, path)) = request_rx.recv() {
                    if let Ok(img) = image::open(&path) {
                        let rgba = img.into_rgba8();
                        let (w, h) = rgba.dimensions();
                        let pixels = rgba.into_raw();
                        if result_tx.send((game_id, w, h, pixels)).is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("failed to spawn texture decoder thread");

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
            gilrs: {
                let mut builder = GilrsBuilder::new()
                    .with_default_filters(true)
                    .add_env_mappings(true);
                if let Ok(mappings) = std::fs::read_to_string("gamecontrollerdb.txt") {
                    builder = builder.add_mappings(&mappings);
                }
                builder.build().ok()
            },
            gamepad_axis: GamepadAxisState::default(),
            gamepad_repeat: GamepadRepeatState::default(),
            texture_request_tx: request_tx,
            texture_result_rx: result_rx,
            texture_pending: Vec::new(),
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
    /// The browser state (catalog, textures) is preserved across calls.
    pub fn run(&mut self, event_loop: &mut EventLoop<()>) -> Result<BrowserResult, String> {
        use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
        // Reset transient state for a fresh UI pass but keep catalog/textures.
        self.result = BrowserResult::Closed;
        self.gl = None;
        event_loop
            .run_app_on_demand(self)
            .map_err(|e| format!("Browser event loop error: {e}"))?;
        Ok(self.result.clone())
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
        let dt = gl.delta_time();
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

                            // Button legend at the bottom of the sidebar.
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                                let legend_items: &[(&str, &str)] = if search_active {
                                    &[("Esc", "Close"), ("Enter", "Launch")]
                                } else if genre_filter_active {
                                    &[("↑↓", "Navigate"), ("Enter", "Toggle"), ("Esc", "Close")]
                                } else if detail_view_active {
                                    &[("A", "Launch"), ("Y", "Fav"), ("B", "Back")]
                                } else {
                                    &[
                                        ("A", "Details"),
                                        ("B", "Filter"),
                                        ("Select", "Favorite"),
                                        ("Start", "Search"),
                                    ]
                                };
                                Self::render_button_legend(ui, legend_items);
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
            if detail_view_active && let Some(ref entry) = selected_entry {
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
    fn button_pill_color(btn: &str) -> egui::Color32 {
        match btn {
            "A" => theme::BUTTON_COLOR_A,
            "B" => theme::BUTTON_COLOR_B,
            "X" => theme::BUTTON_COLOR_X,
            "Y" => theme::BUTTON_COLOR_Y,
            "Select" | "Start" => egui::Color32::from_rgb(160, 160, 175),
            _ => theme::BUTTON_PILL_BG,
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
                    // Left: cover art (preserve actual aspect ratio).
                    if let Some(game_id) = entry.metadata_game_id
                        && let Some(&(tex_id, tex_w, tex_h)) = tex_map.get(&game_id)
                    {
                        let img_aspect = tex_w as f32 / tex_h.max(1) as f32;
                        let art_h = boxart_w / img_aspect;
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
                                .size(25.0),
                        );
                        ui.separator();

                        if !entry.genres.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("Genre: {}", entry.genres.join(", ")))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        if let Some(ref date) = entry.release_date {
                            ui.label(
                                egui::RichText::new(format!("Released: {date}"))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        if let Some(players) = entry.players {
                            ui.label(
                                egui::RichText::new(format!("Players: {players}"))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        if let Some(ref rating) = entry.rating {
                            ui.label(
                                egui::RichText::new(format!("Rating: {rating}"))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("Mapper: {}", entry.mapper_label))
                                .color(theme::DIM_TEXT)
                                .size(18.0),
                        );
                        if let Some(ref crc) = entry.crc {
                            ui.label(
                                egui::RichText::new(format!("CRC: {crc}"))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        if let Some(ref hw) = entry.hardware {
                            ui.label(
                                egui::RichText::new(format!("Hardware: {hw}"))
                                    .color(theme::DIM_TEXT)
                                    .size(18.0),
                            );
                        }
                        if let Some(file_name) = entry.path.file_name() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "File: {}",
                                    file_name.to_string_lossy()
                                ))
                                .color(theme::DIM_TEXT)
                                .size(18.0),
                            );
                        }
                        if entry.is_favorite {
                            ui.label(
                                egui::RichText::new("\u{2665} Favourite")
                                    .color(theme::FAVORITE_COLOR)
                                    .size(19.0),
                            );
                        }

                        if let Some(ref overview) = entry.overview {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(overview)
                                    .color(theme::TEXT_COLOR)
                                    .size(18.0),
                            );
                        }

                        ui.separator();
                        Self::render_button_legend(
                            ui,
                            &[("A", "Launch"), ("B", "Back"), ("Select", "Favorite")],
                        );
                    });
                });
            });
    }

    /// Maximum number of decode requests to send per frame.
    const MAX_REQUESTS_PER_FRAME: usize = 8;
    /// Maximum number of decoded results to upload per frame.
    const MAX_UPLOADS_PER_FRAME: usize = 4;
    /// How many rows of buffer above/below the viewport to preload.
    const PRELOAD_ROW_BUFFER: usize = 2;
    /// Maximum number of textures to keep in memory.
    const MAX_CACHED_TEXTURES: usize = 200;

    /// Lazily load textures for visible grid items and evict offscreen ones.
    ///
    /// Image decoding is done on a background thread. This method:
    /// 1. Uploads any decoded results that are ready (fast GL upload only)
    /// 2. Sends decode requests for visible but unloaded textures
    /// 3. Evicts offscreen textures when the budget is exceeded
    fn lazy_load_visible_textures(&mut self) {
        // 1. Upload any decoded results from the background thread (non-blocking).
        let mut decoded: Vec<(i64, u32, u32, Vec<u8>)> =
            Vec::with_capacity(Self::MAX_UPLOADS_PER_FRAME);
        while decoded.len() < Self::MAX_UPLOADS_PER_FRAME {
            match self.texture_result_rx.try_recv() {
                Ok(result) => decoded.push(result),
                Err(_) => break,
            }
        }
        if !decoded.is_empty() {
            if let Some(ref mut gl) = self.gl {
                for (game_id, w, h, pixels) in &decoded {
                    let key = TextureKey::CoverArt(*game_id);
                    gl.load_texture_from_rgba(key, *w, *h, pixels);
                }
            }
            for (game_id, _, _, _) in &decoded {
                self.texture_pending.retain(|&id| id != *game_id);
            }
        }

        let Some(ref gl) = self.gl else { return };

        let (display_w, display_h) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let grid_height = display_h - theme::HEADER_HEIGHT;

        // Determine the range of rows visible on screen (with buffer).
        let first_visible_row =
            (self.scroll_offset / (cell_h + theme::GRID_SPACING)).floor() as usize;
        let rows_on_screen = (grid_height / (cell_h + theme::GRID_SPACING)).ceil() as usize + 1;
        let first_row = first_visible_row.saturating_sub(Self::PRELOAD_ROW_BUFFER);
        let last_row = first_visible_row + rows_on_screen + Self::PRELOAD_ROW_BUFFER;

        // Collect game IDs for entries in the visible range.
        let first_idx = first_row * cols;
        let last_idx = ((last_row + 1) * cols).min(self.filtered_indices.len());
        let range_start = first_idx.min(self.filtered_indices.len());
        let range_end = last_idx.min(self.filtered_indices.len());

        let mut visible_game_ids: Vec<i64> = Vec::with_capacity(range_end - range_start);
        for &fi in &self.filtered_indices[range_start..range_end] {
            if let Some(gid) = self.catalog[fi].metadata_game_id {
                visible_game_ids.push(gid);
            }
        }

        // Also include the selected entry (sidebar needs its texture).
        if let Some(&catalog_idx) = self.filtered_indices.get(self.selected_index)
            && let Some(gid) = self.catalog[catalog_idx].metadata_game_id
            && !visible_game_ids.contains(&gid)
        {
            visible_game_ids.push(gid);
        }

        // 2. Send decode requests for visible entries not yet loaded or pending.
        let mut requests_sent = 0;
        for &game_id in &visible_game_ids {
            if requests_sent >= Self::MAX_REQUESTS_PER_FRAME {
                break;
            }
            let key = TextureKey::CoverArt(game_id);
            if gl.get_texture(&key).is_some() || self.texture_pending.contains(&game_id) {
                continue;
            }
            if let Some(path) = self
                .catalog
                .iter()
                .find(|e| e.metadata_game_id == Some(game_id))
                .and_then(|e| e.boxart_path.as_ref())
                && path.exists()
            {
                let _ = self.texture_request_tx.send((game_id, path.clone()));
                self.texture_pending.push(game_id);
                requests_sent += 1;
            }
        }

        // 3. Evict offscreen textures when we exceed the budget.
        if gl.texture_count() > Self::MAX_CACHED_TEXTURES {
            let loaded_keys = gl.texture_keys();
            let mut to_evict: Vec<TextureKey> = Vec::new();
            for key in loaded_keys {
                if gl.texture_count().saturating_sub(to_evict.len()) <= Self::MAX_CACHED_TEXTURES {
                    break;
                }
                if let TextureKey::CoverArt(gid) = key
                    && !visible_game_ids.contains(&gid)
                {
                    to_evict.push(TextureKey::CoverArt(gid));
                }
            }
            let gl = self.gl.as_mut().unwrap();
            for key in to_evict {
                gl.remove_texture(&key);
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
        let grid_height = display_h - theme::HEADER_HEIGHT;

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

    /// Poll gamepad events and return browser actions.
    fn poll_gamepad(&mut self) -> Vec<BrowserAction> {
        let gilrs = match self.gilrs.as_mut() {
            Some(g) => g,
            None => return Vec::new(),
        };

        let mut actions = Vec::new();
        let now = Instant::now();

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    // Track held state for directional buttons.
                    if let Some(dir) = Self::button_to_direction(button) {
                        self.gamepad_repeat.held.insert(dir, now);
                        self.gamepad_repeat.last_repeat.remove(&dir);
                    }
                    if let Some(action) = Self::map_button(button) {
                        actions.push(action);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(dir) = Self::button_to_direction(button) {
                        self.gamepad_repeat.held.remove(&dir);
                        self.gamepad_repeat.last_repeat.remove(&dir);
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    let axis_actions = Self::update_axis(
                        &mut self.gamepad_axis,
                        axis,
                        value,
                        &mut self.gamepad_repeat,
                        now,
                    );
                    actions.extend(axis_actions);
                }
                _ => {}
            }
        }

        // Generate repeat actions for held directional inputs.
        let held_snapshot: Vec<(RepeatDirection, Instant)> = self
            .gamepad_repeat
            .held
            .iter()
            .map(|(&d, &t)| (d, t))
            .collect();
        for (dir, press_time) in held_snapshot {
            let elapsed = now.duration_since(press_time).as_millis();
            if elapsed < REPEAT_DELAY_MS {
                continue;
            }
            let should_fire = match self.gamepad_repeat.last_repeat.get(&dir) {
                Some(&last) => now.duration_since(last).as_millis() >= REPEAT_INTERVAL_MS,
                None => true,
            };
            if should_fire {
                actions.push(Self::direction_to_action(dir));
                self.gamepad_repeat.last_repeat.insert(dir, now);
            }
        }

        actions
    }

    /// Map a D-pad button to a logical repeat direction.
    fn button_to_direction(button: gilrs::Button) -> Option<RepeatDirection> {
        match button {
            gilrs::Button::DPadUp => Some(RepeatDirection::Up),
            gilrs::Button::DPadDown => Some(RepeatDirection::Down),
            gilrs::Button::DPadLeft => Some(RepeatDirection::Left),
            gilrs::Button::DPadRight => Some(RepeatDirection::Right),
            _ => None,
        }
    }

    /// Convert a repeat direction back to a browser action.
    fn direction_to_action(dir: RepeatDirection) -> BrowserAction {
        match dir {
            RepeatDirection::Up => BrowserAction::Up,
            RepeatDirection::Down => BrowserAction::Down,
            RepeatDirection::Left => BrowserAction::Left,
            RepeatDirection::Right => BrowserAction::Right,
        }
    }

    /// Map a gilrs button to a browser action.
    fn map_button(button: gilrs::Button) -> Option<BrowserAction> {
        match button {
            gilrs::Button::DPadUp => Some(BrowserAction::Up),
            gilrs::Button::DPadDown => Some(BrowserAction::Down),
            gilrs::Button::DPadLeft => Some(BrowserAction::Left),
            gilrs::Button::DPadRight => Some(BrowserAction::Right),
            gilrs::Button::East => Some(BrowserAction::Confirm), // Nintendo A button
            gilrs::Button::South => Some(BrowserAction::Back),   // Nintendo B button
            gilrs::Button::Start => Some(BrowserAction::Search),
            gilrs::Button::North => Some(BrowserAction::Detail), // Nintendo X button
            gilrs::Button::West => Some(BrowserAction::Favorite), // Nintendo Y button
            gilrs::Button::Select => Some(BrowserAction::GenreFilter),
            _ => None,
        }
    }

    /// Update axis state, return actions for new presses, and track repeat state.
    fn update_axis(
        state: &mut GamepadAxisState,
        axis: Axis,
        value: f32,
        repeat: &mut GamepadRepeatState,
        now: Instant,
    ) -> Vec<BrowserAction> {
        let mut actions = Vec::new();
        match axis {
            Axis::LeftStickX | Axis::RightStickX => {
                let new_left = value < -AXIS_DEAD_ZONE;
                let new_right = value > AXIS_DEAD_ZONE;
                if new_left && !state.left {
                    actions.push(BrowserAction::Left);
                    repeat.held.insert(RepeatDirection::Left, now);
                    repeat.last_repeat.remove(&RepeatDirection::Left);
                }
                if !new_left && state.left {
                    repeat.held.remove(&RepeatDirection::Left);
                    repeat.last_repeat.remove(&RepeatDirection::Left);
                }
                if new_right && !state.right {
                    actions.push(BrowserAction::Right);
                    repeat.held.insert(RepeatDirection::Right, now);
                    repeat.last_repeat.remove(&RepeatDirection::Right);
                }
                if !new_right && state.right {
                    repeat.held.remove(&RepeatDirection::Right);
                    repeat.last_repeat.remove(&RepeatDirection::Right);
                }
                state.left = new_left;
                state.right = new_right;
            }
            Axis::LeftStickY | Axis::RightStickY => {
                // gilrs on macOS: positive = up (Cartesian convention)
                let new_up = value > AXIS_DEAD_ZONE;
                let new_down = value < -AXIS_DEAD_ZONE;
                if new_up && !state.up {
                    actions.push(BrowserAction::Up);
                    repeat.held.insert(RepeatDirection::Up, now);
                    repeat.last_repeat.remove(&RepeatDirection::Up);
                }
                if !new_up && state.up {
                    repeat.held.remove(&RepeatDirection::Up);
                    repeat.last_repeat.remove(&RepeatDirection::Up);
                }
                if new_down && !state.down {
                    actions.push(BrowserAction::Down);
                    repeat.held.insert(RepeatDirection::Down, now);
                    repeat.last_repeat.remove(&RepeatDirection::Down);
                }
                if !new_down && state.down {
                    repeat.held.remove(&RepeatDirection::Down);
                    repeat.last_repeat.remove(&RepeatDirection::Down);
                }
                state.up = new_up;
                state.down = new_down;
            }
            _ => {}
        }
        actions
    }

    /// Apply a browser action (shared between keyboard and gamepad).
    fn apply_action(&mut self, action: BrowserAction, event_loop: &ActiveEventLoop) {
        if self.search_active {
            match action {
                BrowserAction::Back => self.search_active = false,
                BrowserAction::Up => self.navigate_up(),
                BrowserAction::Down => self.navigate_down(),
                BrowserAction::Confirm => {
                    if let Some(entry) = self.selected_entry() {
                        self.result = BrowserResult::RomSelected(entry.path.clone());
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        } else if self.genre_filter_active {
            match action {
                BrowserAction::Back => self.genre_filter_active = false,
                BrowserAction::Up => {
                    if self.genre_cursor > 0 {
                        self.genre_cursor -= 1;
                    }
                }
                BrowserAction::Down => {
                    if self.genre_cursor + 1 < self.available_genres.len() {
                        self.genre_cursor += 1;
                    }
                }
                BrowserAction::Confirm => {
                    if let Some(genre) = self.available_genres.get(self.genre_cursor).cloned() {
                        if let Some(pos) = self.active_genres.iter().position(|g| *g == genre) {
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
            match action {
                BrowserAction::Back => self.detail_view_active = false,
                BrowserAction::Confirm => {
                    if let Some(entry) = self.selected_entry() {
                        self.result = BrowserResult::RomSelected(entry.path.clone());
                        event_loop.exit();
                    }
                }
                BrowserAction::Favorite => self.toggle_favorite(),
                _ => {}
            }
        } else {
            match action {
                BrowserAction::Up => self.navigate_up(),
                BrowserAction::Down => self.navigate_down(),
                BrowserAction::Left => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                        self.ensure_selected_visible();
                    }
                }
                BrowserAction::Right => {
                    if self.selected_index + 1 < self.filtered_indices.len() {
                        self.selected_index += 1;
                        self.ensure_selected_visible();
                    }
                }
                BrowserAction::Confirm => {
                    // In grid mode, Confirm opens the detail view.
                    if self.selected_entry().is_some() {
                        self.detail_view_active = true;
                    }
                }
                BrowserAction::Back => {
                    if !self.search_query.is_empty() || !self.active_genres.is_empty() {
                        self.search_query.clear();
                        self.active_genres.clear();
                        self.rebuild_filtered();
                    } else {
                        self.result = BrowserResult::Closed;
                        event_loop.exit();
                    }
                }
                BrowserAction::Search => {
                    self.search_active = true;
                }
                BrowserAction::Favorite => self.toggle_favorite(),
                BrowserAction::Detail => {
                    if self.selected_entry().is_some() {
                        self.detail_view_active = true;
                    }
                }
                BrowserAction::GenreFilter => {
                    self.genre_filter_active = true;
                    self.genre_cursor = 0;
                }
            }
        }
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
                    let (
                        search_paths,
                        rebuild,
                        metadata_db_path,
                        image_cache_path,
                        include_unofficial,
                    ) = {
                        let ctx = self.app_context.borrow();
                        let config = ctx.config();
                        (
                            config.frontend.cartridge_search_paths.clone(),
                            config.frontend.rebuild_cartridge_catalog,
                            config.frontend.resolved_metadata_db_path(),
                            config.frontend.resolved_image_cache_path(),
                            config.frontend.include_unofficial_roms,
                        )
                    };
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        match crate::platform::catalog::load_catalog(
                            &search_paths,
                            rebuild,
                            include_unofficial,
                        ) {
                            Ok(mut catalog) => {
                                let tx2 = tx.clone();
                                crate::platform::catalog::enrich_catalog(
                                    &mut catalog,
                                    &metadata_db_path,
                                    &image_cache_path,
                                    rebuild,
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
                            // Open detail view; launch is done from the detail view.
                            if self.selected_entry().is_some() {
                                self.detail_view_active = true;
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
                // Poll gamepad events and apply browser actions.
                let actions = self.poll_gamepad();
                for action in actions {
                    self.apply_action(action, event_loop);
                }

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
            gilrs: None, // No gamepad in tests
            gamepad_axis: GamepadAxisState::default(),
            gamepad_repeat: GamepadRepeatState::default(),
            texture_request_tx: {
                let (tx, _rx) = mpsc::channel();
                tx
            },
            texture_result_rx: {
                let (_tx, rx) = mpsc::channel();
                rx
            },
            texture_pending: Vec::new(),
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
