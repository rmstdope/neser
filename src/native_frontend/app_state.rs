//! Runtime state for the native frontend.
//!
//! [`NativeAppState`] centralises all mutable UI/emulator state so that the
//! keyboard handler, event loop, and rendering code share a single source of
//! truth without borrowing individual fields piecemeal.

use crate::autorun::state::AutorunState;
use crate::console::{AutorunMode, Nes, TimingMode};
use winit::keyboard::ModifiersState;

/// State for the in-game cartridge-switch dialog.
#[derive(Default)]
pub struct CartridgeSwitchState {
    /// Whether the dialog is currently visible.
    pub open: bool,

    /// All ROM paths loaded from the catalog CSV.
    pub entries: Vec<String>,

    /// Text currently typed into the filter box.
    pub filter: String,

    /// Index of the currently highlighted entry.
    pub selection: usize,
}

impl CartridgeSwitchState {
    /// Closes the dialog and resets ephemeral filter/selection state.
    pub fn close(&mut self) {
        self.open = false;
        self.filter.clear();
        self.selection = 0;
    }
}

/// All runtime state owned by the native frontend event loop.
#[derive(Default)]
pub struct NativeAppState {
    /// Whether the emulator is currently paused.
    pub paused: bool,

    /// Whether the window is currently in fullscreen mode.
    pub fullscreen: bool,

    /// Whether the debugger overlay is open.
    pub debugger_open: bool,

    /// Whether the help overlay is currently visible.
    pub help_overlay_visible: bool,

    /// Whether the mouse cursor is currently grabbed (relative mode).
    pub mouse_grabbed: bool,

    /// Set when the user presses Escape to release the mouse grab.
    /// Prevents auto-re-grab until the next left-click.
    pub mouse_released_by_escape: bool,

    /// Whether the window currently has focus.
    pub window_focused: bool,

    /// Virtual cursor position in logical pixels, accumulated from raw
    /// `DeviceEvent::MouseMotion` deltas when the cursor is locked.
    /// Used for Zapper and Arkanoid absolute-position mapping while
    /// the real cursor is kept at the window centre via `Locked` grab.
    pub virtual_cursor: (f32, f32),

    /// Last known Zapper position in NES coordinates for crosshair rendering.
    pub last_zapper_position: Option<(u8, u8)>,

    /// Current state of modifier keys (Ctrl, Shift, Alt, …).
    pub modifiers: ModifiersState,

    /// State of the in-game cartridge-switch dialog.
    pub cart_switch: CartridgeSwitchState,
}

impl NativeAppState {
    /// Returns the overlay text to display on top of the rendered frame, if any.
    ///
    /// Priority (highest first):
    /// 1. Cartridge-switch dialog (always shown when open, even with no entries).
    /// 2. Autorun status (playback/recording progress).
    /// 3. Help overlay.
    pub fn overlay_text(&self, nes: &Nes, autorun_state: Option<&AutorunState>) -> Option<String> {
        if self.cart_switch.open {
            return Some(cart_switch_overlay_text(&self.cart_switch));
        }
        if let Some(autorun) = autorun_state {
            let tv_system = nes
                .app_context()
                .borrow()
                .config()
                .hardware_model
                .timing_mode();
            return Some(autorun_overlay_text(autorun, tv_system));
        }
        if self.help_overlay_visible {
            return Some(help_overlay_text());
        }
        None
    }
}

fn help_overlay_text() -> String {
    "Controls\n\
Ctrl+Q: Quit\n\
Space: Pause\n\
H: Toggle help\n\
\n\
System\n\
Ctrl+R: Soft reset\n\
Shift+Ctrl+R: Hard reset\n\
Ctrl+F: Toggle fullscreen\n\
Ctrl+O: Switch cartridge\n\
F2/F3: Volume up/down\n\
F4: Cycle shader\n\
F5: Debugger (open/continue)\n\
F6: Save state\n\
F7: Load state\n\
F10: Step over\n\
F11: Step into\n\
\n\
Controller (Player 1)\n\
W/A/S/D: D-Pad\n\
R: A\n\
T: B\n\
4: Select\n\
5: Start\n\
\n\
Controller (Player 2)\n\
I/J/K/L: D-Pad\n\
O: A\n\
P: B\n\
9: Select\n\
0: Start"
        .to_string()
}

fn cart_switch_overlay_text(cart_switch: &CartridgeSwitchState) -> String {
    if cart_switch.entries.is_empty() {
        return "Cartridge Switch\n[No catalog loaded]\n\nPress Escape to cancel".to_string();
    }

    // TODO: cache the filtered list and rendered lines in CartridgeSwitchState
    // and only recompute when entries/filter/selection changes, to avoid
    // allocating a Vec at 60fps while the dialog is open with a large catalog.
    let filter_lower = cart_switch.filter.to_lowercase();
    let visible: Vec<&str> = cart_switch
        .entries
        .iter()
        .map(String::as_str)
        .filter(|e| cart_switch.filter.is_empty() || e.to_lowercase().contains(&filter_lower))
        .collect();

    let mut lines = vec!["Cartridge Switch".to_string()];
    if !cart_switch.filter.is_empty() {
        lines.push(format!("Filter: {}", cart_switch.filter));
    }
    lines.push(String::new());

    for (i, entry) in visible.iter().enumerate() {
        let marker = if i == cart_switch.selection {
            "> "
        } else {
            "  "
        };
        lines.push(format!("{marker}{entry}"));
    }

    lines.push(String::new());
    lines.push("Enter: Load  Escape: Cancel".to_string());
    lines.join("\n")
}

/// Generate autorun overlay text showing playback/recording progress.
fn autorun_overlay_text(autorun_state: &AutorunState, tv_system: TimingMode) -> String {
    match autorun_state.mode() {
        AutorunMode::Playback => {
            let current = autorun_state.current_frame_index();
            let total = autorun_state.total_frames();
            let (elapsed, total_str) = format_time_pair(current, total, tv_system);
            format!("Playback\n{elapsed} / {total_str}")
        }
        AutorunMode::Record => {
            if autorun_state.is_extending_playback() {
                let current = autorun_state.current_frame_index();
                let total = autorun_state.total_frames();
                let (elapsed, total_str) = format_time_pair(current, total, tv_system);
                format!("Playback\n{elapsed} / {total_str}")
            } else {
                let current = autorun_state.total_frames();
                let (elapsed, _) = format_time_pair(current, current, tv_system);
                format!("Recording\n{elapsed} / {elapsed}")
            }
        }
        AutorunMode::None => String::new(),
    }
}

fn format_time_pair(
    current_frames: usize,
    total_frames: usize,
    tv_system: TimingMode,
) -> (String, String) {
    let fps = tv_system.frame_rate_hz().round().max(1.0) as usize;
    let current_secs = current_frames / fps;
    let total_secs = total_frames / fps;
    (format_mm_ss(current_secs), format_mm_ss(total_secs))
}

fn format_mm_ss(seconds: usize) -> String {
    let minutes = seconds / 60;
    let secs = seconds % 60;
    format!("{minutes:02}:{secs:02}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::console::Config;

    fn make_nes() -> Nes {
        Nes::new(AppContext::new_with_config(Config::default()))
    }

    // ── overlay_text: help overlay ────────────────────────────────────────────

    #[test]
    fn test_overlay_text_returns_none_when_nothing_visible() {
        let state = NativeAppState::default();
        assert!(state.overlay_text(&make_nes(), None).is_none());
    }

    #[test]
    fn test_overlay_text_returns_controls_when_help_visible() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes(), None);
        assert!(
            text.is_some(),
            "overlay_text should be Some when help is visible"
        );
        assert!(
            text.unwrap().contains("Controls"),
            "help overlay must contain 'Controls' section"
        );
    }

    #[test]
    fn test_overlay_text_help_contains_wasd() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes(), None).unwrap();
        assert!(
            text.contains("W/A/S/D"),
            "help overlay should list W/A/S/D keys"
        );
    }

    #[test]
    fn test_overlay_text_help_contains_hotkeys() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes(), None).unwrap();
        assert!(
            text.contains("Ctrl+Q"),
            "help overlay should mention Ctrl+Q"
        );
        assert!(
            text.contains("Ctrl+R"),
            "help overlay should mention Ctrl+R"
        );
    }

    // ── overlay_text: cartridge-switch dialog ─────────────────────────────────

    #[test]
    fn test_overlay_text_returns_cart_switch_when_open() {
        let mut state = NativeAppState::default();
        state.cart_switch.open = true;
        let text = state.overlay_text(&make_nes(), None);
        assert!(
            text.is_some(),
            "overlay_text should be Some when cart-switch is open"
        );
        assert!(
            text.unwrap().contains("Escape"),
            "cart-switch overlay should mention Escape to cancel"
        );
    }

    #[test]
    fn test_overlay_text_cart_switch_takes_priority_over_help() {
        let mut state = NativeAppState::default();
        state.cart_switch.open = true;
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes(), None).unwrap();
        // Cart-switch takes priority; help text should NOT appear
        assert!(
            !text.contains("W/A/S/D"),
            "cart-switch overlay should not show help text"
        );
    }

    // ── overlay_text: autorun ─────────────────────────────────────────────────

    fn make_recording_autorun_state() -> AutorunState {
        let dir = tempfile::tempdir().expect("create temp dir");
        let rom_path = dir.path().join("test.nes");
        std::fs::write(&rom_path, b"dummy").expect("write dummy rom");
        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path.to_str().unwrap(),
            true,
            false,
            None,
            crate::autorun::AutorunFormat::Json,
        )
        .expect("create recording autorun state");
        state
    }

    #[test]
    fn test_overlay_text_shows_autorun_when_active() {
        let state = NativeAppState::default();
        let autorun = make_recording_autorun_state();
        let text = state.overlay_text(&make_nes(), Some(&autorun));
        assert!(text.is_some(), "overlay_text should be Some for autorun");
        assert!(
            text.unwrap().contains("Recording"),
            "autorun overlay should show 'Recording'"
        );
    }

    #[test]
    fn test_overlay_text_autorun_takes_priority_over_help() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let autorun = make_recording_autorun_state();
        let text = state.overlay_text(&make_nes(), Some(&autorun)).unwrap();
        assert!(
            text.contains("Recording"),
            "autorun overlay should take priority over help"
        );
        assert!(
            !text.contains("W/A/S/D"),
            "help overlay should not appear when autorun is active"
        );
    }

    #[test]
    fn test_overlay_text_cart_switch_takes_priority_over_autorun() {
        let mut state = NativeAppState::default();
        state.cart_switch.open = true;
        let autorun = make_recording_autorun_state();
        let text = state.overlay_text(&make_nes(), Some(&autorun)).unwrap();
        assert!(
            text.contains("Cartridge Switch"),
            "cart-switch should take priority over autorun"
        );
    }

    // ── format helpers ───────────────────────────────────────────────────────

    #[test]
    fn test_format_mm_ss() {
        assert_eq!(format_mm_ss(0), "00:00");
        assert_eq!(format_mm_ss(59), "00:59");
        assert_eq!(format_mm_ss(60), "01:00");
        assert_eq!(format_mm_ss(125), "02:05");
        assert_eq!(format_mm_ss(3661), "61:01");
    }
}
