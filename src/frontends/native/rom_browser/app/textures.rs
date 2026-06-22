use super::*;

impl RomBrowserApp {
    /// Lazily load textures for visible grid items and evict offscreen ones.
    ///
    /// Image decoding is done on a background thread. This method:
    /// 1. Uploads any decoded results that are ready (fast GL upload only)
    /// 2. Sends decode requests for visible but unloaded textures
    /// 3. Evicts offscreen textures when the budget is exceeded
    pub(super) fn lazy_load_visible_textures(&mut self) {
        // 1. Upload any decoded results from the background thread (non-blocking).
        let mut decoded: Vec<(i64, u32, u32, Vec<u8>)> =
            Vec::with_capacity(Self::MAX_UPLOADS_PER_FRAME);
        while decoded.len() < Self::MAX_UPLOADS_PER_FRAME {
            match self.texture_result_rx.try_recv() {
                Ok(result) => decoded.push(result),
                Err(_) => break,
            }
        }
        if !decoded.is_empty() {
            if let Some(ref mut gl) = self.gl {
                for (game_id, w, h, pixels) in &decoded {
                    if *w > 0 && *h > 0 {
                        let key = TextureKey::CoverArt(*game_id);
                        gl.load_texture_from_rgba(key, *w, *h, pixels);
                    }
                }
            }
            for (game_id, _, _, _) in &decoded {
                self.texture_pending.retain(|&id| id != *game_id);
            }
        }

        let Some(ref gl) = self.gl else { return };

        let (display_w, display_h) = gl.logical_size();
        let sidebar_w = theme::sidebar_width(display_w);
        let grid_area_w = display_w - sidebar_w;
        let (cols, cover_w) = theme::grid_layout(grid_area_w);
        let cell_h = theme::cell_height(cover_w);
        let grid_height = display_h - theme::HEADER_HEIGHT;

        // Determine the range of rows visible on screen (with buffer).
        let first_visible_row =
            (self.scroll_offset / (cell_h + theme::GRID_SPACING)).floor() as usize;
        let rows_on_screen = (grid_height / (cell_h + theme::GRID_SPACING)).ceil() as usize + 1;
        let first_row = first_visible_row.saturating_sub(Self::PRELOAD_ROW_BUFFER);
        let last_row = first_visible_row + rows_on_screen + Self::PRELOAD_ROW_BUFFER;

        // Collect game IDs for entries in the visible range.
        let first_idx = first_row * cols;
        let last_idx = ((last_row + 1) * cols).min(self.filtered_indices.len());
        let range_start = first_idx.min(self.filtered_indices.len());
        let range_end = last_idx.min(self.filtered_indices.len());

        let mut visible_game_ids: Vec<i64> = Vec::with_capacity(range_end - range_start);
        for &fi in &self.filtered_indices[range_start..range_end] {
            if let Some(gid) = self.catalog[fi].metadata_game_id {
                visible_game_ids.push(gid);
            }
        }

        // Also include the selected entry (sidebar needs its texture).
        if let Some(&catalog_idx) = self.filtered_indices.get(self.selected_index)
            && let Some(gid) = self.catalog[catalog_idx].metadata_game_id
            && !visible_game_ids.contains(&gid)
        {
            visible_game_ids.push(gid);
        }

        // 2. Send decode requests for visible entries not yet loaded or pending.
        let mut requests_sent = 0;
        for &game_id in &visible_game_ids {
            if requests_sent >= Self::MAX_REQUESTS_PER_FRAME {
                break;
            }
            let key = TextureKey::CoverArt(game_id);
            if gl.get_texture(&key).is_some() || self.texture_pending.contains(&game_id) {
                continue;
            }
            if let Some(path) = self.boxart_by_game_id.get(&game_id)
                && path.exists()
                && self
                    .texture_request_tx
                    .send((game_id, path.clone()))
                    .is_ok()
            {
                self.texture_pending.push(game_id);
                requests_sent += 1;
            }
        }

        // 3. Evict offscreen textures when we exceed the budget.
        if gl.texture_count() > Self::MAX_CACHED_TEXTURES {
            let loaded_keys = gl.texture_keys();
            let mut to_evict: Vec<TextureKey> = Vec::new();
            for key in loaded_keys {
                if gl.texture_count().saturating_sub(to_evict.len()) <= Self::MAX_CACHED_TEXTURES {
                    break;
                }
                if let TextureKey::CoverArt(gid) = key
                    && !visible_game_ids.contains(&gid)
                {
                    to_evict.push(TextureKey::CoverArt(gid));
                }
            }
            let gl = self.gl.as_mut().unwrap();
            for key in to_evict {
                gl.remove_texture(&key);
            }
        }
    }

    /// Get the number of screenshots for the currently selected entry.
    fn detail_screenshot_count(&self) -> usize {
        self.selected_entry()
            .map(|e| e.screenshot_paths.len())
            .unwrap_or(0)
    }

    /// Advance the auto-scroll carousel for screenshots in the detail view.
    /// Pauses longer at the first and last screenshot, then reverses direction.
    pub(super) fn advance_screenshot_auto_scroll(&mut self) {
        let count = self.detail_screenshot_count();
        if count <= 1 {
            return;
        }
        let elapsed = self.detail_scroll_last_advance.elapsed().as_secs_f64();
        let pause =
            if self.detail_screenshot_index == 0 || self.detail_screenshot_index == count - 1 {
                2.0
            } else {
                1.5
            };
        if elapsed >= pause {
            self.detail_scroll_last_advance = Instant::now();
            if self.detail_scroll_forward {
                if self.detail_screenshot_index + 1 < count {
                    self.detail_screenshot_index += 1;
                } else {
                    self.detail_scroll_forward = false;
                    self.detail_screenshot_index -= 1;
                }
            } else if self.detail_screenshot_index > 0 {
                self.detail_screenshot_index -= 1;
            } else {
                self.detail_scroll_forward = true;
                self.detail_screenshot_index += 1;
            }
        }
    }
}
