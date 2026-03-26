//! TUI frontend for Neser — launched via `--tui` flag.
//!
//! This module provides an interactive terminal UI for browsing ROMs and
//! launching the emulator. It requires the `tui` Cargo feature.

mod app;
mod terminal;

use app::App;
use terminal::TerminalHandle;

/// Launch the TUI ROM browser.
///
/// Sets up the terminal, runs the interactive UI, then restores the terminal.
///
/// # Errors
///
/// Returns an error if terminal setup or the event loop fails.
pub fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = TerminalHandle::new()?;
    let mut app = App;
    app.run(&mut terminal)?;
    Ok(())
}
