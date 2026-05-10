//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! GL-backed window with imgui rendering and accepts a ROM selection that
//! transitions the application into emulation mode.

use std::collections::HashMap;
use std::path::PathBuf;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use super::renderer::{BrowserGl, TextureKey};
use super::theme;
use crate::platform::app_context::SharedAppContext;
use crate::platform::catalog::RomEntry;

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
    selected_index: usize,
    /// Current scroll offset (logical pixels from top).
    scroll_offset: f32,
    /// Target scroll offset for smooth scrolling.
    scroll_target: f32,
    /// Tracks whether cover art textures have been loaded.
    textures_loaded: bool,
}

impl RomBrowserApp {
    /// Create a new ROM browser application.
    pub fn new(app_context: SharedAppContext) -> Self {
        let (default_height, fullscreen) = {
            let ctx = app_context.borrow();
            let config = ctx.config();
            (config.frontend.window_height, config.frontend.fullscreen)
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
            selected_index: 0,
            scroll_offset: 0.0,
            scroll_target: 0.0,
            textures_loaded: false,
        }
    }

    /// Set the ROM catalog to display.
    pub fn set_catalog(&mut self, catalog: Vec<RomEntry>) {
        self.catalog = catalog;
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

    /// Render the browser UI for one frame.
    fn render_frame(&mut self) {
        // Load textures on first frame (needs &mut self before splitting borrows).
        if !self.textures_loaded {
            self.load_cover_textures();
            self.textures_loaded = true;
        }

        let Some(ref mut gl) = self.gl else { return };

        let (display_w, display_h) = gl.logical_size();
        let catalog_len = self.catalog.len();
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

        // Extract catalog slice reference before begin_frame to avoid borrow conflicts.
        let catalog = &self.catalog;

        let ui = gl.begin_frame();

        // --- Header bar ---
        ui.window("##header")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size(
                [grid_area_w, theme::HEADER_HEIGHT],
                imgui::Condition::Always,
            )
            .flags(Self::panel_flags())
            .build(|| {
                let _color = ui.push_style_color(imgui::StyleColor::Text, theme::HEADER_TEXT);
                ui.text(format!("NESER ROM Browser — {catalog_len} games"));
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

                if catalog.is_empty() {
                    ui.text("No ROMs found. Add ROM files to ~/.neser/roms/");
                    ui.text("or configure cartridge-search-paths in neser.conf");
                    return;
                }

                // Use a child window so we can control scroll position.
                let total_rows = catalog_len.div_ceil(cols);
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

                        for (i, entry) in catalog.iter().enumerate() {
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
                if let Some(entry) = catalog.get(selected) {
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
                ui.text(
                    "Enter/A: Launch  |  \u{2190}\u{2191}\u{2192}\u{2193}: Navigate  |  Esc/B: Quit",
                );
            });

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
        let new_idx = self.selected_index + cols;
        if new_idx < self.catalog.len() {
            self.selected_index = new_idx;
        } else if self.selected_index < self.catalog.len().saturating_sub(1) {
            // Jump to last item if the target row doesn't exist.
            self.selected_index = self.catalog.len() - 1;
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
                // Load the ROM catalog.
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
                match crate::platform::catalog::load_catalog(&search_paths, rebuild) {
                    Ok(mut catalog) => {
                        // Enrich with metadata and cover art.
                        crate::platform::catalog::enrich_catalog(
                            &mut catalog,
                            &metadata_db_path,
                            &image_cache_path,
                            |_progress| {
                                // TODO: render progress bar (startup-progress todo)
                            },
                        );
                        catalog.sort_by(|a, b| {
                            a.display_name
                                .to_lowercase()
                                .cmp(&b.display_name.to_lowercase())
                        });
                        self.catalog = catalog;
                    }
                    Err(e) => {
                        eprintln!("Failed to load ROM catalog: {e}");
                    }
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
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.result = BrowserResult::Closed;
                        event_loop.exit();
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
                        if self.selected_index + 1 < self.catalog.len() {
                            self.selected_index += 1;
                            self.ensure_selected_visible();
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        if let Some(entry) = self.catalog.get(self.selected_index) {
                            self.result = BrowserResult::RomSelected(entry.path.clone());
                            event_loop.exit();
                        }
                    }
                    _ => {}
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
}
