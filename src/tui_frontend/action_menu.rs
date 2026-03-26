//! Action selection popup overlay for a chosen ROM.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::launcher::LaunchAction;

/// Actions offered in the popup menu.
const ACTIONS: [(&str, LaunchAction); 3] = [
    ("▶  Play", LaunchAction::Play),
    ("⏺  Record", LaunchAction::Record),
    ("⏵  Playback", LaunchAction::Playback),
];

/// State for the action selection popup.
pub(crate) struct ActionMenu {
    pub rom_name: String,
    list_state: ListState,
}

impl ActionMenu {
    pub fn new(rom_name: impl Into<String>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            rom_name: rom_name.into(),
            list_state,
        }
    }

    pub fn select_next(&mut self) {
        let next = self
            .list_state
            .selected()
            .map_or(0, |s| (s + 1).min(ACTIONS.len() - 1));
        self.list_state.select(Some(next));
    }

    pub fn select_prev(&mut self) {
        let prev = self
            .list_state
            .selected()
            .map_or(0, |s| s.saturating_sub(1));
        self.list_state.select(Some(prev));
    }

    /// Return the currently highlighted `LaunchAction`.
    pub fn selected_action(&self) -> LaunchAction {
        let idx = self.list_state.selected().unwrap_or(0);
        ACTIONS[idx.min(ACTIONS.len() - 1)].1
    }

    /// Render the popup centred over `area`.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(40, 9, area);

        // Clear the area behind the popup so the list is readable.
        frame.render_widget(Clear, popup_area);

        let title = format!(" {} ", truncate(&self.rom_name, 30));

        let items: Vec<ListItem> = ACTIONS
            .iter()
            .map(|(label, _)| ListItem::new(*label))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, popup_area, &mut self.list_state);

        // Footer hint
        let hint_area = Rect {
            y: popup_area.bottom().saturating_sub(1),
            height: 1,
            ..popup_area
        };
        let hint = Paragraph::new(" Enter: confirm  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, hint_area);
    }
}

/// Return a rectangle centred in `area` with the given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width.max(1),
        height: height.min(area.height),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!(
            "{}…",
            &s[..s
                .char_indices()
                .nth(max_chars - 1)
                .map_or(s.len(), |(i, _)| i)]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_menu_default_selection_is_play() {
        let menu = ActionMenu::new("Test ROM");
        assert_eq!(menu.selected_action(), LaunchAction::Play);
    }

    #[test]
    fn test_action_menu_select_next_moves_to_record() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        assert_eq!(menu.selected_action(), LaunchAction::Record);
    }

    #[test]
    fn test_action_menu_select_next_twice_reaches_playback() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        menu.select_next();
        assert_eq!(menu.selected_action(), LaunchAction::Playback);
    }

    #[test]
    fn test_action_menu_select_next_clamps_at_end() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        menu.select_next();
        menu.select_next(); // already at Playback
        assert_eq!(menu.selected_action(), LaunchAction::Playback);
    }

    #[test]
    fn test_action_menu_select_prev_wraps_back() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        menu.select_prev();
        assert_eq!(menu.selected_action(), LaunchAction::Play);
    }

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate("Short", 10), "Short");
    }

    #[test]
    fn test_truncate_long_string_gets_ellipsis() {
        let result = truncate("A very long ROM name that exceeds the limit", 10);
        assert!(result.contains('…'));
        assert!(result.chars().count() <= 10);
    }
}
