//! Action selection popup overlay for a chosen ROM.

use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::launcher::LaunchAction;

/// State for the action selection popup.
pub(crate) struct ActionMenu {
    pub rom_name: String,
    list_state: ListState,
    /// Available actions with their display labels (dynamic, based on recording state).
    actions: Vec<(String, LaunchAction)>,
}

impl ActionMenu {
    /// Build the action list based on whether a recording exists and its duration.
    ///
    /// - Always present: Play (fullscreen), Play (windowed), Record (new/overwrite)
    /// - When recording exists: Playback (MM:SS), Extend recording
    pub fn new_with_recording(
        rom_name: impl Into<String>,
        recording_duration: Option<Duration>,
    ) -> Self {
        let mut actions: Vec<(String, LaunchAction)> = vec![
            (
                "▶  Play (fullscreen)".to_string(),
                LaunchAction::PlayFullscreen,
            ),
            ("▶  Play (windowed)".to_string(), LaunchAction::PlayWindowed),
        ];

        let record_label = if recording_duration.is_some() {
            "⏺  Record (overwrite)".to_string()
        } else {
            "⏺  Record (new)".to_string()
        };
        actions.push((record_label, LaunchAction::Record));

        if let Some(duration) = recording_duration {
            actions.push((
                format!("⏵  Playback ({})", format_duration(duration)),
                LaunchAction::Playback,
            ));
            actions.push((
                "⏭  Extend recording".to_string(),
                LaunchAction::ExtendRecording,
            ));
        }

        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            rom_name: rom_name.into(),
            list_state,
            actions,
        }
    }

    /// Convenience constructor — no recording, Play + Play + Record (new) only. Used in tests.
    #[cfg(test)]
    pub fn new(rom_name: impl Into<String>) -> Self {
        Self::new_with_recording(rom_name, None)
    }

    pub fn select_next(&mut self) {
        let next = self
            .list_state
            .selected()
            .map_or(0, |s| (s + 1).min(self.actions.len() - 1));
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
        self.actions[idx.min(self.actions.len() - 1)].1
    }

    /// Render the popup centred over `area`.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        let popup_height = (self.actions.len() as u16 + 4).max(7);
        let popup_area = centered_rect(50, popup_height, area);

        frame.render_widget(Clear, popup_area);

        let title = format!(" {} ", truncate(&self.rom_name, 30));

        let items: Vec<ListItem> = self
            .actions
            .iter()
            .map(|(label, _)| ListItem::new(label.as_str()))
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

/// Format a duration as `MM:SS` or `H:MM:SS` for recordings longer than one hour.
fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
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
    fn test_action_menu_default_selection_is_play_fullscreen() {
        let menu = ActionMenu::new("Test ROM");
        assert_eq!(menu.selected_action(), LaunchAction::PlayFullscreen);
    }

    #[test]
    fn test_action_menu_select_next_moves_to_play_windowed() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        assert_eq!(menu.selected_action(), LaunchAction::PlayWindowed);
    }

    #[test]
    fn test_action_menu_select_next_twice_reaches_record() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        menu.select_next();
        assert_eq!(menu.selected_action(), LaunchAction::Record);
    }

    #[test]
    fn test_action_menu_select_next_clamps_at_record_when_no_recording() {
        let mut menu = ActionMenu::new("Test ROM");
        for _ in 0..10 {
            menu.select_next();
        }
        assert_eq!(menu.selected_action(), LaunchAction::Record);
    }

    #[test]
    fn test_action_menu_select_prev_wraps_back() {
        let mut menu = ActionMenu::new("Test ROM");
        menu.select_next();
        menu.select_prev();
        assert_eq!(menu.selected_action(), LaunchAction::PlayFullscreen);
    }

    #[test]
    fn test_action_menu_without_recording_excludes_playback_and_extend() {
        let mut m = ActionMenu::new_with_recording("Test ROM", None);
        let mut seen = vec![m.selected_action()];
        for _ in 0..10 {
            m.select_next();
            seen.push(m.selected_action());
        }
        assert!(
            !seen.contains(&LaunchAction::Playback),
            "Playback should not appear without recording: {seen:?}"
        );
        assert!(
            !seen.contains(&LaunchAction::ExtendRecording),
            "ExtendRecording should not appear without recording: {seen:?}"
        );
    }

    #[test]
    fn test_action_menu_with_recording_includes_playback_and_extend() {
        let dur = Duration::from_secs(330); // 5m30s
        let mut menu = ActionMenu::new_with_recording("Test ROM", Some(dur));
        let mut seen = vec![menu.selected_action()];
        for _ in 0..10 {
            menu.select_next();
            seen.push(menu.selected_action());
        }
        assert!(
            seen.contains(&LaunchAction::Playback),
            "Playback should appear with recording"
        );
        assert!(
            seen.contains(&LaunchAction::ExtendRecording),
            "ExtendRecording should appear with recording"
        );
    }

    #[test]
    fn test_action_menu_record_label_is_new_without_recording() {
        let menu = ActionMenu::new_with_recording("Test ROM", None);
        let labels: Vec<&str> = menu.actions.iter().map(|(l, _)| l.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("Record (new)")),
            "should show 'Record (new)' without recording"
        );
    }

    #[test]
    fn test_action_menu_record_label_is_overwrite_with_recording() {
        let menu = ActionMenu::new_with_recording("Test ROM", Some(Duration::from_secs(60)));
        let labels: Vec<&str> = menu.actions.iter().map(|(l, _)| l.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("Record (overwrite)")),
            "should show 'Record (overwrite)' when recording exists"
        );
    }

    #[test]
    fn test_action_menu_playback_label_shows_duration() {
        let menu = ActionMenu::new_with_recording("Test ROM", Some(Duration::from_secs(330)));
        let labels: Vec<&str> = menu.actions.iter().map(|(l, _)| l.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("05:30")),
            "Playback label should contain formatted duration: {labels:?}"
        );
    }

    #[test]
    fn test_format_duration_under_one_hour() {
        assert_eq!(format_duration(Duration::from_secs(330)), "05:30");
        assert_eq!(format_duration(Duration::from_secs(90)), "01:30");
        assert_eq!(format_duration(Duration::from_secs(0)), "00:00");
    }

    #[test]
    fn test_format_duration_over_one_hour() {
        assert_eq!(format_duration(Duration::from_secs(3690)), "1:01:30");
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
