//! TUI application state and main event loop.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::{io, time::Duration};

use super::terminal::TerminalHandle;

/// How long to wait for an event before re-drawing (~60 fps).
const FRAME_DURATION: Duration = Duration::from_millis(16);

/// Main TUI application state.
pub(crate) struct App;

impl App {
    /// Run the application event loop until the user quits.
    ///
    /// # Errors
    ///
    /// Returns an error if drawing to the terminal or reading events fails.
    pub(crate) fn run(&mut self, terminal: &mut TerminalHandle) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;
            if self.handle_events()? {
                break;
            }
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let title_block = Block::default()
            .title(" Neser TUI ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let content = Paragraph::new("Welcome to Neser TUI\n\nPress q or Esc to quit.")
            .block(title_block)
            .alignment(Alignment::Center);

        frame.render_widget(content, chunks[0]);

        let footer = Paragraph::new(" q/Esc: quit").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[1]);
    }

    /// Poll for terminal events and return `true` if the user requested quit.
    ///
    /// # Errors
    ///
    /// Returns an error if polling or reading a terminal event fails.
    fn handle_events(&self) -> io::Result<bool> {
        if event::poll(FRAME_DURATION)?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(true);
                }
                _ => {}
            }
        }
        Ok(false)
    }
}

impl Default for App {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new_creates_successfully() {
        // Assert — just verifying it constructs without panic
        let _app = App;
    }
}
