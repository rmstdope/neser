//! TUI frontend for Neser — launched via `--tui` flag.
//!
//! This module provides an interactive terminal UI for browsing ROMs and
//! launching the emulator. It requires the `tui` Cargo feature.

mod action_menu;
mod app;
mod catalog;
mod launcher;
mod rom_entry;
mod rom_list;
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
pub fn run_tui(
    search_paths: &[String],
    rebuild_catalog: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = catalog::load_catalog(search_paths, rebuild_catalog).unwrap_or_else(|_| vec![]);
    let mut terminal = TerminalHandle::new()?;
    let mut app = App::new(entries);
    app.run(&mut terminal)?;
    Ok(())
}
