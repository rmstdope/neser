//! Runtime state for the native frontend.
//!
//! [`NativeAppState`] centralises all mutable UI/emulator state so that the
//! keyboard handler, event loop, and rendering code share a single source of
//! truth without borrowing individual fields piecemeal.

use crate::platform::autorun::AutorunMode;
use crate::platform::autorun::state::AutorunState;
use crate::platform::emulator::Console;
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

    /// Index of the currently highlighted entry within the visible
    /// (filtered) list.
    pub selection: usize,

    /// Indices into `entries` that pass the current filter.
    /// Empty when no filter is active (all entries are visible).
    filtered_indices: Vec<usize>,
}

impl CartridgeSwitchState {
    /// Closes the dialog and resets ephemeral filter/selection state.
    pub fn close(&mut self) {
        self.open = false;
        self.filter.clear();
        self.selection = 0;
        self.filtered_indices.clear();
    }

    /// Returns true when a non-empty filter is active and
    /// `filtered_indices` is populated.
    pub fn has_active_filter(&self) -> bool {
        !self.filter.is_empty()
    }

    /// Recomputes `filtered_indices` from the current filter text.
    /// Clamps `selection` so it stays within the visible range.
    pub fn refresh_filtered(&mut self) {
        if self.filter.is_empty() {
            self.filtered_indices.clear();
            return;
        }

        let needle = self.filter.to_ascii_lowercase();
        self.filtered_indices = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, path)| {
                if entry_matches_filter(path, &needle) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        let count = self.visible_count();
        if count == 0 {
            self.selection = 0;
        } else {
            self.selection = self.selection.min(count - 1);
        }
    }

    /// Number of entries currently visible (after filtering).
    pub fn visible_count(&self) -> usize {
        if self.filter.is_empty() {
            self.entries.len()
        } else {
            self.filtered_indices.len()
        }
    }

    /// Moves the selection cursor down, wrapping around.
    pub fn move_selection_next(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selection = 0;
            return;
        }
        self.selection = (self.selection + 1) % count;
    }

    /// Moves the selection cursor up, wrapping around.
    pub fn move_selection_prev(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selection = 0;
            return;
        }
        self.selection = if self.selection == 0 {
            count - 1
        } else {
            self.selection - 1
        };
    }

    /// Returns the full path of the currently selected entry, if any.
    pub fn selected_entry(&self) -> Option<&str> {
        self.visible_entry_at(self.selection)
    }

    /// Returns the entry at the given visible (post-filter) index.
    fn visible_entry_at(&self, visible_idx: usize) -> Option<&str> {
        let entry_idx = if self.filter.is_empty() {
            Some(visible_idx)
        } else {
            self.filtered_indices.get(visible_idx).copied()
        };
        entry_idx
            .and_then(|i| self.entries.get(i))
            .map(String::as_str)
    }
}

/// Returns true if the needle characters appear in order (fuzzy) in the
/// haystack filename, or if the needle is a substring of the directory.
fn entry_matches_filter(path: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let filename_haystack = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();
    if fuzzy_matches(needle, &filename_haystack) {
        return true;
    }
    let dir_haystack = std::path::Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    dir_haystack.contains(needle)
}

/// Fuzzy subsequence match: every character in `needle` must appear in
/// `haystack` in order, but not necessarily consecutively.
fn fuzzy_matches(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next().expect("needle is non-empty");
    for ch in haystack.chars() {
        if ch == current {
            match needle_chars.next() {
                Some(next) => current = next,
                None => return true,
            }
        }
    }
    false
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

    /// Whether the FPS counter is visible.
    pub show_fps: bool,

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

    /// Number of gamepads currently connected.
    /// Controls keyboard input routing: 0 → keyboard covers ports 1+2;
    /// 1 → keyboard covers port 2 only; 2+ → keyboard disabled for controllers.
    pub gamepad_count: usize,

    /// Whether Four Score mode is enabled, allowing up to 4 players.
    /// When true, keyboard input is routed to ports beyond the first two
    /// when gamepads occupy the lower ports.
    pub four_score_enabled: bool,
}

impl NativeAppState {
    /// Returns the overlay text to display on top of the rendered frame, if any.
    ///
    /// Priority (highest first):
    /// 1. Cartridge-switch dialog (always shown when open, even with no entries).
    /// 2. Autorun status (playback/recording progress).
    /// 3. Help overlay.
    pub fn overlay_text(
        &self,
        console: &Console,
        autorun_state: Option<&AutorunState>,
    ) -> Option<String> {
        if self.cart_switch.open {
            return Some(cart_switch_overlay_text(&self.cart_switch));
        }
        if let Some(autorun) = autorun_state {
            let frame_duration = console.as_core().target_frame_duration();
            let fps = (1.0 / frame_duration.as_secs_f64()).round().max(1.0) as usize;
            return Some(autorun_overlay_text(autorun, fps));
        }
        if self.help_overlay_visible {
            let (port1_is_power_pad, port2_is_power_pad) = match console {
                Console::Nes(nes) => {
                    use crate::nes::input::ControllerType;
                    (
                        nes.active_controller_port_type(1) == ControllerType::PowerPad,
                        nes.active_controller_port_type(2) == ControllerType::PowerPad,
                    )
                }
                Console::GameBoy(_) | Console::GameBoyAdvance(_) | Console::Snes(_) => {
                    (false, false)
                }
            };
            return Some(help_overlay_text(
                console,
                self.gamepad_count,
                self.four_score_enabled,
                port1_is_power_pad,
                port2_is_power_pad,
            ));
        }
        None
    }

    /// Returns `true` when keyboard text characters should be forwarded to the
    /// active UI layer rather than being consumed only by the game controller
    /// handler.
    pub fn keyboard_captured_by_ui(&self) -> bool {
        self.debugger_open
    }
}

fn help_overlay_text(
    console: &Console,
    gamepad_count: usize,
    four_score: bool,
    port1_is_power_pad: bool,
    port2_is_power_pad: bool,
) -> String {
    let hotkeys = "Controls\n\
H: Toggle help\n\
Ctrl+Q: Quit\n\
Space: Pause\n\
(Shift+)Ctrl+R: Reset emulator (use Shift for hard reset)\n\
Ctrl+F: Toggle fullscreen\n\
Ctrl+O: Switch cartridge\n\
F2/F3: Volume up/down\n\
F4: Cycle visual filter\n\
F5: Debugger (open/continue)\n\
F10/F11: Step over/into\n\
F6/F7: Save/Load state";

    let max_ports: usize = if four_score { 4 } else { 2 };
    let keyboard_ports =
        crate::frontends::native::keyboard::keyboard_target_ports(gamepad_count, four_score);

    let is_power_pad_for = |port: usize| match port {
        1 => port1_is_power_pad,
        2 => port2_is_power_pad,
        _ => false,
    };

    let mut controllers = String::new();

    if matches!(console, Console::GameBoyAdvance(_)) {
        return format!("{hotkeys}{}", gba_keyboard_section(gamepad_count));
    }

    if matches!(console, Console::GameBoy(_)) {
        return format!("{hotkeys}{}", gameboy_keyboard_section(gamepad_count));
    }

    if matches!(console, Console::Snes(_)) {
        return format!("{hotkeys}{}", snes_keyboard_section(gamepad_count));
    }

    for port in 1..=max_ports {
        let port_u8 = port as u8;
        if port <= gamepad_count {
            controllers.push_str(&format!("\n\nPort {port}: Gamepad"));
        } else if let Some(slot) = keyboard_ports.iter().position(|&p| p == port_u8) {
            if is_power_pad_for(port) {
                controllers.push_str(&power_pad_keyboard_section(port, slot));
            } else {
                controllers.push_str(&joypad_keyboard_section(port, slot));
            }
        } else {
            controllers.push_str(&format!("\n\nPort {port}: Empty"));
        }
    }

    format!("{hotkeys}{controllers}")
}

/// Power Pad key bindings for a given keyboard slot (0 = primary keys, 1 = secondary keys).
fn power_pad_keyboard_section(port: usize, slot: usize) -> String {
    if slot == 0 {
        format!(
            "\n\nPower Pad (Port {port} via keyboard)\n\
1/2/3: Buttons 1-3\n\
Q/W/E: Buttons 4-6\n\
A/S/D: Buttons 7-9\n\
Z/X/C: Buttons 10-12"
        )
    } else {
        format!(
            "\n\nPower Pad (Port {port} via keyboard)\n\
7/8/9: Buttons 1-3\n\
U/I/O: Buttons 4-6\n\
J/K/L: Buttons 7-9\n\
M/,/.: Buttons 10-12"
        )
    }
}

/// Joypad key bindings for a given keyboard slot (0 = primary WASD, 1 = secondary IJKL).
fn joypad_keyboard_section(port: usize, slot: usize) -> String {
    if slot == 0 {
        format!(
            "\n\nPort {port}: Keyboard\n\
W/A/S/D: D-Pad\n\
R: B\n\
T: A\n\
4: Select\n\
5: Start"
        )
    } else {
        format!(
            "\n\nPort {port}: Keyboard\n\
I/J/K/L: D-Pad\n\
O: A\n\
P: B\n\
9: Select\n\
0: Start"
        )
    }
}

fn gameboy_keyboard_section(gamepad_count: usize) -> String {
    let controller = if gamepad_count == 0 {
        "Keyboard".to_string()
    } else {
        "Gamepad + Keyboard aliases".to_string()
    };
    format!(
        "\n\nGame Boy: {controller}\n\
W/A/S/D: D-Pad\n\
Arrow keys: D-Pad\n\
R: B\n\
T: A\n\
4: Select\n\
5: Start"
    )
}

fn gba_keyboard_section(gamepad_count: usize) -> String {
    let controller = if gamepad_count == 0 {
        "Keyboard".to_string()
    } else {
        "Gamepad + Keyboard shoulders/aliases".to_string()
    };
    format!(
        "\n\nGBA: {controller}\n\
W/A/S/D: D-Pad\n\
Arrow keys: D-Pad\n\
Q: L\n\
E: R\n\
R: B\n\
T: A\n\
4: Select\n\
5: Start"
    )
}

fn snes_keyboard_section(gamepad_count: usize) -> String {
    let controller = if gamepad_count == 0 {
        "Keyboard".to_string()
    } else {
        "Gamepad + Keyboard shoulders/aliases".to_string()
    };
    format!(
        "\n\nSNES: {controller}\n\
W/A/S/D: D-Pad\n\
Arrow keys: D-Pad\n\
Q: L\n\
E: R\n\
R: B\n\
T: A\n\
Y: Y\n\
U: X\n\
4: Select\n\
5: Start"
    )
}

fn cart_switch_overlay_text(cart_switch: &CartridgeSwitchState) -> String {
    if cart_switch.entries.is_empty() {
        return "Switch cartridge\nNo catalog entries found\n\nPress Esc to close".to_string();
    }

    let filter_label = if cart_switch.filter.is_empty() {
        "(none)"
    } else {
        cart_switch.filter.as_str()
    };
    let visible_count = cart_switch.visible_count();
    let total_count = cart_switch.entries.len();
    let matches_label = format!("Matches: {visible_count}/{total_count}");

    if visible_count == 0 {
        return format!(
            "Switch cartridge\nFilter: {filter_label}\n{matches_label}\n\
             No matching entries\n\nBackspace: Edit filter    Esc: Cancel"
        );
    }

    const MAX_VISIBLE: usize = 12;
    let selected = cart_switch.selection.min(visible_count.saturating_sub(1));
    let mut start = selected.saturating_sub(MAX_VISIBLE / 2);
    let mut end = (start + MAX_VISIBLE).min(visible_count);
    if end - start < MAX_VISIBLE {
        start = end.saturating_sub(MAX_VISIBLE);
    }
    end = (start + MAX_VISIBLE).min(visible_count);

    let mut lines = vec![
        "Switch cartridge".to_string(),
        format!("Filter: {filter_label}"),
        matches_label,
        "Up/Down: Select".to_string(),
        "Type to filter    Backspace: Delete".to_string(),
        "Enter: Load    Esc: Cancel".to_string(),
        String::new(),
    ];

    for visible_idx in start..end {
        let Some(entry_text) = cart_switch.visible_entry_at(visible_idx) else {
            continue;
        };

        let line = if visible_idx == selected {
            format!(">> {entry_text} <<")
        } else {
            format!("   {entry_text}")
        };
        lines.push(line);
    }

    lines.join("\n")
}

/// Generate autorun overlay text showing playback/recording progress.
fn autorun_overlay_text(autorun_state: &AutorunState, fps: usize) -> String {
    match autorun_state.mode() {
        // Both pure playback and extend-playback (record mode replaying existing frames)
        // display the same progress indicator.
        AutorunMode::Playback => {
            let current = autorun_state.current_frame_index();
            let total = autorun_state.total_frames();
            let (elapsed, total_str) = format_time_pair(current, total, fps);
            format!("Playback\n{elapsed} / {total_str}")
        }
        AutorunMode::Record if autorun_state.is_extending_playback() => {
            let current = autorun_state.current_frame_index();
            let total = autorun_state.total_frames();
            let (elapsed, total_str) = format_time_pair(current, total, fps);
            format!("Playback\n{elapsed} / {total_str}")
        }
        AutorunMode::Record => {
            let current = autorun_state.total_frames();
            let (elapsed, _) = format_time_pair(current, current, fps);
            format!("Recording\n{elapsed} / {elapsed}")
        }
        AutorunMode::None => String::new(),
    }
}

fn format_time_pair(current_frames: usize, total_frames: usize, fps: usize) -> (String, String) {
    let fps = fps.max(1);
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
    use crate::nes::console::{Config, Nes, NesConfig};
    use crate::platform::app_context::AppContext;

    fn make_console() -> Console {
        Console::Nes(Box::new(Nes::new(AppContext::new_with_config(
            Config::default(),
        ))))
    }

    fn make_gba_console() -> Console {
        Console::new_gba(AppContext::new_with_config(Config::default()))
    }

    fn make_gameboy_console() -> Console {
        Console::new_gameboy(AppContext::new_with_config(Config::default()))
    }

    // ── overlay_text: help overlay ────────────────────────────────────────────

    #[test]
    fn test_overlay_text_returns_none_when_nothing_visible() {
        let state = NativeAppState::default();
        assert!(state.overlay_text(&make_console(), None).is_none());
    }

    #[test]
    fn test_overlay_text_returns_controls_when_help_visible() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None);
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
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("W/A/S/D"),
            "help overlay should list W/A/S/D keys"
        );
    }

    #[test]
    fn test_overlay_text_help_contains_hotkeys() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("Ctrl+Q"),
            "help overlay should mention Ctrl+Q"
        );
        assert!(
            text.contains("Ctrl+R"),
            "help overlay should mention Ctrl+R"
        );
    }

    #[test]
    fn test_help_overlay_wasd_labeled_as_port1_only_when_no_gamepad() {
        // Given: no gamepads connected
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 0,
            ..NativeAppState::default()
        };
        // When: help overlay is generated
        let text = state.overlay_text(&make_console(), None).unwrap();
        // Then: WASD is labelled as Port 1 only — not "Port 1 + Port 2"
        assert!(
            !text.contains("Port 1 + Port 2"),
            "WASD should not be labelled 'Port 1 + Port 2'; each keyboard set controls one port, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_shows_gamepad_for_port1_when_one_gamepad_connected() {
        // Given: one gamepad connected
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            ..NativeAppState::default()
        };
        // When: help overlay is generated
        let text = state.overlay_text(&make_console(), None).unwrap();
        // Then: port 1 is labelled as gamepad-controlled
        let lower = text.to_lowercase();
        assert!(
            lower.contains("gamepad"),
            "help overlay should mention 'gamepad' when one is connected, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_keyboard_disabled_note_when_two_gamepads() {
        // Given: two gamepads connected
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 2,
            ..NativeAppState::default()
        };
        // When: help overlay is generated
        let text = state.overlay_text(&make_console(), None).unwrap();
        // Then: the overlay does NOT list keyboard controller keys
        // (both ports are owned by gamepads)
        assert!(
            !text.contains("W/A/S/D"),
            "help overlay should NOT show WASD when two gamepads control both ports, got:\n{text}"
        );
    }

    // ── overlay_text: cartridge-switch dialog ─────────────────────────────────

    #[test]
    fn test_overlay_text_returns_cart_switch_when_open() {
        let mut state = NativeAppState::default();
        state.cart_switch.open = true;
        let text = state.overlay_text(&make_console(), None);
        assert!(
            text.is_some(),
            "overlay_text should be Some when cart-switch is open"
        );
        assert!(
            text.unwrap().contains("Esc"),
            "cart-switch overlay should mention Esc to close"
        );
    }

    #[test]
    fn test_overlay_text_cart_switch_takes_priority_over_help() {
        let mut state = NativeAppState::default();
        state.cart_switch.open = true;
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_console(), None).unwrap();
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
            crate::platform::autorun::AutorunFormat::Json,
        )
        .expect("create recording autorun state");
        state
    }

    #[test]
    fn test_overlay_text_shows_autorun_when_active() {
        let state = NativeAppState::default();
        let autorun = make_recording_autorun_state();
        let text = state.overlay_text(&make_console(), Some(&autorun));
        assert!(text.is_some(), "overlay_text should be Some for autorun");
        assert!(
            text.unwrap().contains("Recording"),
            "autorun overlay should show 'Recording'"
        );
    }

    #[test]
    fn test_overlay_text_autorun_takes_priority_over_help() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let autorun = make_recording_autorun_state();
        let text = state.overlay_text(&make_console(), Some(&autorun)).unwrap();
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
        let text = state.overlay_text(&make_console(), Some(&autorun)).unwrap();
        assert!(
            text.contains("Switch cartridge"),
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

    // ── fuzzy matching ───────────────────────────────────────────────────────

    #[test]
    fn test_fuzzy_matches_empty_needle() {
        assert!(fuzzy_matches("", "anything"));
    }

    #[test]
    fn test_fuzzy_matches_consecutive_chars() {
        assert!(fuzzy_matches("abc", "abc"));
    }

    #[test]
    fn test_fuzzy_matches_non_consecutive_chars() {
        assert!(fuzzy_matches("ac", "abc"));
    }

    #[test]
    fn test_fuzzy_matches_spread_chars() {
        assert!(fuzzy_matches("smb", "super mario bros"));
    }

    #[test]
    fn test_fuzzy_matches_no_match() {
        assert!(!fuzzy_matches("ba", "abc"));
    }

    #[test]
    fn test_fuzzy_matches_partial_match_at_end() {
        assert!(!fuzzy_matches("abcd", "abc"));
    }

    #[test]
    fn test_entry_matches_filter_empty_needle() {
        assert!(entry_matches_filter("/roms/test.nes", ""));
    }

    #[test]
    fn test_entry_matches_filter_by_filename() {
        assert!(entry_matches_filter("/roms/super_mario.nes", "smr"));
    }

    #[test]
    fn test_entry_matches_filter_by_directory_substring() {
        assert!(entry_matches_filter("/home/user/roms/game.nes", "roms"));
    }

    #[test]
    fn test_entry_matches_filter_case_insensitive() {
        assert!(entry_matches_filter("/roms/Zelda.nes", "zelda"));
    }

    // ── CartridgeSwitchState ─────────────────────────────────────────────────

    fn make_cart_switch(entries: &[&str]) -> CartridgeSwitchState {
        CartridgeSwitchState {
            open: true,
            entries: entries.iter().map(|s| s.to_string()).collect(),
            filter: String::new(),
            selection: 0,
            filtered_indices: Vec::new(),
        }
    }

    #[test]
    fn test_visible_count_no_filter() {
        let cs = make_cart_switch(&["a.nes", "b.nes", "c.nes"]);
        assert_eq!(cs.visible_count(), 3);
    }

    #[test]
    fn test_visible_count_with_filter() {
        let mut cs = make_cart_switch(&["alpha.nes", "beta.nes", "gamma.nes"]);
        cs.filter = "lph".to_string();
        cs.refresh_filtered();
        assert_eq!(cs.visible_count(), 1, "only alpha matches 'lph'");
    }

    #[test]
    fn test_refresh_filtered_clamps_selection() {
        let mut cs = make_cart_switch(&["alpha.nes", "beta.nes", "gamma.nes"]);
        cs.selection = 2;
        cs.filter = "beta".to_string();
        cs.refresh_filtered();
        assert_eq!(cs.selection, 0, "selection clamped to single match");
    }

    #[test]
    fn test_move_selection_next_wraps() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes", "c.nes"]);
        cs.selection = 2;
        cs.move_selection_next();
        assert_eq!(cs.selection, 0, "should wrap to first");
    }

    #[test]
    fn test_move_selection_prev_wraps() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes", "c.nes"]);
        cs.selection = 0;
        cs.move_selection_prev();
        assert_eq!(cs.selection, 2, "should wrap to last");
    }

    #[test]
    fn test_move_selection_next_empty() {
        let mut cs = make_cart_switch(&[]);
        cs.move_selection_next();
        assert_eq!(cs.selection, 0);
    }

    #[test]
    fn test_selected_entry_no_filter() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes", "c.nes"]);
        cs.selection = 1;
        assert_eq!(cs.selected_entry(), Some("b.nes"));
    }

    #[test]
    fn test_selected_entry_with_filter() {
        let mut cs = make_cart_switch(&["alpha.nes", "beta.nes", "gamma.nes"]);
        cs.filter = "mm".to_string();
        cs.refresh_filtered();
        // Only gamma matches 'mm' (g-a-m-m-a)
        assert_eq!(cs.visible_count(), 1);
        cs.selection = 0;
        assert_eq!(cs.selected_entry(), Some("gamma.nes"));
    }

    #[test]
    fn test_selected_entry_empty() {
        let cs = make_cart_switch(&[]);
        assert_eq!(cs.selected_entry(), None);
    }

    #[test]
    fn test_close_resets_filter_and_selection() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes"]);
        cs.filter = "test".to_string();
        cs.selection = 1;
        cs.refresh_filtered();
        cs.close();
        assert!(!cs.open);
        assert!(cs.filter.is_empty());
        assert_eq!(cs.selection, 0);
    }

    #[test]
    fn test_refresh_filtered_empty_filter_does_not_populate_indices() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes", "c.nes"]);
        cs.filter.clear();
        cs.refresh_filtered();
        assert!(
            !cs.has_active_filter(),
            "empty filter should not populate filtered_indices"
        );
        assert_eq!(cs.visible_count(), 3, "all entries visible without filter");
    }

    // ── overlay text: cartridge switch ────────────────────────────────────────

    #[test]
    fn test_cart_switch_overlay_no_catalog() {
        let cs = CartridgeSwitchState {
            open: true,
            ..Default::default()
        };
        let text = cart_switch_overlay_text(&cs);
        assert!(
            text.contains("No catalog"),
            "should show no-catalog message"
        );
    }

    #[test]
    fn test_cart_switch_overlay_shows_filter() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes"]);
        cs.filter = "test".to_string();
        cs.refresh_filtered();
        let text = cart_switch_overlay_text(&cs);
        assert!(text.contains("Filter: test"), "should show filter text");
    }

    #[test]
    fn test_cart_switch_overlay_shows_match_count() {
        let mut cs = make_cart_switch(&["alpha.nes", "beta.nes", "gamma.nes"]);
        cs.filter = "lph".to_string();
        cs.refresh_filtered();
        let text = cart_switch_overlay_text(&cs);
        assert!(
            text.contains("Matches: 1/3"),
            "should show 1/3 matches, got: {text}"
        );
    }

    #[test]
    fn test_cart_switch_overlay_no_filter_shows_all() {
        let cs = make_cart_switch(&["a.nes", "b.nes"]);
        let text = cart_switch_overlay_text(&cs);
        assert!(text.contains("Matches: 2/2"), "got: {text}");
    }

    #[test]
    fn test_cart_switch_overlay_selected_entry_marked() {
        let cs = make_cart_switch(&["a.nes", "b.nes"]);
        let text = cart_switch_overlay_text(&cs);
        assert!(
            text.contains(">> a.nes <<"),
            "first entry should be selected, got: {text}"
        );
        assert!(
            text.contains("   b.nes"),
            "second entry should not be selected, got: {text}"
        );
    }

    #[test]
    fn test_cart_switch_overlay_no_matches() {
        let mut cs = make_cart_switch(&["a.nes", "b.nes"]);
        cs.filter = "zzzzz".to_string();
        cs.refresh_filtered();
        let text = cart_switch_overlay_text(&cs);
        assert!(
            text.contains("No matching entries"),
            "should show no-match message, got: {text}"
        );
    }

    // --- keyboard_captured_by_ui (#1859) ---

    #[test]
    fn keyboard_not_captured_by_ui_when_debugger_closed() {
        // Given the debugger is closed.
        let state = NativeAppState::default();

        // When checking whether the UI should receive keyboard text.
        let captured = state.keyboard_captured_by_ui();

        // Then gameplay input keeps exclusive keyboard ownership.
        assert!(
            !captured,
            "UI must not capture keyboard during normal gameplay"
        );
    }

    #[test]
    fn keyboard_captured_by_ui_when_debugger_open() {
        // Given the debugger is open.
        let state = NativeAppState {
            debugger_open: true,
            ..NativeAppState::default()
        };

        // When checking whether the UI should receive keyboard text.
        let captured = state.keyboard_captured_by_ui();

        // Then debugger text fields can receive keyboard input.
        assert!(captured, "UI must capture keyboard when debugger is open");
    }

    // ── Four Score: help overlay ──────────────────────────────────────────────

    #[test]
    fn test_help_overlay_four_score_2_gamepads_shows_ports_3_4_keyboard() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("Port 3"),
            "four-score with 2 gamepads should mention Port 3, got:\n{text}"
        );
        assert!(
            text.contains("Port 4"),
            "four-score with 2 gamepads should mention Port 4, got:\n{text}"
        );
        assert!(
            text.contains("W/A/S/D"),
            "four-score with 2 gamepads should show keyboard keys for port 3, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_four_score_1_gamepad_shows_ports_2_3_keyboard() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("Port 1: Gamepad"),
            "port 1 should be gamepad, got:\n{text}"
        );
        assert!(
            text.contains("Port 2"),
            "should mention Port 2 for keyboard, got:\n{text}"
        );
        assert!(
            text.contains("Port 3"),
            "should mention Port 3 for keyboard, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_four_score_3_gamepads_shows_port_4_keyboard() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 3,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("Port 4"),
            "four-score with 3 gamepads should mention Port 4, got:\n{text}"
        );
        assert!(
            text.contains("W/A/S/D"),
            "should show keyboard keys for port 4, got:\n{text}"
        );
        assert!(
            !text.contains("I/J/K/L"),
            "should NOT show P2 keyboard keys (only one keyboard slot), got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_four_score_4_gamepads_no_keyboard() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 4,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            text.contains("Port 4: Gamepad"),
            "all 4 ports should be gamepads, got:\n{text}"
        );
        assert!(
            !text.contains("W/A/S/D"),
            "no keyboard keys when all 4 ports have gamepads, got:\n{text}"
        );
    }

    // --- Power Pad help overlay tests ---

    #[test]
    fn test_help_overlay_shows_power_pad_section_when_port2_is_power_pad() {
        let config = Config {
            nes: NesConfig {
                controller_port2: crate::nes::input::ControllerType::PowerPad,
                ..Default::default()
            },
            ..Default::default()
        };
        let console = Console::Nes(Box::new(Nes::new(AppContext::new_with_config(config))));
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&console, None).unwrap();
        assert!(
            text.contains("Power Pad"),
            "Help overlay should show Power Pad section when port 2 is Power Pad, got:\n{text}"
        );
        assert!(
            text.contains("7/8/9"),
            "Help overlay should show P2 Power Pad keys, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_shows_power_pad_section_when_port1_is_power_pad() {
        let config = Config {
            nes: NesConfig {
                controller_port1: crate::nes::input::ControllerType::PowerPad,
                ..Default::default()
            },
            ..Default::default()
        };
        let console = Console::Nes(Box::new(Nes::new(AppContext::new_with_config(config))));
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&console, None).unwrap();
        assert!(
            text.contains("Power Pad"),
            "Help overlay should show Power Pad section when port 1 is Power Pad, got:\n{text}"
        );
        assert!(
            text.contains("1/2/3"),
            "Help overlay should show P1 Power Pad keys, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_no_power_pad_section_for_default_joypads() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };
        let text = state.overlay_text(&make_console(), None).unwrap();
        assert!(
            !text.contains("Power Pad"),
            "Help overlay should not show Power Pad section for default joypads, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_lists_gba_keyboard_shoulders() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };

        let text = state.overlay_text(&make_gba_console(), None).unwrap();

        assert!(
            text.contains("Q: L"),
            "GBA help overlay should list Q as L shoulder, got:\n{text}"
        );
        assert!(
            text.contains("E: R"),
            "GBA help overlay should list E as R shoulder, got:\n{text}"
        );
        assert!(
            text.contains("R: B"),
            "GBA help overlay should list R as B, got:\n{text}"
        );
        assert!(
            text.contains("T: A"),
            "GBA help overlay should list T as A, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_lists_gba_gamepad_and_keyboard_when_gamepad_connected() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            ..NativeAppState::default()
        };

        let text = state.overlay_text(&make_gba_console(), None).unwrap();

        assert!(
            text.contains("Gamepad + Keyboard shoulders/aliases"),
            "GBA help overlay should mention connected gamepad plus keyboard-only controls, got:\n{text}"
        );
        assert!(
            text.contains("Q: L") && text.contains("E: R"),
            "GBA help overlay should still list keyboard shoulders with a gamepad connected, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_lists_gameboy_arrow_key_aliases() {
        let state = NativeAppState {
            help_overlay_visible: true,
            ..NativeAppState::default()
        };

        let text = state.overlay_text(&make_gameboy_console(), None).unwrap();

        assert!(
            text.contains("Game Boy: Keyboard"),
            "Game Boy help overlay should use a Game Boy-specific section, got:\n{text}"
        );
        assert!(
            text.contains("Arrow keys: D-Pad"),
            "Game Boy help overlay should list arrow-key D-pad aliases, got:\n{text}"
        );
        assert!(
            !text.contains("Q: L") && !text.contains("E: R"),
            "Game Boy help overlay should not list GBA shoulders, got:\n{text}"
        );
    }

    #[test]
    fn test_help_overlay_lists_gameboy_gamepad_and_keyboard_when_gamepad_connected() {
        let state = NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            ..NativeAppState::default()
        };

        let text = state.overlay_text(&make_gameboy_console(), None).unwrap();

        assert!(
            text.contains("Game Boy: Gamepad + Keyboard aliases"),
            "Game Boy help overlay should mention connected gamepad plus keyboard aliases, got:\n{text}"
        );
        assert!(
            text.contains("Arrow keys: D-Pad"),
            "Game Boy help overlay should still list arrow-key D-pad aliases with a gamepad connected, got:\n{text}"
        );
        assert!(
            !text.contains("Keyboard shoulders/aliases"),
            "Game Boy help overlay should not use GBA shoulder wording, got:\n{text}"
        );
    }
}
