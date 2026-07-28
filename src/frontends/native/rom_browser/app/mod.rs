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
    /// Active column in filter panel (0 = Platform, 1 = Players, 2 = Genre).
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
    /// Message shown when the catalog is empty, listing the ROM search paths.
    no_roms_hint: String,
}

impl RomBrowserApp {
    /// Create a new ROM browser application.
    pub fn new(app_context: SharedAppContext) -> Self {
        let (default_height, fullscreen, favorites_path, no_roms_hint) = {
            let ctx = app_context.borrow();
            let config = ctx.config();
            (
                config.frontend.window_height,
                config.frontend.fullscreen,
                config.frontend.resolved_favorites_path(),
                Self::no_roms_hint(&config.frontend.cartridge_search_paths),
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
            active_platform: None,
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
            no_roms_hint,
        }
    }

    /// Build the empty-catalog message from the configured cartridge search
    /// paths, mirroring the fallback used by the catalog scan: when no paths
    /// are configured, ROMs are read from ~/.neser/roms/.
    fn no_roms_hint(search_paths: &[String]) -> String {
        if search_paths.is_empty() {
            "No ROMs found. Add ROM files to ~/.neser/roms/".to_string()
        } else {
            format!(
                "No ROMs found. Add ROM files to {}",
                search_paths.join(", ")
            )
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

    /// Maximum number of decode requests to send per frame.
    const MAX_REQUESTS_PER_FRAME: usize = 8;
    /// Maximum number of decoded results to upload per frame.
    const MAX_UPLOADS_PER_FRAME: usize = 4;
    /// How many rows of buffer above/below the viewport to preload.
    const PRELOAD_ROW_BUFFER: usize = 2;
    /// Maximum number of textures to keep in memory.
    const MAX_CACHED_TEXTURES: usize = 200;

    /// Available platform options shown in the filter panel (Platform column).
    const PLATFORMS: [Platform; 5] = [
        Platform::Nes,
        Platform::Gb,
        Platform::Gbc,
        Platform::Gba,
        Platform::Snes,
    ];

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
}

mod actions;
mod catalog_state;
mod handler;
mod input;
mod render;
mod textures;

#[cfg(test)]
mod tests;
