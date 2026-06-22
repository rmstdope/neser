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
use crate::platform::catalog::Platform;
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
    /// Tracks the instant of the last render_frame call for frame-to-frame dt.
    last_render_instant: Instant,
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
    /// Animation progress for search panel slide (0.0 = hidden, 1.0 = fully shown).
    search_anim: f32,
    /// On-screen keyboard cursor position (row, col).
    search_kb_row: usize,
    search_kb_col: usize,
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
    /// Currently selected screenshot index in the detail view.
    detail_screenshot_index: usize,
    /// Instant when the last screenshot auto-scroll advance happened.
    detail_scroll_last_advance: Instant,
    /// Whether auto-scroll is going forward (true) or backward (false).
    detail_scroll_forward: bool,
    /// Filter panel overlay active.
    filter_panel_active: bool,
    /// Animation progress for filter panel slide (0.0 = hidden, 1.0 = fully shown).
    filter_panel_anim: f32,
    /// Cursor position within the current filter panel column.
    filter_panel_cursor: usize,
    /// Active column in filter panel (0 = Platform, 1 = Genre, 2 = Players).
    filter_panel_column: usize,
    /// Active platform filter (`None` = show all platforms).
    active_platform: Option<crate::platform::catalog::Platform>,
    /// Minimum number of players filter (`None` = any, `Some(2)` = 2+, etc.).
    min_players_filter: Option<u32>,
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
    /// Fast lookup from game_id to boxart path (built when catalog is set).
    boxart_by_game_id: std::collections::HashMap<i64, PathBuf>,
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
                    match image::open(&path) {
                        Ok(img) => {
                            let rgba = img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            let pixels = rgba.into_raw();
                            if result_tx.send((game_id, w, h, pixels)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            // Send a zero-size result so the pending entry is cleared.
                            if result_tx.send((game_id, 0, 0, Vec::new())).is_err() {
                                break;
                            }
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
            last_render_instant: Instant::now(),
            catalog: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            scroll_offset: 0.0,
            scroll_target: 0.0,
            search_active: false,
            search_query: String::new(),
            search_anim: 0.0,
            search_kb_row: 1,
            search_kb_col: 0,
            genre_filter_active: false,
            available_genres: Vec::new(),
            active_genres: Vec::new(),
            genre_cursor: 0,
            detail_view_active: false,
            detail_screenshot_index: 0,
            detail_scroll_last_advance: Instant::now(),
            detail_scroll_forward: true,
            filter_panel_active: false,
            filter_panel_anim: 0.0,
            filter_panel_cursor: 0,
            filter_panel_column: 0,
            active_platform: Some(crate::platform::catalog::Platform::Nes),
            min_players_filter: None,
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
            boxart_by_game_id: std::collections::HashMap::new(),
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

        // Build fast game_id→boxart_path lookup.
        self.boxart_by_game_id.clear();
        for entry in &self.catalog {
            if let (Some(gid), Some(path)) = (entry.metadata_game_id, &entry.boxart_path) {
                self.boxart_by_game_id.insert(gid, path.clone());
            }
        }

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
                // Platform filter.
                if matches!(self.active_platform, Some(plat) if e.platform != plat) {
                    return false;
                }
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
                // Players filter.
                if let Some(min) = self.min_players_filter
                    && e.players.unwrap_or(1) < min
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
        let genre_filter_active = self.genre_filter_active;
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
        let genre_cursor = self.genre_cursor;
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
                                let legend_items: &[(&str, &str)] = if search_active {
                                    &[("Tab", "Close"), ("Enter", "Launch")]
                                } else if genre_filter_active {
                                    &[("↑↓", "Navigate"), ("Enter", "Toggle"), ("Esc", "Close")]
                                } else if filter_panel_active {
                                    &[("↑↓", "Navigate"), ("A", "Toggle"), ("B", "Close")]
                                } else if detail_view_active {
                                    &[("A", "Launch"), ("Y", "Fav"), ("B", "Back")]
                                } else {
                                    &[
                                        ("A", "Details"),
                                        ("B", "Filter"),
                                        ("Select", "Favorite"),
                                        ("Tab", "Search"),
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
            if filter_panel_active || filter_panel_anim > 0.0 {
                Self::render_filter_panel_egui(
                    ui.ctx(),
                    &available_genres,
                    &active_genres,
                    active_platform,
                    min_players_filter,
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

    #[allow(clippy::too_many_arguments)]
    fn render_search_panel_egui(
        ctx: &egui::Context,
        query: &str,
        count: usize,
        kb_row: usize,
        kb_col: usize,
        anim: f32,
        display_w: f32,
        display_h: f32,
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
                    egui::Stroke::new(1.5, accent),
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
                                (accent, egui::Color32::WHITE, egui::Stroke::new(2.0, accent))
                            } else {
                                (
                                    egui::Color32::from_rgb(44, 44, 58),
                                    egui::Color32::from_rgb(200, 200, 215),
                                    egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)),
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
                    &[("Tab", "Close"), ("Enter", "Select"), ("Type", "Search")],
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

    #[allow(clippy::too_many_arguments)]
    fn render_filter_panel_egui(
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

    fn render_detail_view_egui(
        ctx: &egui::Context,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (egui::TextureId, u32, u32)>,
        screenshot_textures: &[(egui::TextureId, u32, u32)],
        screenshot_index: usize,
        display_w: f32,
        display_h: f32,
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
                                    &[("A", "Launch"), ("Y", "Fav"), ("B", "Back")],
                                );
                            });
                        });
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
                    if *w > 0 && *h > 0 {
                        let key = TextureKey::CoverArt(*game_id);
                        gl.load_texture_from_rgba(key, *w, *h, pixels);
                    }
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
            if let Some(path) = self.boxart_by_game_id.get(&game_id)
                && path.exists()
                && self
                    .texture_request_tx
                    .send((game_id, path.clone()))
                    .is_ok()
            {
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

    /// Get the number of screenshots for the currently selected entry.
    fn detail_screenshot_count(&self) -> usize {
        self.selected_entry()
            .map(|e| e.screenshot_paths.len())
            .unwrap_or(0)
    }

    /// Advance the auto-scroll carousel for screenshots in the detail view.
    /// Pauses longer at the first and last screenshot, then reverses direction.
    fn advance_screenshot_auto_scroll(&mut self) {
        let count = self.detail_screenshot_count();
        if count <= 1 {
            return;
        }
        let elapsed = self.detail_scroll_last_advance.elapsed().as_secs_f64();
        let pause =
            if self.detail_screenshot_index == 0 || self.detail_screenshot_index == count - 1 {
                2.0
            } else {
                1.5
            };
        if elapsed >= pause {
            self.detail_scroll_last_advance = Instant::now();
            if self.detail_scroll_forward {
                if self.detail_screenshot_index + 1 < count {
                    self.detail_screenshot_index += 1;
                } else {
                    self.detail_scroll_forward = false;
                    self.detail_screenshot_index -= 1;
                }
            } else if self.detail_screenshot_index > 0 {
                self.detail_screenshot_index -= 1;
            } else {
                self.detail_scroll_forward = true;
                self.detail_screenshot_index += 1;
            }
        }
    }

    /// Open the detail view for the currently selected entry.
    fn open_detail_view(&mut self) {
        if self.selected_entry().is_some() {
            self.detail_view_active = true;
            self.detail_screenshot_index = 0;
            self.detail_scroll_last_advance = Instant::now();
            self.detail_scroll_forward = true;
        }
    }

    /// Open the search panel overlay.
    fn open_search_panel(&mut self) {
        self.filter_panel_active = false;
        self.genre_filter_active = false;
        self.detail_view_active = false;
        self.search_active = true;
        self.search_kb_row = 1;
        self.search_kb_col = 0;
    }

    /// Close the search panel overlay.
    fn close_search_panel(&mut self) {
        self.search_active = false;
    }

    /// Handle confirm action on the on-screen keyboard.
    fn search_kb_confirm(&mut self) {
        let row = &Self::SEARCH_KB_ROWS[self.search_kb_row];
        if self.search_kb_col < row.len() {
            let ch = row[self.search_kb_col];
            match ch {
                '\u{232B}' => {
                    // Backspace
                    self.search_query.pop();
                }
                '\u{21B5}' => {
                    // Enter — close search
                    self.close_search_panel();
                    return;
                }
                _ => {
                    self.search_query.push(ch.to_ascii_lowercase());
                }
            }
            self.rebuild_filtered();
        }
    }

    /// Move on-screen keyboard cursor up.
    fn search_kb_move_up(&mut self) {
        if self.search_kb_row > 0 {
            self.search_kb_row -= 1;
            let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
            if self.search_kb_col >= row_len {
                self.search_kb_col = row_len - 1;
            }
        }
    }

    /// Move on-screen keyboard cursor down.
    fn search_kb_move_down(&mut self) {
        if self.search_kb_row + 1 < Self::SEARCH_KB_ROWS.len() {
            self.search_kb_row += 1;
            let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
            if self.search_kb_col >= row_len {
                self.search_kb_col = row_len - 1;
            }
        }
    }

    /// Move on-screen keyboard cursor left.
    fn search_kb_move_left(&mut self) {
        if self.search_kb_col > 0 {
            self.search_kb_col -= 1;
        }
    }

    /// Move on-screen keyboard cursor right.
    fn search_kb_move_right(&mut self) {
        let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
        if self.search_kb_col + 1 < row_len {
            self.search_kb_col += 1;
        }
    }

    /// Open the filter panel overlay.
    fn open_filter_panel(&mut self) {
        self.search_active = false;
        self.genre_filter_active = false;
        self.detail_view_active = false;
        self.filter_panel_active = true;
        self.filter_panel_cursor = 0;
        self.filter_panel_column = 0;
    }

    /// Close the filter panel overlay.
    fn close_filter_panel(&mut self) {
        self.filter_panel_active = false;
    }

    /// Total number of selectable items in the filter panel
    /// (platform options + genre options).
    const PLATFORMS: [Platform; 3] = [Platform::Nes, Platform::Gb, Platform::Gbc];

    const PLAYER_OPTIONS: [(Option<u32>, &'static str); 3] =
        [(None, "Any"), (Some(2), "2+"), (Some(4), "4+")];

    /// On-screen QWERTY keyboard layout for search.
    const SEARCH_KB_ROWS: [&'static [char]; 4] = [
        &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
        &['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
        &['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', '-'],
        &[
            'Z', 'X', 'C', 'V', 'B', 'N', 'M', ' ', '\u{232B}', '\u{21B5}',
        ],
    ];

    /// Handle confirm action within the filter panel.
    fn filter_panel_confirm(&mut self) {
        let cursor = self.filter_panel_cursor;
        match self.filter_panel_column {
            0 => {
                // Platform column
                if cursor < Self::PLATFORMS.len() {
                    let selected = Self::PLATFORMS[cursor];
                    if self.active_platform == Some(selected) {
                        self.active_platform = None;
                    } else {
                        self.active_platform = Some(selected);
                    }
                }
            }
            1 => {
                // Players column
                if cursor < Self::PLAYER_OPTIONS.len() {
                    let (value, _) = Self::PLAYER_OPTIONS[cursor];
                    if self.min_players_filter == value && value.is_some() {
                        self.min_players_filter = None;
                    } else {
                        self.min_players_filter = value;
                    }
                }
            }
            2 => {
                // Genre column
                if let Some(genre) = self.available_genres.get(cursor).cloned() {
                    if let Some(pos) = self.active_genres.iter().position(|g| *g == genre) {
                        self.active_genres.remove(pos);
                    } else {
                        self.active_genres.push(genre);
                    }
                }
            }
            _ => {}
        }
        self.rebuild_filtered();
    }

    /// Move the filter panel cursor up within the current column.
    fn filter_panel_move_cursor_up(&mut self) {
        if self.filter_panel_cursor > 0 {
            self.filter_panel_cursor -= 1;
        }
    }

    /// Move the filter panel cursor down within the current column.
    fn filter_panel_move_cursor_down(&mut self) {
        let max = self
            .filter_panel_column_len(self.filter_panel_column)
            .saturating_sub(1);
        if self.filter_panel_cursor < max {
            self.filter_panel_cursor += 1;
        }
    }

    /// Return the number of items in a given filter panel column.
    fn filter_panel_column_len(&self, col: usize) -> usize {
        match col {
            0 => Self::PLATFORMS.len(),
            1 => Self::PLAYER_OPTIONS.len(),
            2 => self.available_genres.len(),
            _ => 0,
        }
    }

    /// Move the filter panel to the previous column.
    fn filter_panel_move_left(&mut self) {
        if self.filter_panel_column > 0 {
            self.filter_panel_column -= 1;
            let max = self
                .filter_panel_column_len(self.filter_panel_column)
                .saturating_sub(1);
            self.filter_panel_cursor = self.filter_panel_cursor.min(max);
        }
    }

    /// Move the filter panel to the next column.
    fn filter_panel_move_right(&mut self) {
        let next = self.filter_panel_column + 1;
        let next_len = self.filter_panel_column_len(next);
        if next_len > 0 {
            self.filter_panel_column = next;
            self.filter_panel_cursor = self.filter_panel_cursor.min(next_len.saturating_sub(1));
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
            gilrs::Button::Start | gilrs::Button::RightTrigger2 => Some(BrowserAction::Search),
            gilrs::Button::North => Some(BrowserAction::Detail), // Nintendo X button
            gilrs::Button::West => Some(BrowserAction::GenreFilter), // Nintendo Y button
            gilrs::Button::Select | gilrs::Button::LeftTrigger2 => Some(BrowserAction::Favorite),
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
                BrowserAction::Back => self.close_search_panel(),
                BrowserAction::Up => self.search_kb_move_up(),
                BrowserAction::Down => self.search_kb_move_down(),
                BrowserAction::Left => self.search_kb_move_left(),
                BrowserAction::Right => self.search_kb_move_right(),
                BrowserAction::Confirm => self.search_kb_confirm(),
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
        } else if self.filter_panel_active {
            match action {
                BrowserAction::Back => self.close_filter_panel(),
                BrowserAction::Up => self.filter_panel_move_cursor_up(),
                BrowserAction::Down => self.filter_panel_move_cursor_down(),
                BrowserAction::Left => self.filter_panel_move_left(),
                BrowserAction::Right => self.filter_panel_move_right(),
                BrowserAction::Confirm => self.filter_panel_confirm(),
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
                    self.open_detail_view();
                }
                BrowserAction::Back => {
                    self.open_filter_panel();
                }
                BrowserAction::Search => {
                    self.open_search_panel();
                }
                BrowserAction::Favorite => self.toggle_favorite(),
                BrowserAction::Detail => {
                    self.open_detail_view();
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

                // Ctrl+Q always quits regardless of active overlay.
                let ctrl = self
                    .modifiers
                    .contains(winit::keyboard::ModifiersState::CONTROL)
                    || self
                        .modifiers
                        .contains(winit::keyboard::ModifiersState::SUPER);
                if let Key::Character(ref ch) = event.logical_key
                    && (ch.as_str() == "q" || ch.as_str() == "Q")
                    && ctrl
                {
                    self.result = BrowserResult::Closed;
                    event_loop.exit();
                    return;
                }

                // Ctrl+F always toggles fullscreen regardless of active overlay.
                if let Key::Character(ref ch) = event.logical_key
                    && (ch.as_str() == "f" || ch.as_str() == "F")
                    && ctrl
                {
                    if let Some(ref gl) = self.gl {
                        let window = gl.window();
                        if window.fullscreen().is_some() {
                            window.set_fullscreen(None);
                        } else {
                            window
                                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                    return;
                }

                // Tab toggles search from any screen.
                if matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                    if self.search_active {
                        self.close_search_panel();
                    } else {
                        // Close any other overlay first.
                        self.filter_panel_active = false;
                        self.genre_filter_active = false;
                        self.detail_view_active = false;
                        self.open_search_panel();
                    }
                    return;
                }

                if self.search_active {
                    // Search mode input handling.
                    // Physical keyboard typing works directly; arrows move
                    // the on-screen keyboard cursor for gamepad users.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.close_search_panel();
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.search_query.pop();
                            self.rebuild_filtered();
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.search_kb_confirm();
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.search_kb_move_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.search_kb_move_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.search_kb_move_left();
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.search_kb_move_right();
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
                } else if self.filter_panel_active {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.close_filter_panel();
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.filter_panel_move_cursor_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.filter_panel_move_cursor_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.filter_panel_move_left();
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.filter_panel_move_right();
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            self.filter_panel_confirm();
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
                        Key::Named(NamedKey::ArrowUp)
                        | Key::Named(NamedKey::ArrowLeft)
                        | Key::Named(NamedKey::ArrowDown)
                        | Key::Named(NamedKey::ArrowRight) => {
                            // Screenshots auto-scroll; arrow keys are ignored.
                        }
                        _ => {}
                    }
                } else {
                    // Normal browsing mode.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.open_filter_panel();
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
                            self.open_detail_view();
                        }
                        Key::Character(ref ch) if ch.as_str() == "g" && !ctrl => {
                            self.genre_filter_active = true;
                            self.genre_cursor = 0;
                        }
                        Key::Character(ref ch) if ch.as_str() == "d" && !ctrl => {
                            self.open_detail_view();
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
mod tests;
