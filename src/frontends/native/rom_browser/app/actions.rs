use super::*;

impl RomBrowserApp {
    /// Open the detail view for the currently selected entry.
    pub(super) fn open_detail_view(&mut self) {
        if self.selected_entry().is_some() {
            self.detail_view_active = true;
            self.detail_screenshot_index = 0;
            self.detail_scroll_last_advance = Instant::now();
            self.detail_scroll_forward = true;
        }
    }

    /// Open the search panel overlay.
    pub(super) fn open_search_panel(&mut self) {
        self.filter_panel_active = false;
        self.detail_view_active = false;
        self.search_active = true;
        self.search_kb_row = 1;
        self.search_kb_col = 0;
    }

    /// Close the search panel overlay.
    pub(super) fn close_search_panel(&mut self) {
        self.search_active = false;
    }

    /// Handle confirm action on the on-screen keyboard.
    pub(super) fn search_kb_confirm(&mut self) {
        let row = &Self::SEARCH_KB_ROWS[self.search_kb_row];
        if self.search_kb_col < row.len() {
            let ch = row[self.search_kb_col];
            match ch {
                '\u{232B}' => {
                    // Backspace
                    self.search_query.pop();
                }
                '\u{21B5}' => {
                    // Enter — close search
                    self.close_search_panel();
                    return;
                }
                _ => {
                    self.search_query.push(ch.to_ascii_lowercase());
                }
            }
            self.rebuild_filtered();
        }
    }

    /// Move on-screen keyboard cursor up.
    pub(super) fn search_kb_move_up(&mut self) {
        if self.search_kb_row > 0 {
            self.search_kb_row -= 1;
            let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
            if self.search_kb_col >= row_len {
                self.search_kb_col = row_len - 1;
            }
        }
    }

    /// Move on-screen keyboard cursor down.
    pub(super) fn search_kb_move_down(&mut self) {
        if self.search_kb_row + 1 < Self::SEARCH_KB_ROWS.len() {
            self.search_kb_row += 1;
            let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
            if self.search_kb_col >= row_len {
                self.search_kb_col = row_len - 1;
            }
        }
    }

    /// Move on-screen keyboard cursor left.
    pub(super) fn search_kb_move_left(&mut self) {
        if self.search_kb_col > 0 {
            self.search_kb_col -= 1;
        }
    }

    /// Move on-screen keyboard cursor right.
    pub(super) fn search_kb_move_right(&mut self) {
        let row_len = Self::SEARCH_KB_ROWS[self.search_kb_row].len();
        if self.search_kb_col + 1 < row_len {
            self.search_kb_col += 1;
        }
    }

    /// Open the filter panel overlay.
    pub(super) fn open_filter_panel(&mut self) {
        self.search_active = false;
        self.detail_view_active = false;
        self.filter_panel_active = true;
        self.filter_panel_cursor = 0;
        self.filter_panel_column = 0;
    }

    /// Close the filter panel overlay.
    pub(super) fn close_filter_panel(&mut self) {
        self.filter_panel_active = false;
    }

    /// Handle confirm action within the filter panel.
    pub(super) fn filter_panel_confirm(&mut self) {
        let cursor = self.filter_panel_cursor;
        match self.filter_panel_column {
            0 => {
                // Platform column
                if cursor < Self::PLATFORMS.len() {
                    let selected = Self::PLATFORMS[cursor];
                    if self.active_platform == Some(selected) {
                        self.active_platform = None;
                    } else {
                        self.active_platform = Some(selected);
                    }
                }
            }
            1 => {
                // Players column
                if cursor < Self::PLAYER_OPTIONS.len() {
                    let (value, _) = Self::PLAYER_OPTIONS[cursor];
                    if self.min_players_filter == value && value.is_some() {
                        self.min_players_filter = None;
                    } else {
                        self.min_players_filter = value;
                    }
                }
            }
            3 => {
                // Favorites column
                self.show_favorites_only = !self.show_favorites_only;
                self.rebuild_filtered();
            }
            2 => {
                // Genre column
                if let Some(genre) = self.available_genres.get(cursor).cloned() {
                    if let Some(pos) = self.active_genres.iter().position(|g| *g == genre) {
                        self.active_genres.remove(pos);
                    } else {
                        self.active_genres.push(genre);
                    }
                }
            }
            _ => {}
        }
        self.rebuild_filtered();
    }

    /// Move the filter panel cursor up within the current column.
    pub(super) fn filter_panel_move_cursor_up(&mut self) {
        if self.filter_panel_cursor > 0 {
            self.filter_panel_cursor -= 1;
        }
    }

    /// Move the filter panel cursor down within the current column.
    pub(super) fn filter_panel_move_cursor_down(&mut self) {
        let max = self
            .filter_panel_column_len(self.filter_panel_column)
            .saturating_sub(1);
        if self.filter_panel_cursor < max {
            self.filter_panel_cursor += 1;
        }
    }

    /// Return the number of items in a given filter panel column.
    fn filter_panel_column_len(&self, col: usize) -> usize {
        match col {
            0 => Self::PLATFORMS.len(),
            1 => Self::PLAYER_OPTIONS.len(),
            2 => self.available_genres.len(),
            // Favorites column: a single "favorites only" toggle.
            3 => 1,
            _ => 0,
        }
    }

    /// Move the filter panel to the previous column.
    pub(super) fn filter_panel_move_left(&mut self) {
        if self.filter_panel_column > 0 {
            self.filter_panel_column -= 1;
            let max = self
                .filter_panel_column_len(self.filter_panel_column)
                .saturating_sub(1);
            self.filter_panel_cursor = self.filter_panel_cursor.min(max);
        }
    }

    /// Move the filter panel to the next column.
    pub(super) fn filter_panel_move_right(&mut self) {
        let next = self.filter_panel_column + 1;
        let next_len = self.filter_panel_column_len(next);
        if next_len > 0 {
            self.filter_panel_column = next;
            self.filter_panel_cursor = self.filter_panel_cursor.min(next_len.saturating_sub(1));
        }
    }

    /// Apply a browser action (shared between keyboard and gamepad).
    pub(super) fn apply_action(&mut self, action: BrowserAction, event_loop: &ActiveEventLoop) {
        if self.search_active {
            match action {
                BrowserAction::Back => self.close_search_panel(),
                BrowserAction::Up => self.search_kb_move_up(),
                BrowserAction::Down => self.search_kb_move_down(),
                BrowserAction::Left => self.search_kb_move_left(),
                BrowserAction::Right => self.search_kb_move_right(),
                BrowserAction::Confirm => self.search_kb_confirm(),
                _ => {}
            }
        } else if self.filter_panel_active {
            match action {
                BrowserAction::Back => self.close_filter_panel(),
                BrowserAction::Up => self.filter_panel_move_cursor_up(),
                BrowserAction::Down => self.filter_panel_move_cursor_down(),
                BrowserAction::Left => self.filter_panel_move_left(),
                BrowserAction::Right => self.filter_panel_move_right(),
                BrowserAction::Confirm => self.filter_panel_confirm(),
                _ => {}
            }
        } else if self.detail_view_active {
            match action {
                BrowserAction::Back => self.detail_view_active = false,
                BrowserAction::Confirm => {
                    if let Some(entry) = self.selected_entry() {
                        self.result = BrowserResult::RomSelected(entry.path.clone());
                        event_loop.exit();
                    }
                }
                BrowserAction::Favorite => self.toggle_favorite(),
                _ => {}
            }
        } else {
            match action {
                BrowserAction::Up => self.navigate_up(),
                BrowserAction::Down => self.navigate_down(),
                BrowserAction::Left => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                        self.ensure_selected_visible();
                    }
                }
                BrowserAction::Right => {
                    if self.selected_index + 1 < self.filtered_indices.len() {
                        self.selected_index += 1;
                        self.ensure_selected_visible();
                    }
                }
                BrowserAction::Confirm => {
                    // In grid mode, Confirm opens the detail view.
                    self.open_detail_view();
                }
                BrowserAction::Back => {
                    self.open_filter_panel();
                }
                BrowserAction::Search => {
                    self.open_search_panel();
                }
                BrowserAction::Favorite => self.toggle_favorite(),
                BrowserAction::Detail => {
                    self.open_detail_view();
                }
            }
        }
    }
}
