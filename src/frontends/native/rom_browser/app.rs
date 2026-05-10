//! ROM browser winit application handler.
//!
//! This is the `ApplicationHandler` for the ROM browser window. It opens a
//! GL-backed window with imgui rendering and accepts a ROM selection that
//! transitions the application into emulation mode.

use std::path::PathBuf;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use super::renderer::BrowserGl;
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
        let Some(ref mut gl) = self.gl else { return };

        let (display_w, display_h) = gl.logical_size();
        let catalog_len = self.catalog.len();
        let selected = self.selected_index;

        let ui = gl.begin_frame();

        // Full-window background panel.
        ui.window("##browser_bg")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([display_w, display_h], imgui::Condition::Always)
            .flags(
                imgui::WindowFlags::NO_TITLE_BAR
                    | imgui::WindowFlags::NO_RESIZE
                    | imgui::WindowFlags::NO_MOVE
                    | imgui::WindowFlags::NO_SCROLLBAR
                    | imgui::WindowFlags::NO_COLLAPSE
                    | imgui::WindowFlags::NO_BACKGROUND,
            )
            .build(|| {
                if self.catalog.is_empty() {
                    ui.text("No ROMs found. Add ROM files to ~/.neser/roms/");
                    ui.text("or configure cartridge-search-paths in neser.conf");
                } else {
                    ui.text(format!("NESER ROM Browser — {catalog_len} games"));
                    ui.separator();

                    let avail = ui.content_region_avail();
                    ui.child_window("##rom_list")
                        .size([avail[0], avail[1] - 30.0])
                        .build(|| {
                            for (i, entry) in self.catalog.iter().enumerate() {
                                if ui
                                    .selectable_config(&entry.display_name)
                                    .selected(i == selected)
                                    .build()
                                {
                                    self.selected_index = i;
                                }
                            }
                        });

                    ui.separator();
                    ui.text("Enter: Launch  |  ↑↓: Navigate  |  Esc: Quit");
                }
            });

        gl.end_frame();
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
                        if self.selected_index > 0 {
                            self.selected_index -= 1;
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if self.selected_index + 1 < self.catalog.len() {
                            self.selected_index += 1;
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
