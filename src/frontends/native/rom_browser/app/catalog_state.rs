use super::*;

impl RomBrowserApp {
    /// Set the ROM catalog to display.
    pub fn set_catalog(&mut self, mut catalog: Vec<RomEntry>) {
        // Apply stored favorites to catalog entries.
        for entry in &mut catalog {
            entry.is_favorite = self.favorites.contains(&entry.path);
        }

        // Collect all unique genres from the catalog.
        let mut genres: Vec<String> = catalog
            .iter()
            .flat_map(|e| e.genres.iter().cloned())
            .collect();
        genres.sort();
        genres.dedup();
        self.available_genres = genres;

        self.catalog = catalog;

        // Build fast game_id→boxart_path lookup.
        self.boxart_by_game_id.clear();
        for entry in &self.catalog {
            if let (Some(gid), Some(path)) = (entry.metadata_game_id, &entry.boxart_path) {
                self.boxart_by_game_id.insert(gid, path.clone());
            }
        }

        self.rebuild_filtered();
    }

    /// Rebuild the filtered index list based on current search query, genre, and favorites filter.
    pub(super) fn rebuild_filtered(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .catalog
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                // Platform filter.
                if matches!(self.active_platform, Some(plat) if e.platform != plat) {
                    return false;
                }
                // Favorites filter.
                if self.show_favorites_only && !e.is_favorite {
                    return false;
                }
                // Text search filter.
                if !query.is_empty()
                    && !e.search_key.contains(&query)
                    && !e.display_name.to_lowercase().contains(&query)
                {
                    return false;
                }
                // Genre filter: entry must have at least one of the active genres.
                if !self.active_genres.is_empty()
                    && !e.genres.iter().any(|g| self.active_genres.contains(g))
                {
                    return false;
                }
                // Players filter.
                if let Some(min) = self.min_players_filter
                    && e.players.unwrap_or(1) < min
                {
                    return false;
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        // Clamp selection.
        if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }
        self.scroll_offset = 0.0;
        self.scroll_target = 0.0;
    }

    /// Get the catalog entry for the currently selected filtered item.
    pub(super) fn selected_entry(&self) -> Option<&RomEntry> {
        let &catalog_idx = self.filtered_indices.get(self.selected_index)?;
        self.catalog.get(catalog_idx)
    }

    /// Toggle favorite status for the currently selected ROM.
    pub(super) fn toggle_favorite(&mut self) {
        if let Some(&catalog_idx) = self.filtered_indices.get(self.selected_index)
            && let Some(entry) = self.catalog.get_mut(catalog_idx)
        {
            let new_status = self.favorites.toggle(&entry.path);
            entry.is_favorite = new_status;
            if let Err(e) = self.favorites.save() {
                crate::platform::debugging::log_info(format!("Failed to save favorites: {e}"));
            }
            // If showing favorites only and we just unfavorited, rebuild filter.
            if self.show_favorites_only && !new_status {
                self.rebuild_filtered();
            }
        }
    }

    /// Check if the background catalog loading thread has finished.
    pub(super) fn poll_catalog_loading(&mut self) {
        // Drain all pending messages — update progress or finalize catalog.
        loop {
            let msg = if let CatalogState::Loading { ref receiver, .. } = self.catalog_state {
                receiver.try_recv().ok()
            } else {
                break;
            };
            match msg {
                Some(CatalogMessage::Progress(p)) => {
                    if let CatalogState::Loading {
                        ref mut progress, ..
                    } = self.catalog_state
                    {
                        *progress = Some(p);
                    }
                }
                Some(CatalogMessage::Done(catalog)) => {
                    self.set_catalog(catalog);
                    self.catalog_state = CatalogState::Ready;
                    break;
                }
                None => break,
            }
        }
    }

    /// Ensure the selected cell is visible by adjusting scroll target.
    pub(super) fn ensure_selected_visible(&mut self) {
        let Some(ref gl) = self.gl else { return };
        let (display_w, display_h) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let grid_height = display_h - theme::HEADER_HEIGHT;

        let row = self.selected_index / cols;
        let cell_top = theme::GRID_PADDING + row as f32 * (cell_h + theme::GRID_SPACING);
        let cell_bottom = cell_top + cell_h + theme::GRID_SPACING;

        if cell_top < self.scroll_target {
            self.scroll_target = cell_top - theme::GRID_PADDING;
        }
        if cell_bottom > self.scroll_target + grid_height {
            self.scroll_target = cell_bottom - grid_height + theme::GRID_PADDING;
        }
        self.scroll_target = self.scroll_target.max(0.0);
    }

    /// Get the current number of grid columns based on window size.
    fn current_cols(&self) -> usize {
        let Some(ref gl) = self.gl else { return 1 };
        let (display_w, _) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, _) = theme::grid_layout(grid_area_w);
        cols
    }

    /// Move selection up by one row in the grid.
    pub(super) fn navigate_up(&mut self) {
        let cols = self.current_cols();
        if self.selected_index >= cols {
            self.selected_index -= cols;
        } else {
            self.selected_index = 0;
        }
        self.ensure_selected_visible();
    }

    /// Move selection down by one row in the grid.
    pub(super) fn navigate_down(&mut self) {
        let cols = self.current_cols();
        let count = self.filtered_indices.len();
        let new_idx = self.selected_index + cols;
        if new_idx < count {
            self.selected_index = new_idx;
        } else if self.selected_index < count.saturating_sub(1) {
            self.selected_index = count - 1;
        }
        self.ensure_selected_visible();
    }
}
