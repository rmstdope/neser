/// Colour info for a single sprite pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// Colour index (1–3; 0 is transparent and will never appear here).
    pub colour_index: u8,
    /// Palette selection: 0 = OBP0, 1 = OBP1.
    pub palette: u8,
    /// Priority: if true the sprite is drawn behind BG colours 1–3.
    pub bg_priority: bool,
}

/// Collect up to 10 OAM entry indices for sprites visible on `scanline`.
///
/// Sprites are returned in OAM order (lower index = higher priority).
///
/// # Arguments
/// * `scanline` — Current LY (0–143)
/// * `oam`      — Full 160-byte OAM array
/// * `lcdc`     — Current LCDC value (bit 2 selects 8×8 vs 8×16)
pub fn scan_oam_line(scanline: u8, oam: &[u8; 0xA0], lcdc: u8) -> Vec<usize> {
    let height: u8 = if lcdc & 0x04 != 0 { 16 } else { 8 };
    let mut result = Vec::new();
    for i in 0..40usize {
        let oam_y = oam[i * 4];
        // OAM Y stores screen_y + 16; screen_y = oam_y.wrapping_sub(16).
        // Off-screen sprites (oam_y == 0 or screen_y > 143) are naturally
        // excluded because the wrapping converts them to screen_y > 143.
        let screen_y = oam_y.wrapping_sub(16);
        if scanline >= screen_y && scanline < screen_y.wrapping_add(height) {
            result.push(i);
            if result.len() >= 10 {
                break;
            }
        }
    }
    result
}

/// Fetch the highest-priority visible sprite pixel at screen position `x`.
///
/// Returns `None` if no opaque sprite pixel exists at this X coordinate.
///
/// # Arguments
/// * `x`              — Screen X coordinate (0–159)
/// * `scanline`       — Current LY
/// * `sprite_indices` — Pre-scanned OAM indices (from `scan_oam_line`)
/// * `oam`            — Full OAM array
/// * `vram`           — Full 8 KiB VRAM
/// * `lcdc`           — Current LCDC value
pub fn fetch_sprite_pixel(
    x: u32,
    scanline: u8,
    sprite_indices: &[usize],
    oam: &[u8; 0xA0],
    vram: &[u8; 0x2000],
    lcdc: u8,
) -> Option<SpritePixel> {
    let height: u8 = if lcdc & 0x04 != 0 { 16 } else { 8 };

    // DMG drawing priority: lower OAM X wins; equal X breaks ties by lower OAM index.
    // https://gbdev.io/pandocs/OAM.html#drawing-priority
    // `scan_oam_line` caps sprites at 10, so a fixed stack buffer avoids heap allocation
    // in this hot path (called once per screen pixel, 160×144 times per frame).
    let mut sorted = [0usize; 10];
    let mut count = 0usize;
    for &i in sprite_indices.iter().take(10) {
        sorted[count] = i;
        count += 1;
    }
    sorted[..count].sort_by_key(|&i| (oam[i * 4 + 1], i));

    for &i in &sorted[..count] {
        let oam_y = oam[i * 4];
        let oam_x = oam[i * 4 + 1];
        let tile_num = oam[i * 4 + 2];
        let attrs = oam[i * 4 + 3];

        let screen_y = oam_y.wrapping_sub(16);
        let screen_x = oam_x.wrapping_sub(8);

        // Skip sprites that don't cover column x.
        if x < screen_x as u32 || x >= screen_x as u32 + 8 {
            continue;
        }

        let y_flip = attrs & 0x40 != 0;
        let x_flip = attrs & 0x20 != 0;
        let palette = (attrs >> 4) & 1;
        let bg_priority = attrs & 0x80 != 0;

        let mut row = (scanline - screen_y) as usize;
        if y_flip {
            row = (height as usize - 1) - row;
        }

        let mut pixel_x = (x as u8).wrapping_sub(screen_x);
        if x_flip {
            pixel_x = 7 - pixel_x;
        }

        // For 8×16, select upper or lower tile (bit 0 forced).
        let tile_index = if height == 16 {
            if row < 8 {
                (tile_num & 0xFE) as usize
            } else {
                row -= 8;
                (tile_num | 0x01) as usize
            }
        } else {
            tile_num as usize
        };

        let tile_addr = tile_index * 16;
        let low = vram[tile_addr + row * 2];
        let high = vram[tile_addr + row * 2 + 1];
        let bit = 7 - pixel_x;
        let colour_index = ((high >> bit) & 1) << 1 | ((low >> bit) & 1);

        if colour_index == 0 {
            continue; // colour 0 is transparent
        }

        return Some(SpritePixel {
            colour_index,
            palette,
            bg_priority,
        });
    }
    None
}

/// Flat dot cost per visible sprite (OBJ tile fetch).
const OBJ_FETCH_DOTS: u16 = 6;

/// BG tile width in pixels.
const BG_TILE_WIDTH: i16 = 8;

/// Sprites with OAM X ≥ 168 are fully off-screen right and incur no penalty.
const OAM_X_OFFSCREEN: u8 = 168;

/// Maximum tile-wait penalty when a sprite is the first on its BG tile.
/// Per Pan Docs: tile_wait = max(MAX_TILE_WAIT − pos_in_tile, 0).
const MAX_TILE_WAIT: u16 = 5;

/// Calculate Mode 3 OBJ penalty dots for the sprites on the current scanline.
///
/// Implements the Pan Docs "OBJ penalty algorithm":
/// - Sprites are processed left-to-right by OAM X (ties broken by OAM index)
/// - Each visible sprite incurs a flat 6-dot penalty (OBJ tile fetch)
/// - Additional 0–5 dot tile-wait penalty depends on BG tile alignment
/// - OAM X == 0 exception: always max tile-wait regardless of SCX
/// - OAM X >= 168: off-screen right, no penalty
///
/// Returns total penalty in dots (T-cycles).
pub fn calculate_obj_penalty(sprite_indices: &[usize], oam: &[u8; 0xA0], scx: u8) -> u16 {
    debug_assert!(
        sprite_indices.len() <= 10,
        "OAM scan is capped at 10 sprites per scanline"
    );
    if sprite_indices.is_empty() {
        return 0;
    }

    // Process sprites left-to-right (ascending OAM X, then OAM index).
    let mut sorted: Vec<(usize, u8)> = sprite_indices
        .iter()
        .map(|&i| (i, oam[i * 4 + 1]))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let mut total_penalty: u16 = 0;
    // First sprite on a given BG tile pays the tile-wait; subsequent sprites share it.
    let mut seen_tiles: [i16; 10] = [i16::MIN; 10];
    let mut seen_count: usize = 0;

    for &(_, oam_x) in &sorted {
        if oam_x >= OAM_X_OFFSCREEN {
            continue;
        }

        let screen_x = oam_x as i16 - 8; // OAM X = screen_x + 8
        let bg_x = screen_x + scx as i16;
        let tile_id = bg_x.div_euclid(BG_TILE_WIDTH);

        if !seen_tiles[..seen_count].contains(&tile_id) {
            total_penalty += tile_wait_penalty(oam_x, bg_x);
            if seen_count < seen_tiles.len() {
                seen_tiles[seen_count] = tile_id;
                seen_count += 1;
            }
        }
        total_penalty += OBJ_FETCH_DOTS;
    }

    total_penalty
}

/// Tile-wait penalty for a sprite that is the first on its BG tile.
///
/// OAM X == 0 always gets the maximum penalty (Pan Docs exception);
/// otherwise penalty decreases as the sprite moves rightward within the tile.
fn tile_wait_penalty(oam_x: u8, bg_x: i16) -> u16 {
    if oam_x == 0 {
        return MAX_TILE_WAIT;
    }
    let pos_in_tile = bg_x.rem_euclid(BG_TILE_WIDTH) as u16;
    MAX_TILE_WAIT.saturating_sub(pos_in_tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_oam() -> [u8; 0xA0] {
        [0u8; 0xA0]
    }

    fn blank_vram() -> [u8; 0x2000] {
        [0u8; 0x2000]
    }

    /// Place a sprite at OAM index 0 at position (y=16, x=8) — visible on scanline 0.
    /// (OAM Y is screen Y + 16, OAM X is screen X + 8.)
    fn oam_with_sprite_at(oam_y: u8, oam_x: u8, tile: u8, attrs: u8) -> [u8; 0xA0] {
        let mut oam = blank_oam();
        oam[0] = oam_y;
        oam[1] = oam_x;
        oam[2] = tile;
        oam[3] = attrs;
        oam
    }

    #[test]
    fn test_sprite_on_scanline_is_found() {
        // Given: one sprite at OAM Y=16 (screen Y=0) — visible on scanline 0
        let oam = oam_with_sprite_at(16, 8, 0, 0);
        let lcdc = 0x02u8; // OBJ enabled; 8×8
        // When: scan OAM for scanline 0
        let indices = scan_oam_line(0, &oam, lcdc);
        // Then: sprite 0 is included
        assert!(indices.contains(&0));
    }

    #[test]
    fn test_sprite_above_scanline_is_not_found() {
        // Given: sprite at OAM Y=16 (screen Y=0), 8×8 covers rows 0–7.
        // Scanning scanline 8 (just past the bottom edge) — should NOT be found.
        let oam = oam_with_sprite_at(16, 8, 0, 0);
        let lcdc = 0x02u8;
        let indices = scan_oam_line(8, &oam, lcdc);
        assert!(!indices.contains(&0));
    }

    #[test]
    fn test_sprite_below_scanline_is_not_found() {
        // Given: sprite at OAM Y=17 (screen Y=1), scanning scanline 0
        let oam = oam_with_sprite_at(17, 8, 0, 0);
        let lcdc = 0x02u8;
        let indices = scan_oam_line(0, &oam, lcdc);
        assert!(!indices.contains(&0));
    }

    #[test]
    fn test_oam_scan_limits_to_10_sprites_per_scanline() {
        // Given: 40 sprites all on scanline 0 (OAM Y=16 for all)
        let mut oam = blank_oam();
        for i in 0..40usize {
            oam[i * 4] = 16; // OAM Y = 16 → screen Y = 0
            oam[i * 4 + 1] = (i as u8 + 1) * 2; // distinct X values
            oam[i * 4 + 2] = 0;
            oam[i * 4 + 3] = 0;
        }
        let lcdc = 0x02u8;
        // When: scan OAM for scanline 0
        let indices = scan_oam_line(0, &oam, lcdc);
        // Then: at most 10 sprites returned
        assert!(indices.len() <= 10);
        assert_eq!(indices.len(), 10);
    }

    #[test]
    fn test_8x16_sprite_covers_two_tile_rows() {
        // Given: LCDC with 8×16 sprites; sprite at OAM Y=16 (screen Y=0)
        // In 8×16 mode the sprite covers scanlines 0–15
        let mut oam = blank_oam();
        oam[0] = 16; // OAM Y
        oam[1] = 8;
        oam[2] = 0;
        oam[3] = 0;
        let lcdc = 0x06u8; // OBJ on, 8×16
        // Then: sprite visible on scanlines 0 and 15
        assert!(scan_oam_line(0, &oam, lcdc).contains(&0));
        assert!(scan_oam_line(15, &oam, lcdc).contains(&0));
        // And NOT on scanline 16
        assert!(!scan_oam_line(16, &oam, lcdc).contains(&0));
    }

    #[test]
    fn test_transparent_sprite_pixel_returns_none() {
        // Given: tile 0, row 0 all zeros → colour index 0 = transparent
        let oam = oam_with_sprite_at(16, 8, 0, 0); // screen Y=0, screen X=0
        let vram = blank_vram(); // tile 0 row 0 = 0x00, 0x00 → index 0 everywhere
        let lcdc = 0x02u8;
        let indices = vec![0usize];
        // When: fetch sprite pixel at (x=0, scanline=0)
        let result = fetch_sprite_pixel(0, 0, &indices, &oam, &vram, lcdc);
        // Then: transparent → None
        assert_eq!(result, None);
    }

    #[test]
    fn test_opaque_sprite_pixel_returns_some() {
        // Given: tile 1 row 0 = (low=0xFF, high=0x00) → colour index 1 for all pixels
        let mut vram = blank_vram();
        vram[0x0010] = 0xFF; // tile 1 row 0 low
        vram[0x0011] = 0x00; // tile 1 row 0 high
        // Sprite at screen (X=0, Y=0), tile=1, no palette/flip flags
        let oam = oam_with_sprite_at(16, 8, 1, 0);
        let lcdc = 0x02u8;
        let indices = vec![0usize];
        // When: fetch sprite pixel at (x=0, scanline=0)
        let result = fetch_sprite_pixel(0, 0, &indices, &oam, &vram, lcdc);
        // Then: colour index 1, palette 0, no bg_priority
        assert!(result.is_some());
        let px = result.unwrap();
        assert_eq!(px.colour_index, 1);
        assert_eq!(px.palette, 0);
        assert!(!px.bg_priority);
    }

    /// Build an OAM with two overlapping sprites on scanline 0:
    /// - OAM index 0: OAM_X=28 (screen_x=20), tile 1 → colour index 2 at x=20
    /// - OAM index 1: OAM_X=24 (screen_x=16), tile 2 → colour index 1 at x=20
    ///
    /// DMG rule: lower OAM_X wins regardless of OAM index.
    /// So sprite 1 (OAM_X=24) must win over sprite 0 (OAM_X=28).
    /// Reference: https://gbdev.io/pandocs/OAM.html#drawing-priority
    fn overlapping_oam_and_vram() -> ([u8; 0xA0], [u8; 0x2000]) {
        let mut oam = blank_oam();
        // Sprite 0: OAM index 0, OAM_Y=16 (screen_y=0), OAM_X=28 (screen_x=20), tile=1
        oam[0] = 16;
        oam[1] = 28;
        oam[2] = 1;
        oam[3] = 0;
        // Sprite 1: OAM index 1, OAM_Y=16 (screen_y=0), OAM_X=24 (screen_x=16), tile=2
        oam[4] = 16;
        oam[5] = 24;
        oam[6] = 2;
        oam[7] = 0;

        let mut vram = blank_vram();
        // Tile 1 row 0: colour index 2 (high=1, low=0) at all pixels.
        // colour_index = ((high >> bit) & 1) << 1 | ((low >> bit) & 1)
        // For index 2: high byte = 0xFF, low byte = 0x00
        vram[0x0010] = 0x00; // tile 1 low
        vram[0x0011] = 0xFF; // tile 1 high → colour 2 everywhere
        // Tile 2 row 0: colour index 1 (high=0, low=1) at all pixels.
        vram[0x0020] = 0xFF; // tile 2 low → colour 1 everywhere
        vram[0x0021] = 0x00; // tile 2 high

        (oam, vram)
    }

    /// DMG OBJ priority: lower X-coordinate wins, even if it has a higher OAM index.
    ///
    /// At x=20: sprite 0 (OAM index 0, screen_x=20) and sprite 1 (OAM index 1, screen_x=16)
    /// both cover this column. Sprite 1 has the lower OAM_X (24 < 28) so it must win.
    #[test]
    fn test_lower_oam_x_wins_over_lower_oam_index() {
        let (oam, vram) = overlapping_oam_and_vram();
        let lcdc = 0x02u8;
        let indices = vec![0usize, 1usize];
        // When: fetch sprite pixel at x=20 (both sprites overlap here)
        let result = fetch_sprite_pixel(20, 0, &indices, &oam, &vram, lcdc);
        // Then: sprite 1 (OAM_X=24, lower X) wins → colour index 1
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().colour_index,
            1,
            "sprite with lower OAM_X should win, not lower OAM index"
        );
    }

    /// OAM index tiebreaker: when two sprites share the same X-coordinate,
    /// the one with the lower OAM index wins (matches current + correct behaviour).
    #[test]
    fn test_equal_oam_x_lower_oam_index_wins() {
        let mut oam = blank_oam();
        // Sprite 0 (OAM index 0) and sprite 1 (OAM index 1): same OAM_X=8 (screen_x=0)
        // Tile 1 → colour 2; tile 2 → colour 1
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 1;
        oam[3] = 0;
        oam[4] = 16;
        oam[5] = 8;
        oam[6] = 2;
        oam[7] = 0;
        let mut vram = blank_vram();
        vram[0x0010] = 0x00;
        vram[0x0011] = 0xFF; // tile 1 → colour 2
        vram[0x0020] = 0xFF;
        vram[0x0021] = 0x00; // tile 2 → colour 1
        let lcdc = 0x02u8;
        let indices = vec![0usize, 1usize];
        let result = fetch_sprite_pixel(0, 0, &indices, &oam, &vram, lcdc);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().colour_index,
            2,
            "on equal OAM_X, lower OAM index (sprite 0, colour 2) should win"
        );
    }

    #[test]
    fn test_sprite_palette_bit_selected_from_attr() {
        // Given: sprite with attr bit 4 set → OBP1
        let mut vram = blank_vram();
        vram[0x0010] = 0xFF; // tile 1, opaque
        vram[0x0011] = 0x00;
        let oam = oam_with_sprite_at(16, 8, 1, 0x10); // attr bit 4 = palette 1
        let lcdc = 0x02u8;
        let indices = vec![0usize];
        let result = fetch_sprite_pixel(0, 0, &indices, &oam, &vram, lcdc).unwrap();
        assert_eq!(result.palette, 1);
    }

    // ── OBJ penalty tests ─────────────────────────────────────────────────

    /// Helper: place up to 10 sprites in OAM at the given (oam_y, oam_x) positions.
    /// Returns (oam, sprite_indices).
    fn oam_with_sprites(positions: &[(u8, u8)]) -> ([u8; 0xA0], Vec<usize>) {
        let mut oam = blank_oam();
        let mut indices = Vec::new();
        for (i, &(y, x)) in positions.iter().enumerate() {
            oam[i * 4] = y;
            oam[i * 4 + 1] = x;
            oam[i * 4 + 2] = 0x30 + i as u8; // distinct tile
            oam[i * 4 + 3] = 0;
            indices.push(i);
        }
        (oam, indices)
    }

    /// Helper: create sprites all on the same scanline (Y=$52 = screen Y 66)
    /// with the given OAM X positions.
    fn penalty_sprites(x_positions: &[u8]) -> ([u8; 0xA0], Vec<usize>) {
        let positions: Vec<(u8, u8)> = x_positions.iter().map(|&x| (0x52, x)).collect();
        oam_with_sprites(&positions)
    }

    #[test]
    fn test_obj_penalty_no_sprites_returns_zero() {
        let oam = blank_oam();
        assert_eq!(calculate_obj_penalty(&[], &oam, 0), 0);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x0_is_11_dots() {
        // OAM X=0: exception, always 11 dots (5 tile-wait + 6 flat)
        let (oam, indices) = penalty_sprites(&[0]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 11);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x8_is_11_dots() {
        // OAM X=8 → screen_x=0, bg_x=0, pos_in_tile=0, right=7, wait=5, total=5+6=11
        let (oam, indices) = penalty_sprites(&[8]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 11);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x5_is_6_dots() {
        // OAM X=5 → screen_x=-3, bg_x=-3, pos_in_tile=5, right=2, wait=0, total=6
        let (oam, indices) = penalty_sprites(&[5]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 6);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x4_is_7_dots() {
        // OAM X=4 → screen_x=-4, bg_x=-4, pos_in_tile=4, right=3, wait=1, total=7
        let (oam, indices) = penalty_sprites(&[4]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 7);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x167_is_6_dots() {
        // OAM X=167 → screen_x=159, bg_x=159, pos_in_tile=7, right=0, wait=0, total=6
        let (oam, indices) = penalty_sprites(&[167]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 6);
    }

    #[test]
    fn test_obj_penalty_sprite_at_x168_is_offscreen_no_penalty() {
        // OAM X ≥ 168: off-screen right, no penalty
        let (oam, indices) = penalty_sprites(&[168]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 0);
    }

    #[test]
    fn test_obj_penalty_two_sprites_at_x0_share_tile() {
        // Two sprites at X=0: first gets 11, second shares tile → only flat 6
        // Total: 11 + 6 = 17
        let (oam, indices) = penalty_sprites(&[0, 0]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 17);
    }

    #[test]
    fn test_obj_penalty_ten_sprites_at_x0() {
        // 10 sprites at X=0: 11 + 9×6 = 65
        let (oam, indices) = penalty_sprites(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 65);
    }

    #[test]
    fn test_obj_penalty_ten_sprites_spread_across_tiles() {
        // 10 sprites 8 apart at X=0,8,16,...,72: each on a different tile
        // X=0: 11, X=8..72: each 11 (new tile, pos=0, wait=5, +6 flat)
        // Total: 11 × 10 = 110
        let (oam, indices) = penalty_sprites(&[0, 8, 16, 24, 32, 40, 48, 56, 64, 72]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 110);
    }

    #[test]
    fn test_obj_penalty_reverse_oam_order_same_result() {
        // Sprites at X=72,64,...,0 in OAM: sorted by X internally → same result
        let (oam, indices) = penalty_sprites(&[72, 64, 56, 48, 40, 32, 24, 16, 8, 0]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 110);
    }

    #[test]
    fn test_obj_penalty_two_groups_different_tiles() {
        // 5 at X=0, 5 at X=160: two separate tile groups
        // Group X=0: 11 + 4×6 = 35
        // Group X=160: screen_x=152, pos=0, wait=5 → 11 + 4×6 = 35
        // Total: 70
        let (oam, indices) = penalty_sprites(&[0, 0, 0, 0, 0, 160, 160, 160, 160, 160]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 70);
    }

    #[test]
    fn test_obj_penalty_scx_shifts_tile_boundaries() {
        // With SCX=4, sprite at OAM X=8 (screen_x=0):
        // bg_x = 0 + 4 = 4, pos_in_tile = 4, right = 3, wait = 1, total = 7
        let (oam, indices) = penalty_sprites(&[8]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 4), 7);
    }

    #[test]
    fn test_obj_penalty_x0_exception_ignores_scx() {
        // OAM X=0 always 11 dots regardless of SCX
        let (oam, indices) = penalty_sprites(&[0]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 3), 11);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 7), 11);
    }
}
