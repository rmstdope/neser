//! Runtime state for the native frontend.
//!
//! [`NativeAppState`] centralises all mutable UI/emulator state so that the
//! keyboard handler, event loop, and rendering code share a single source of
//! truth without borrowing individual fields piecemeal.

use crate::console::Nes;
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

    /// Last known Zapper position for crosshair rendering.
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
    /// 2. Help overlay.
    pub fn overlay_text(&self, _nes: &Nes) -> Option<String> {
        if self.cart_switch.open {
            return Some(cart_switch_overlay_text(&self.cart_switch));
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
    let visible: Vec<&str> = cart_switch
        .entries
        .iter()
        .map(String::as_str)
        .filter(|e| {
            cart_switch.filter.is_empty()
                || e.to_lowercase()
                    .contains(&cart_switch.filter.to_lowercase())
        })
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
        assert!(state.overlay_text(&make_nes()).is_none());
    }

    #[test]
    fn test_overlay_text_returns_controls_when_help_visible() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes());
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
        let text = state.overlay_text(&make_nes()).unwrap();
        assert!(
            text.contains("W/A/S/D"),
            "help overlay should list W/A/S/D keys"
        );
    }

    #[test]
    fn test_overlay_text_help_contains_hotkeys() {
        let mut state = NativeAppState::default();
        state.help_overlay_visible = true;
        let text = state.overlay_text(&make_nes()).unwrap();
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
        let text = state.overlay_text(&make_nes());
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
        let text = state.overlay_text(&make_nes()).unwrap();
        // Cart-switch takes priority; help text should NOT appear
        assert!(
            !text.contains("W/A/S/D"),
            "cart-switch overlay should not show help text"
        );
    }
}
