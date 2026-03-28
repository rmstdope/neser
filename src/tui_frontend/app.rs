//! TUI application state and main event loop.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};
use std::{io, time::Duration};

use super::action_menu::ActionMenu;
use super::launcher::{LaunchResult, launch_rom};
use super::rom_entry::RomEntry;
use super::rom_list::RomList;
use super::terminal::TerminalHandle;

/// How long to wait for an event before re-drawing (~60 fps).
const FRAME_DURATION: Duration = Duration::from_millis(16);

/// Tracks what is receiving keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Navigate,
    Search,
    ActionMenu,
    Help,
}

/// Main TUI application state.
pub(crate) struct App {
    rom_list: RomList,
    search: String,
    input_mode: InputMode,
    action_menu: Option<ActionMenu>,
    last_launch: Option<LaunchResult>,
    catalog_error: Option<String>,
}

impl App {
    pub fn new(entries: Vec<RomEntry>) -> Self {
        Self {
            rom_list: RomList::new(entries),
            search: String::new(),
            input_mode: InputMode::Navigate,
            action_menu: None,
            last_launch: None,
            catalog_error: None,
        }
    }

    /// Attach a catalog load error to display in the status bar.
    pub fn with_catalog_error(mut self, error: String) -> Self {
        self.catalog_error = Some(error);
        self
    }

    /// Run the application event loop until the user quits.
    ///
    /// # Errors
    ///
    /// Returns an error if drawing to the terminal or reading events fails.
    pub(crate) fn run(&mut self, terminal: &mut TerminalHandle) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;
            if self.handle_events(terminal)? {
                break;
            }
        }
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // search / title bar
                Constraint::Min(0),    // ROM list
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Search / title bar
        let search_border_style = if self.input_mode == InputMode::Search {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let search_label = if self.input_mode == InputMode::Search {
            format!("Search: {}_", self.search)
        } else {
            format!("Search: {} (/ to search)", self.search)
        };
        let search_widget = Paragraph::new(search_label).block(
            Block::default()
                .title(" Neser TUI ")
                .borders(Borders::ALL)
                .border_style(search_border_style),
        );
        frame.render_widget(search_widget, chunks[0]);

        // ROM list
        self.rom_list.render(frame, chunks[1]);

        // Footer / status
        let footer_text = if let Some(err) = &self.catalog_error {
            format!(" ⚠ Catalog error: {err}")
        } else if let Some(result) = &self.last_launch {
            result.summary()
        } else {
            " ↑/↓: navigate  Enter: select  /: search  ?: help  q/Esc: quit".to_string()
        };
        let footer_style = if self.catalog_error.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let footer = Paragraph::new(footer_text).style(footer_style);
        frame.render_widget(footer, chunks[2]);

        // Overlays (rendered last, on top of everything)
        if let Some(menu) = self.action_menu.as_mut() {
            menu.render(frame, area);
        }
        if self.input_mode == InputMode::Help {
            render_help_overlay(frame, area);
        }
    }

    /// Poll for terminal events and return `true` if the user requested quit.
    ///
    /// # Errors
    ///
    /// Returns an error if polling or reading a terminal event fails.
    fn handle_events(&mut self, terminal: &mut TerminalHandle) -> io::Result<bool> {
        if event::poll(FRAME_DURATION)?
            && let Event::Key(key) = event::read()?
        {
            return match self.input_mode {
                InputMode::Search => Ok(self.handle_search_key(key)),
                InputMode::Navigate => Ok(self.handle_navigate_key(key, terminal)),
                InputMode::ActionMenu => Ok(self.handle_action_menu_key(key, terminal)),
                InputMode::Help => Ok(self.handle_help_key(key)),
            };
        }
        Ok(false)
    }

    fn handle_search_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Navigate;
                self.search.clear();
                self.rom_list.set_filter("");
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Navigate;
            }
            KeyCode::Backspace => {
                self.search.pop();
                let s = self.search.clone();
                self.rom_list.set_filter(&s);
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                let s = self.search.clone();
                self.rom_list.set_filter(&s);
            }
            _ => {}
        }
        false
    }

    fn handle_navigate_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        _terminal: &mut TerminalHandle,
    ) -> bool {
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.rom_list.set_filter("");
                } else {
                    return true;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return true;
            }
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Search;
                self.last_launch = None;
                self.catalog_error = None;
            }
            KeyCode::Char('?') => {
                self.input_mode = InputMode::Help;
            }
            KeyCode::Down => self.rom_list.select_next(),
            KeyCode::Up => self.rom_list.select_prev(),
            KeyCode::PageDown => self.rom_list.select_page_down(10),
            KeyCode::PageUp => self.rom_list.select_page_up(10),
            KeyCode::Home => self.rom_list.select_page_up(usize::MAX),
            KeyCode::End => self.rom_list.select_page_down(usize::MAX),
            KeyCode::Enter => {
                if let Some(entry) = self.rom_list.selected_entry() {
                    let name = entry.display_name.clone();
                    let recording_duration = entry.recording_duration;
                    self.action_menu =
                        Some(ActionMenu::new_with_recording(name, recording_duration));
                    self.input_mode = InputMode::ActionMenu;
                    self.last_launch = None;
                    self.catalog_error = None;
                }
            }
            _ => {}
        }
        false
    }

    fn handle_action_menu_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        terminal: &mut TerminalHandle,
    ) -> bool {
        let Some(menu) = self.action_menu.as_mut() else {
            self.input_mode = InputMode::Navigate;
            return false;
        };
        match key.code {
            KeyCode::Esc => {
                self.action_menu = None;
                self.input_mode = InputMode::Navigate;
            }
            KeyCode::Down => menu.select_next(),
            KeyCode::Up => menu.select_prev(),
            KeyCode::Enter => {
                let action = menu.selected_action();
                self.action_menu = None;
                self.input_mode = InputMode::Navigate;

                if let Some(entry) = self.rom_list.selected_entry() {
                    let rom_path = entry.path.to_string_lossy().into_owned();
                    let entry_path = entry.path.clone();
                    let result = launch_rom(&rom_path, action);
                    let _ = terminal.restore_alternate_screen();
                    // Refresh so newly created/extended recordings are discovered immediately.
                    self.rom_list.refresh_recording_for(&entry_path);
                    self.last_launch = Some(result);
                }
            }
            _ => {}
        }
        false
    }

    fn handle_help_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter => {
                self.input_mode = InputMode::Navigate;
            }
            _ => {}
        }
        false
    }
}

/// Render a help overlay centred on the screen.
fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let width = 50u16.min(area.width);
    let height = 16u16.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    let help_lines = vec![
        ListItem::new("  Navigation"),
        ListItem::new("  ──────────────────────────────"),
        ListItem::new("  ↑ / ↓        Move selection"),
        ListItem::new("  PgUp/PgDn    Scroll by 10 rows"),
        ListItem::new("  Home/End     Jump to first/last"),
        ListItem::new(""),
        ListItem::new("  Search"),
        ListItem::new("  ──────────────────────────────"),
        ListItem::new("  /            Open search bar"),
        ListItem::new("  Esc          Clear search / close"),
        ListItem::new(""),
        ListItem::new("  Actions"),
        ListItem::new("  ──────────────────────────────"),
        ListItem::new("  Enter        Open action menu"),
        ListItem::new("  ?            Show this help"),
        ListItem::new("  q / Esc      Quit"),
    ];

    let help = List::new(help_lines)
        .block(
            Block::default()
                .title(" Help — press Esc or ? to close ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().add_modifier(Modifier::DIM));

    frame.render_widget(help, popup);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entries(names: &[&str]) -> Vec<RomEntry> {
        use std::path::PathBuf;
        names
            .iter()
            .map(|n| RomEntry {
                path: PathBuf::from(format!("/roms/{n}.nes")),
                display_name: n.to_string(),
                mapper: Some(0),
                hardware: Some("NES NTSC".to_string()),
                crc: Some("DEADBEEF".to_string()),
                recording_duration: None,
            })
            .collect()
    }

    #[test]
    fn test_app_starts_in_navigate_mode() {
        let app = App::new(make_entries(&["Alpha"]));
        assert_eq!(app.input_mode, InputMode::Navigate);
    }

    #[test]
    fn test_app_empty_catalog_creates_without_panic() {
        let app = App::new(vec![]);
        assert_eq!(app.input_mode, InputMode::Navigate);
    }

    #[test]
    fn test_app_with_catalog_error_stores_message() {
        let app = App::new(vec![]).with_catalog_error("test error".to_string());
        assert!(app.catalog_error.is_some());
    }
}
