//! TUI application state and main event loop.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
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
}

/// Main TUI application state.
pub(crate) struct App {
    rom_list: RomList,
    search: String,
    input_mode: InputMode,
    action_menu: Option<ActionMenu>,
    last_launch: Option<LaunchResult>,
}

impl App {
    pub fn new(entries: Vec<RomEntry>) -> Self {
        Self {
            rom_list: RomList::new(entries),
            search: String::new(),
            input_mode: InputMode::Navigate,
            action_menu: None,
            last_launch: None,
        }
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
                Constraint::Length(3), // search bar
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
        let footer_text = self
            .last_launch
            .as_ref()
            .map(|r| r.summary())
            .unwrap_or_else(|| {
                " ↑/↓: navigate  Enter: select  /: search  Esc: clear/quit  q: quit".to_string()
            });
        let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[2]);

        // Action menu overlay (rendered on top)
        if let Some(menu) = self.action_menu.as_mut() {
            menu.render(frame, area);
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
        terminal: &mut TerminalHandle,
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
                    self.action_menu = Some(ActionMenu::new(name));
                    self.input_mode = InputMode::ActionMenu;
                    // Clear previous launch status when opening a new menu
                    self.last_launch = None;
                    let _ = terminal; // suppress unused warning
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
                    // Re-enter alternate screen after launch_rom exits the TUI
                    let result = launch_rom(&rom_path, action);
                    // Restore TUI after the emulator exits
                    let _ = terminal.restore_alternate_screen();
                    self.last_launch = Some(result);
                }
            }
            _ => {}
        }
        false
    }
}
