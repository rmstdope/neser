//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! GL-backed window with imgui rendering and accepts a ROM selection that
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

    /// Run the ROM browser and return the result.
    ///
    /// Returns `BrowserResult::RomSelected` if a ROM was chosen, or
    /// `BrowserResult::Closed` if the user closed the window.
    pub fn run(mut self) -> Result<BrowserResult, String> {
        let event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        event_loop
            .run_app(&mut self)
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

    /// Render the browser UI for one frame.
    fn render_frame(&mut self) {
        // Poll background catalog loading.
        self.poll_catalog_loading();

        // Load textures on first frame after catalog arrives.
        if !self.textures_loaded && matches!(self.catalog_state, CatalogState::Ready) {
            self.load_cover_textures();
            self.textures_loaded = true;
        }

        let Some(ref mut gl) = self.gl else { return };

        // If still loading, render a loading screen with progress bar.
        if let CatalogState::Loading { ref progress, .. } = self.catalog_state {
            let progress_snapshot = progress.clone();
            let ui = gl.begin_frame();
            let (display_w, display_h) = (ui.io().display_size[0], ui.io().display_size[1]);
            ui.window("##loading")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([display_w, display_h], imgui::Condition::Always)
                .flags(Self::panel_flags())
                .build(|| {
                    let bar_w = (display_w * 0.55).max(320.0);
                    let bar_x = (display_w - bar_w) * 0.5;
                    let center_y = display_h * 0.45;

                    // Title.
                    let _title = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                    ui.set_cursor_pos([bar_x, center_y - 36.0]);
                    ui.text("NESER ROM Browser");
                    drop(_title);

                    ui.set_cursor_pos([bar_x, center_y - 16.0]);
                    let _dim = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);

                    if let Some(ref p) = progress_snapshot {
                        let fraction = if p.total > 0 {
                            p.current as f32 / p.total as f32
                        } else {
                            0.0
                        };
                        let phase_label = match p.phase {
                            EnrichmentPhase::MatchingMetadata => "Matching metadata",
                            EnrichmentPhase::DownloadingImages => "Downloading cover art",
                        };
                        let overlay = format!("{phase_label}: {} / {}", p.current, p.total);
                        ui.set_cursor_pos([bar_x, center_y - 16.0]);
                        ui.text(&overlay);
                        ui.set_cursor_pos([bar_x, center_y]);
                        imgui::ProgressBar::new(fraction)
                            .size([bar_w, 18.0])
                            .build(ui);
                        ui.set_cursor_pos([bar_x, center_y + 26.0]);
                        // Truncate long game titles.
                        let max_chars = (bar_w / 7.5) as usize;
                        let title = if p.game_title.len() > max_chars && max_chars > 3 {
                            format!("{}...", &p.game_title[..max_chars - 3])
                        } else {
                            p.game_title.clone()
                        };
                        ui.text(&title);
                    } else {
                        ui.text("Loading ROM catalog...");
                        ui.set_cursor_pos([bar_x, center_y]);
                        imgui::ProgressBar::new(0.0).size([bar_w, 18.0]).build(ui);
                    }
                });
            gl.end_frame();
            return;
        }

        let (display_w, display_h) = gl.logical_size();
        let total_count = self.catalog.len();
        let selected = self.selected_index;

        // Smooth scroll animation.
        let dt = gl.delta_time();
        self.scroll_offset +=
            (self.scroll_target - self.scroll_offset) * (theme::SCROLL_SPEED * dt).min(1.0);
        let scroll_offset = self.scroll_offset;

        // Pre-collect texture IDs for cover art rendering to avoid borrow conflicts.
        let tex_map: HashMap<i64, (imgui::TextureId, u32, u32)> = self
            .catalog
            .iter()
            .filter_map(|e| {
                let game_id = e.metadata_game_id?;
                let tex = gl.get_texture(&TextureKey::CoverArt(game_id))?;
                Some((game_id, (tex.imgui_id, tex.width, tex.height)))
            })
            .collect();

        // Layout calculations.
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let cover_h = cover_w / theme::COVER_ASPECT;

        // Build display entries from filtered indices (avoids borrow conflicts).
        let filtered_count = self.filtered_indices.len();
        let display_entries: Vec<&RomEntry> = self
            .filtered_indices
            .iter()
            .map(|&idx| &self.catalog[idx])
            .collect();

        // Selected catalog index for sidebar.
        let selected_entry_idx = self.filtered_indices.get(selected).copied();

        let search_active = self.search_active;
        let search_query = self.search_query.clone();
        let genre_filter_active = self.genre_filter_active;
        let available_genres = &self.available_genres;
        let active_genres = &self.active_genres;
        let genre_cursor = self.genre_cursor;
        let detail_view_active = self.detail_view_active;
        let show_favorites_only = self.show_favorites_only;

        let ui = gl.begin_frame();

        // --- Header bar ---
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

        ui.window("##header")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size(
                [grid_area_w, theme::HEADER_HEIGHT],
                imgui::Condition::Always,
            )
            .flags(Self::panel_flags())
            .build(|| {
                let _color = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                ui.text(&header_text);
            });

        // --- Grid area ---
        let grid_top = theme::HEADER_HEIGHT;
        let grid_height = display_h - theme::HEADER_HEIGHT - theme::FOOTER_HEIGHT;

        ui.window("##grid")
            .position([0.0, grid_top], imgui::Condition::Always)
            .size([grid_area_w, grid_height], imgui::Condition::Always)
            .flags(Self::panel_flags() | imgui::WindowFlags::NO_SCROLLBAR)
            .build(|| {
                let _text_color = ui.push_style_color(imgui::StyleColor::Text, theme::TEXT_COLOR);

                if display_entries.is_empty() {
                    if search_query.is_empty() {
                        ui.text("No ROMs found. Add ROM files to ~/.neser/roms/");
                        ui.text("or configure cartridge-search-paths in neser.conf");
                    } else {
                        ui.text(format!("No games match \"{search_query}\""));
                    }
                    return;
                }

                let total_rows = filtered_count.div_ceil(cols);
                let content_height =
                    total_rows as f32 * (cell_h + theme::GRID_SPACING) + theme::GRID_PADDING;

                ui.child_window("##grid_scroll")
                    .size([grid_area_w - 8.0, grid_height - 8.0])
                    .flags(imgui::WindowFlags::NO_SCROLLBAR)
                    .build(|| {
                        // Set scroll position from our smooth-scroll offset.
                        ui.set_scroll_y(scroll_offset);

                        // Invisible dummy to establish full content height.
                        let cursor_start = ui.cursor_pos();
                        ui.dummy([grid_area_w - 24.0, content_height]);
                        ui.set_cursor_pos(cursor_start);

                        let draw_list = ui.get_window_draw_list();
                        let window_pos = ui.window_pos();

                        for (i, entry) in display_entries.iter().enumerate() {
                            let row = i / cols;
                            let col = i % cols;

                            let x =
                                theme::GRID_PADDING + col as f32 * (cover_w + theme::GRID_SPACING);
                            let y =
                                theme::GRID_PADDING + row as f32 * (cell_h + theme::GRID_SPACING);

                            let abs_x = window_pos[0] + x;
                            let abs_y = window_pos[1] + y - scroll_offset;

                            // Skip cells that are off-screen.
                            if abs_y + cell_h < window_pos[1] || abs_y > window_pos[1] + grid_height
                            {
                                continue;
                            }

                            // Selection highlight.
                            if i == selected {
                                draw_list
                                    .add_rect(
                                        [abs_x - 3.0, abs_y - 3.0],
                                        [abs_x + cover_w + 3.0, abs_y + cell_h + 3.0],
                                        theme::SELECTION_COLOR,
                                    )
                                    .thickness(2.5)
                                    .rounding(4.0)
                                    .build();
                            }

                            // Cover art or placeholder.
                            Self::draw_cover(
                                &draw_list, entry, abs_x, abs_y, cover_w, cover_h, &tex_map,
                            );

                            // Game title below cover.
                            let title_y = abs_y + cover_h + 4.0;
                            let title_color = if i == selected {
                                theme::SELECTED_TEXT
                            } else {
                                theme::TEXT_COLOR
                            };
                            // Truncate title to fit cover width.
                            let max_chars = (cover_w / 8.0) as usize; // rough char width estimate
                            let title = if entry.display_name.len() > max_chars && max_chars > 3 {
                                format!("{}...", &entry.display_name[..max_chars - 3])
                            } else {
                                entry.display_name.clone()
                            };
                            draw_list.add_text([abs_x + 2.0, title_y], title_color, &title);

                            // Favourite heart indicator.
                            if entry.is_favorite {
                                draw_list.add_text(
                                    [abs_x + cover_w - 18.0, abs_y + 4.0],
                                    theme::FAVORITE_COLOR,
                                    "\u{2665}",
                                );
                            }
                        }
                    });
            });

        // --- Sidebar ---
        ui.window("##sidebar")
            .position([grid_area_w, 0.0], imgui::Condition::Always)
            .size([sidebar_w, display_h], imgui::Condition::Always)
            .flags(Self::panel_flags())
            .build(|| {
                let _bg = ui.push_style_color(imgui::StyleColor::WindowBg, theme::SIDEBAR_BG);
                if let Some(idx) = selected_entry_idx
                    && let Some(entry) = self.catalog.get(idx)
                {
                    Self::render_sidebar(ui, entry, sidebar_w);
                }
            });

        // --- Footer bar ---
        ui.window("##footer")
            .position([0.0, display_h - theme::FOOTER_HEIGHT], imgui::Condition::Always)
            .size([grid_area_w, theme::FOOTER_HEIGHT], imgui::Condition::Always)
            .flags(Self::panel_flags())
            .build(|| {
                let _color = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);
                if search_active {
                    ui.text("Type to search  |  Esc: Close search  |  Enter: Launch");
                } else if genre_filter_active {
                    ui.text(
                        "\u{2191}\u{2193}: Navigate  |  Enter: Toggle  |  Esc: Close filter",
                    );
                } else if detail_view_active {
                    ui.text("Enter: Launch  |  Esc: Back to grid");
                } else {
                    ui.text(
                        "Enter: Launch  |  /: Search  |  g: Genre  |  d: Details  |  f: Fav  |  F: Filter Favs  |  Esc: Quit",
                    );
                }
            });

        // --- Search overlay ---
        if search_active {
            let overlay_draw = ui.get_foreground_draw_list();
            let dim_color = imgui::ImColor32::from_rgba(0, 0, 0, 120);
            overlay_draw.add_rect_filled_multicolor(
                [0.0, 0.0],
                [display_w, display_h],
                dim_color,
                dim_color,
                dim_color,
                dim_color,
            );

            let search_w = (display_w * 0.5).clamp(300.0, 600.0);
            let search_h = 60.0;
            let search_x = (display_w - search_w) / 2.0;
            let search_y = 80.0;

            ui.window("##search_overlay")
                .position([search_x, search_y], imgui::Condition::Always)
                .size([search_w, search_h], imgui::Condition::Always)
                .flags(
                    Self::panel_flags()
                        | imgui::WindowFlags::NO_SCROLLBAR
                        | imgui::WindowFlags::NO_SAVED_SETTINGS,
                )
                .build(|| {
                    let _color = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                    ui.text(format!(
                        "Search: {search_query}\u{258C}    ({filtered_count} matches)"
                    ));
                });
        }

        // --- Genre filter overlay ---
        if genre_filter_active && !available_genres.is_empty() {
            let overlay_draw = ui.get_foreground_draw_list();
            let dim_color = imgui::ImColor32::from_rgba(0, 0, 0, 120);
            overlay_draw.add_rect_filled_multicolor(
                [0.0, 0.0],
                [display_w, display_h],
                dim_color,
                dim_color,
                dim_color,
                dim_color,
            );

            let panel_w = 300.0_f32.min(display_w * 0.4);
            let panel_h = (available_genres.len() as f32 * 26.0 + 60.0).min(display_h * 0.8);
            let panel_x = (display_w - panel_w) / 2.0;
            let panel_y = (display_h - panel_h) / 2.0;

            ui.window("##genre_filter")
                .position([panel_x, panel_y], imgui::Condition::Always)
                .size([panel_w, panel_h], imgui::Condition::Always)
                .flags(Self::panel_flags() | imgui::WindowFlags::NO_SAVED_SETTINGS)
                .build(|| {
                    let _header = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                    ui.text("Filter by Genre");
                    drop(_header);
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
                        let _c = ui.push_style_color(imgui::StyleColor::Text, color);

                        let label = if is_cursor {
                            format!("> {marker}{genre}")
                        } else {
                            format!("  {marker}{genre}")
                        };
                        ui.text(&label);
                    }
                });
        }

        // --- Detail view overlay ---
        if detail_view_active
            && let Some(idx) = selected_entry_idx
            && let Some(entry) = self.catalog.get(idx)
        {
            let overlay_draw = ui.get_foreground_draw_list();
            let dim_color = imgui::ImColor32::from_rgba(0, 0, 0, 200);
            overlay_draw.add_rect_filled_multicolor(
                [0.0, 0.0],
                [display_w, display_h],
                dim_color,
                dim_color,
                dim_color,
                dim_color,
            );

            let margin = 40.0;
            ui.window("##detail_view")
                .position([margin, margin], imgui::Condition::Always)
                .size(
                    [display_w - margin * 2.0, display_h - margin * 2.0],
                    imgui::Condition::Always,
                )
                .flags(Self::panel_flags() | imgui::WindowFlags::NO_SAVED_SETTINGS)
                .build(|| {
                    Self::render_detail_view(ui, entry, &tex_map);
                });
        }

        gl.end_frame();
    }

    /// Draw cover art or a placeholder rectangle.
    fn draw_cover(
        draw_list: &imgui::DrawListMut<'_>,
        entry: &RomEntry,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex_map: &HashMap<i64, (imgui::TextureId, u32, u32)>,
    ) {
        if let Some(game_id) = entry.metadata_game_id
            && let Some(&(tex_id, _, _)) = tex_map.get(&game_id)
        {
            draw_list.add_image(tex_id, [x, y], [x + w, y + h]).build();
            return;
        }

        // Placeholder: dark rectangle with game title.
        draw_list
            .add_rect([x, y], [x + w, y + h], theme::PLACEHOLDER_BG)
            .filled(true)
            .build();
        // Draw title centred in placeholder.
        let max_chars = (w / 7.0) as usize;
        let short = if entry.display_name.len() > max_chars && max_chars > 3 {
            format!("{}...", &entry.display_name[..max_chars - 3])
        } else {
            entry.display_name.clone()
        };
        draw_list.add_text([x + 6.0, y + h / 2.0 - 8.0], theme::DIM_TEXT, &short);
    }

    /// Render the metadata sidebar for the selected game.
    fn render_sidebar(ui: &imgui::Ui, entry: &RomEntry, _sidebar_w: f32) {
        let _header = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
        ui.text_wrapped(&entry.display_name);
        drop(_header);

        ui.spacing();
        ui.separator();
        ui.spacing();

        let _dim = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);
        if !entry.genres.is_empty() {
            ui.text(format!("Genre: {}", entry.genres.join(", ")));
        }
        if let Some(ref date) = entry.release_date {
            ui.text(format!("Released: {date}"));
        }
        if let Some(players) = entry.players {
            ui.text(format!("Players: {players}"));
        }
        if let Some(ref rating) = entry.rating {
            ui.text(format!("Rating: {rating}"));
        }
        drop(_dim);

        if let Some(ref overview) = entry.overview {
            ui.spacing();
            ui.separator();
            ui.spacing();
            let _text = ui.push_style_color(imgui::StyleColor::Text, theme::TEXT_COLOR);
            ui.text_wrapped(overview);
        }

        ui.spacing();
        ui.separator();
        ui.spacing();
        let _dim2 = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);
        ui.text(format!("Mapper: {}", entry.mapper_label));
        if let Some(ref crc) = entry.crc {
            ui.text(format!("CRC: {crc}"));
        }
        if let Some(ref hw) = entry.hardware {
            ui.text(format!("Hardware: {hw}"));
        }
        if entry.is_favorite {
            let _fav = ui.push_style_color(imgui::StyleColor::Text, theme::FAVORITE_COLOR);
            ui.text("\u{2665} Favourite");
        }
    }

    /// Render the full detail view for a selected game.
    fn render_detail_view(
        ui: &imgui::Ui,
        entry: &RomEntry,
        tex_map: &HashMap<i64, (imgui::TextureId, u32, u32)>,
    ) {
        let avail = ui.content_region_avail();

        // Two-column layout: left = boxart, right = metadata.
        let boxart_w = avail[0] * 0.35;
        let meta_x = boxart_w + 24.0;

        // --- Left: cover art ---
        if let Some(game_id) = entry.metadata_game_id
            && let Some(&(tex_id, _tw, _th)) = tex_map.get(&game_id)
        {
            let art_h = boxart_w / theme::COVER_ASPECT;
            let cursor = ui.cursor_pos();
            ui.set_cursor_pos(cursor);
            imgui::Image::new(tex_id, [boxart_w, art_h]).build(ui);
        }

        // --- Right: metadata ---
        ui.set_cursor_pos([meta_x, 0.0]);
        ui.child_window("##detail_meta")
            .size([avail[0] - meta_x - 8.0, avail[1]])
            .build(|| {
                // Title.
                let _title_color = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                ui.text_wrapped(&entry.display_name);
                drop(_title_color);

                ui.spacing();
                ui.separator();
                ui.spacing();

                // Metadata fields.
                let _dim = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);
                if !entry.genres.is_empty() {
                    ui.text(format!("Genre: {}", entry.genres.join(", ")));
                }
                if let Some(ref date) = entry.release_date {
                    ui.text(format!("Released: {date}"));
                }
                if let Some(players) = entry.players {
                    ui.text(format!("Players: {players}"));
                }
                if let Some(ref rating) = entry.rating {
                    ui.text(format!("Rating: {rating}"));
                }
                ui.text(format!("Mapper: {}", entry.mapper_label));
                if let Some(ref crc) = entry.crc {
                    ui.text(format!("CRC: {crc}"));
                }
                if let Some(ref hw) = entry.hardware {
                    ui.text(format!("Hardware: {hw}"));
                }
                if entry.is_favorite {
                    let _fav = ui.push_style_color(imgui::StyleColor::Text, theme::FAVORITE_COLOR);
                    ui.text("\u{2665} Favourite");
                }
                drop(_dim);

                // Description.
                if let Some(ref overview) = entry.overview {
                    ui.spacing();
                    ui.separator();
                    ui.spacing();
                    let _text = ui.push_style_color(imgui::StyleColor::Text, theme::TEXT_COLOR);
                    ui.text_wrapped(overview);
                }

                ui.spacing();
                ui.separator();
                ui.spacing();
                let _foot = ui.push_style_color(imgui::StyleColor::Text, theme::DIM_TEXT);
                ui.text("Enter: Launch  |  Esc: Back to grid");
            });
    }

    /// Common imgui window flags for the browser panels.
    fn panel_flags() -> imgui::WindowFlags {
        imgui::WindowFlags::NO_TITLE_BAR
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE
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
                        Key::Character(ref ch) if ch.as_str() == "g" => {
                            self.genre_filter_active = true;
                            self.genre_cursor = 0;
                        }
                        Key::Character(ref ch) if ch.as_str() == "d" => {
                            if self.selected_entry().is_some() {
                                self.detail_view_active = true;
                            }
                        }
                        Key::Character(ref ch) if ch.as_str() == "f" => {
                            self.toggle_favorite();
                        }
                        Key::Character(ref ch) if ch.as_str() == "F" => {
                            self.show_favorites_only = !self.show_favorites_only;
                            self.rebuild_filtered();
                        }
                        _ => {}
                    }
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
