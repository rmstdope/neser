//! Stateful ROM list widget: scrollable table with real-time text filtering.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use super::rom_entry::RomEntry;

const COLUMN_HEADERS: [&str; 4] = ["Name", "Mapper", "Hardware", "CRC"];
const COLUMN_WIDTHS: [ratatui::layout::Constraint; 4] = [
    ratatui::layout::Constraint::Min(20),
    ratatui::layout::Constraint::Length(7),
    ratatui::layout::Constraint::Length(10),
    ratatui::layout::Constraint::Length(10),
];

/// Manages a searchable/filterable list of ROM entries.
pub(crate) struct RomList {
    /// All loaded ROM entries.
    all: Vec<RomEntry>,
    /// Indices into `all` that pass the current filter.
    filtered: Vec<usize>,
    /// ratatui table scroll state.
    state: TableState,
    /// Current filter string (lowercase).
    filter: String,
}

impl RomList {
    pub fn new(entries: Vec<RomEntry>) -> Self {
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let mut state = TableState::default();
        if !filtered.is_empty() {
            state.select(Some(0));
        }
        Self {
            all: entries,
            filtered,
            state,
            filter: String::new(),
        }
    }

    /// Apply `filter` as a case-insensitive substring match on the display name.
    pub fn set_filter(&mut self, filter: &str) {
        self.filter = filter.to_lowercase();
        let needle = &self.filter;
        self.filtered = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, e)| e.search_key.contains(needle.as_str()))
            .map(|(i, _)| i)
            .collect();

        // Keep or reset selection
        if self.filtered.is_empty() {
            self.state.select(None);
        } else {
            let current = self.state.selected().unwrap_or(0);
            let clamped = current.min(self.filtered.len().saturating_sub(1));
            self.state.select(Some(clamped));
        }
    }

    /// Move selection one row down.
    pub fn select_next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let next = self
            .state
            .selected()
            .map_or(0, |s| (s + 1).min(self.filtered.len() - 1));
        self.state.select(Some(next));
    }

    /// Move selection one row up.
    pub fn select_prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let prev = self.state.selected().map_or(0, |s| s.saturating_sub(1));
        self.state.select(Some(prev));
    }

    /// Move selection one page down.
    pub fn select_page_down(&mut self, page_size: usize) {
        if self.filtered.is_empty() {
            return;
        }
        let next = self
            .state
            .selected()
            .map_or(0, |s| (s + page_size).min(self.filtered.len() - 1));
        self.state.select(Some(next));
    }

    /// Move selection one page up.
    pub fn select_page_up(&mut self, page_size: usize) {
        if self.filtered.is_empty() {
            return;
        }
        let prev = self
            .state
            .selected()
            .map_or(0, |s| s.saturating_sub(page_size));
        self.state.select(Some(prev));
    }

    #[allow(dead_code)]
    /// Return the currently selected `RomEntry`, if any.
    pub fn selected_entry(&self) -> Option<&RomEntry> {
        let idx = *self.filtered.get(self.state.selected()?)?;
        self.all.get(idx)
    }

    /// Re-read the `.autorun` recording status for the entry at `path` and update it in place.
    ///
    /// Call this after a Record/ExtendRecording launch so newly created recordings are
    /// discovered without a full catalog reload.
    pub fn refresh_recording_for(&mut self, path: &std::path::Path) {
        use crate::platform::catalog::read_recording_duration;
        for entry in self.all.iter_mut() {
            if entry.path == path {
                entry.recording_duration = read_recording_duration(path);
                break;
            }
        }
    }

    /// Total number of entries after filtering.
    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    /// Total number of entries before filtering.
    pub fn total_count(&self) -> usize {
        self.all.len()
    }

    /// Render the ROM list table into `area`.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!(" ROMs ({}/{}) ", self.filtered_count(), self.total_count());

        let header = Row::new(COLUMN_HEADERS.map(|h| {
            Cell::from(h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        }))
        .height(1);

        let rows: Vec<Row> = self
            .filtered
            .iter()
            .map(|&i| {
                let e = &self.all[i];
                Row::new([
                    Cell::from(e.display_name.as_str()),
                    Cell::from(e.mapper_label.as_str()),
                    Cell::from(e.hardware_label()),
                    Cell::from(e.crc_label()),
                ])
            })
            .collect();

        let table = Table::new(rows, COLUMN_WIDTHS)
            .header(header)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .row_highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::catalog::Platform;
    use std::path::PathBuf;

    fn make_entries(names: &[&str]) -> Vec<RomEntry> {
        names
            .iter()
            .map(|n| RomEntry {
                path: PathBuf::from(format!("/roms/{n}.nes")),
                display_name: n.to_string(),
                search_key: n.to_lowercase(),
                mapper_label: "0".to_string(),
                mapper: Some(0),
                hardware: Some("NES NTSC".to_string()),
                crc: Some("DEADBEEF".to_string()),
                recording_duration: None,
                metadata_game_id: None,
                genres: Vec::new(),
                overview: None,
                release_date: None,
                players: None,
                rating: None,
                boxart_path: None,
                screenshot_paths: Vec::new(),
                is_favorite: false,
                platform: Platform::Nes,
            })
            .collect()
    }

    #[test]
    fn test_new_selects_first_entry() {
        let list = RomList::new(make_entries(&["Alpha", "Beta", "Gamma"]));
        assert_eq!(list.state.selected(), Some(0));
    }

    #[test]
    fn test_new_empty_has_no_selection() {
        let list = RomList::new(vec![]);
        assert_eq!(list.state.selected(), None);
    }

    #[test]
    fn test_filter_case_insensitive() {
        let mut list = RomList::new(make_entries(&["Super Mario", "Mega Man", "mega drive"]));
        list.set_filter("mega");
        assert_eq!(list.filtered_count(), 2);
    }

    #[test]
    fn test_filter_empty_string_shows_all() {
        let mut list = RomList::new(make_entries(&["Alpha", "Beta", "Gamma"]));
        list.set_filter("zz");
        assert_eq!(list.filtered_count(), 0);
        list.set_filter("");
        assert_eq!(list.filtered_count(), 3);
    }

    #[test]
    fn test_filter_no_match_clears_selection() {
        let mut list = RomList::new(make_entries(&["Alpha", "Beta"]));
        list.set_filter("zzznomatch");
        assert_eq!(list.state.selected(), None);
    }

    #[test]
    fn test_select_next_advances_selection() {
        let mut list = RomList::new(make_entries(&["A", "B", "C"]));
        assert_eq!(list.state.selected(), Some(0));
        list.select_next();
        assert_eq!(list.state.selected(), Some(1));
    }

    #[test]
    fn test_select_next_clamps_at_end() {
        let mut list = RomList::new(make_entries(&["A", "B"]));
        list.select_next();
        list.select_next(); // already at end
        assert_eq!(list.state.selected(), Some(1));
    }

    #[test]
    fn test_select_prev_moves_back() {
        let mut list = RomList::new(make_entries(&["A", "B", "C"]));
        list.select_next();
        list.select_next();
        list.select_prev();
        assert_eq!(list.state.selected(), Some(1));
    }

    #[test]
    fn test_select_prev_clamps_at_start() {
        let mut list = RomList::new(make_entries(&["A", "B"]));
        list.select_prev(); // already at 0
        assert_eq!(list.state.selected(), Some(0));
    }

    #[test]
    fn test_selected_entry_returns_correct_item() {
        let entries = make_entries(&["Alpha", "Beta", "Gamma"]);
        let mut list = RomList::new(entries);
        list.select_next();
        let selected = list.selected_entry().unwrap();
        assert_eq!(selected.display_name, "Beta");
    }

    #[test]
    fn test_selected_entry_after_filter() {
        let mut list = RomList::new(make_entries(&["Super Mario", "Mega Man"]));
        list.set_filter("mega");
        let selected = list.selected_entry().unwrap();
        assert_eq!(selected.display_name, "Mega Man");
    }

    #[test]
    fn test_total_count_unchanged_by_filter() {
        let mut list = RomList::new(make_entries(&["A", "B", "C"]));
        list.set_filter("zzz");
        assert_eq!(list.total_count(), 3);
        assert_eq!(list.filtered_count(), 0);
    }

    #[test]
    fn test_refresh_recording_for_updates_duration_when_file_appears() {
        use tempfile::NamedTempFile;
        // Create a real temp .nes file so the path can have a real sibling .autorun
        let tmp = NamedTempFile::with_suffix(".nes").unwrap();
        let rom_path = tmp.path().to_path_buf();
        let entry = RomEntry {
            path: rom_path.clone(),
            display_name: "Test".to_string(),
            search_key: "test".to_string(),
            mapper_label: "0".to_string(),
            mapper: Some(0),
            hardware: None,
            crc: None,
            recording_duration: None,
            metadata_game_id: None,
            genres: Vec::new(),
            overview: None,
            release_date: None,
            players: None,
            rating: None,
            boxart_path: None,
            screenshot_paths: Vec::new(),
            is_favorite: false,
            platform: Platform::Nes,
        };
        let mut list = RomList::new(vec![entry]);

        // Initially no recording
        assert!(list.all[0].recording_duration.is_none());

        // Create a .autorun file with 600 frames (10 seconds)
        let autorun_path = rom_path.with_extension("autorun");
        let json =
            r#"{"version":3,"frames":[{"player1":0,"player2":0,"repeat":600}],"checkpoints":[]}"#;
        std::fs::write(&autorun_path, json.as_bytes()).unwrap();

        list.refresh_recording_for(&rom_path);
        let _ = std::fs::remove_file(&autorun_path);

        let dur = list.all[0]
            .recording_duration
            .expect("should have duration after refresh");
        assert_eq!(dur.as_secs(), 10, "600 frames at 60fps = 10 seconds");
    }
}
