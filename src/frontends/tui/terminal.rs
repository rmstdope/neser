//! Terminal setup and teardown using crossterm + ratatui.

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout};

/// RAII guard that sets up the terminal on creation and restores it on drop.
pub struct TerminalHandle {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalHandle {
    /// Set up the terminal: enable raw mode, enter alternate screen.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal cannot be configured.
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Draw a frame to the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying terminal draw operation fails.
    pub fn draw<F>(&mut self, f: F) -> io::Result<ratatui::CompletedFrame<'_>>
    where
        F: FnOnce(&mut ratatui::Frame),
    {
        self.terminal.draw(f)
    }

    /// Re-enter raw mode and alternate screen after the emulator child process exits.
    ///
    /// Called by `App` to restore the TUI after handing control to the SDL emulator.
    ///
    /// # Errors
    ///
    /// Returns an error if re-entering the alternate screen fails.
    pub fn restore_alternate_screen(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        self.terminal.clear()?;
        Ok(())
    }
}

impl Drop for TerminalHandle {
    fn drop(&mut self) {
        // Best-effort cleanup — ignore errors during teardown.
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
