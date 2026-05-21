use serde::{Deserialize, Serialize};

/// Colour info for a single sprite pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// Colour index (1–3; 0 is transparent and will never appear here).
    pub colour_index: u8,
    /// DMG palette selection: 0 = OBP0, 1 = OBP1.
    /// In CGB mode this is unused; use `cgb_palette` instead.
    pub palette: u8,
    /// CGB OBJ palette number (0–7, from OAM attribute bits 0–2).
    /// Always 0 in DMG mode.
    pub cgb_palette: u8,
    /// Priority: if true the sprite is drawn behind BG colours 1–3.
    pub bg_priority: bool,
}

/// One OBJ fetch stall in the visible pixel stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ObjPenaltyEvent {
    pub x: u8,
    pub dots: u16,
    /// Leading stall dots where the BG fetcher keeps advancing before the
    /// visible pixel stream resumes.
    #[serde(default)]
    pub bg_fetch_wait_dots: u16,
}

impl ObjPenaltyEvent {
    fn new(screen_x: i16, dots: u16, bg_fetch_wait_dots: u16) -> Self {
        Self {
            x: screen_x.clamp(0, 159) as u8,
            dots,
            bg_fetch_wait_dots,
        }
    }
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
    let mut result = Vec::new();
    scan_oam_line_into(scanline, oam, lcdc, &mut result);
    result
}

/// Collect up to 10 OAM entry indices into an existing buffer.
///
/// This is equivalent to [`scan_oam_line`] but lets per-dot renderers reuse
/// allocation across scanlines.
pub fn scan_oam_line_into(scanline: u8, oam: &[u8; 0xA0], lcdc: u8, result: &mut Vec<usize>) {
    let height: u8 = if lcdc & 0x04 != 0 { 16 } else { 8 };
    result.clear();
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
    fetch_sprite_pixel_with_lcdc_samples(x, scanline, sprite_indices, oam, vram, lcdc, lcdc)
}

/// Fetch the highest-priority visible DMG sprite pixel with independently sampled
/// LCDC values for the low and high tile-data bytes.
pub(super) fn fetch_sprite_pixel_with_lcdc_samples(
    x: u32,
    scanline: u8,
    sprite_indices: &[usize],
    oam: &[u8; 0xA0],
    vram: &[u8; 0x2000],
    low_lcdc: u8,
    high_lcdc: u8,
) -> Option<SpritePixel> {
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
        let screen_x = oam_x as i16 - 8;

        // Skip sprites that don't cover column x.
        let screen_x_offset = x as i16 - screen_x;
        if !(0..8).contains(&screen_x_offset) {
            continue;
        }

        let y_flip = attrs & 0x40 != 0;
        let x_flip = attrs & 0x20 != 0;
        let palette = (attrs >> 4) & 1;
        let bg_priority = attrs & 0x80 != 0;

        let mut pixel_x = screen_x_offset as u8;
        if x_flip {
            pixel_x = 7 - pixel_x;
        }

        let low_addr = sprite_tile_row_addr(scanline, screen_y, tile_num, y_flip, low_lcdc);
        let high_addr = sprite_tile_row_addr(scanline, screen_y, tile_num, y_flip, high_lcdc);
        let low = vram[low_addr];
        let high = vram[high_addr + 1];
        let bit = 7 - pixel_x;
        let colour_index = ((high >> bit) & 1) << 1 | ((low >> bit) & 1);

        if colour_index == 0 {
            continue; // colour 0 is transparent
        }

        return Some(SpritePixel {
            colour_index,
            palette,
            cgb_palette: 0,
            bg_priority,
        });
    }
    None
}

pub(super) fn sprite_tile_row_high_byte(
    scanline: u8,
    oam_index: usize,
    oam: &[u8; 0xA0],
    vram: &[u8; 0x2000],
    lcdc: u8,
) -> u8 {
    let oam_y = oam[oam_index * 4];
    let tile_num = oam[oam_index * 4 + 2];
    let attrs = oam[oam_index * 4 + 3];
    let screen_y = oam_y.wrapping_sub(16);
    let y_flip = attrs & 0x40 != 0;
    let addr = sprite_tile_row_addr(scanline, screen_y, tile_num, y_flip, lcdc);
    vram[addr + 1]
}

fn sprite_tile_row_addr(scanline: u8, screen_y: u8, tile_num: u8, y_flip: bool, lcdc: u8) -> usize {
    let height: u8 = if lcdc & 0x04 != 0 { 16 } else { 8 };
    let row_mask = height - 1;
    let mut row = scanline.wrapping_sub(screen_y) & row_mask;
    if y_flip {
        row ^= row_mask;
    }

    let tile_index = if height == 16 {
        let base_tile = tile_num & 0xFE;
        if row < 8 {
            base_tile
        } else {
            row -= 8;
            base_tile | 0x01
        }
    } else {
        tile_num
    };

    tile_index as usize * 16 + row as usize * 2
}

/// Fetch the highest-priority visible sprite pixel in CGB mode at screen position `x`.
///
/// When `dmg_priority_mode` is `false` (CGB default), priority is determined by OAM
/// order (earlier entry wins). When `true`, X-coordinate priority is used as in DMG.
///
/// CGB OAM attributes (byte 3):
/// - Bits 0–2: OBJ palette number (0–7)
/// - Bit 3: Tile VRAM bank (0=bank0, 1=bank1)
/// - Bit 5: H-flip
/// - Bit 6: V-flip
/// - Bit 7: OBJ-to-BG priority
#[allow(clippy::too_many_arguments)]
pub fn fetch_sprite_pixel_cgb(
    x: u32,
    scanline: u8,
    sprite_indices: &[usize],
    oam: &[u8; 0xA0],
    vram: &[u8; 0x2000],
    vram_bank1: &[u8; 0x2000],
    lcdc: u8,
    dmg_priority_mode: bool,
) -> Option<SpritePixel> {
    let height: u8 = if lcdc & 0x04 != 0 { 16 } else { 8 };

    // In CGB mode (dmg_priority_mode=false) sprites are prioritized by OAM order
    // (scan_oam_line already returns them in OAM index order).
    // In DMG compatibility mode (dmg_priority_mode=true) sort by X-coord.
    let mut sorted = [0usize; 10];
    let mut count = 0usize;
    for &i in sprite_indices.iter().take(10) {
        sorted[count] = i;
        count += 1;
    }
    if dmg_priority_mode {
        sorted[..count].sort_by_key(|&i| (oam[i * 4 + 1], i));
    }

    for &i in &sorted[..count] {
        let oam_y = oam[i * 4];
        let oam_x = oam[i * 4 + 1];
        let tile_num = oam[i * 4 + 2];
        let attrs = oam[i * 4 + 3];

        let screen_y = oam_y.wrapping_sub(16);
        let screen_x = oam_x as i16 - 8;

        let screen_x_offset = x as i16 - screen_x;
        if !(0..8).contains(&screen_x_offset) {
            continue;
        }

        let y_flip = attrs & 0x40 != 0;
        let x_flip = attrs & 0x20 != 0;
        let cgb_palette = attrs & 0x07;
        let dmg_palette = (attrs >> 4) & 1; // bit 4: DMG palette (0=OBP0, 1=OBP1)
        let tile_vram_bank = (attrs >> 3) & 0x01;
        let bg_priority = attrs & 0x80 != 0;

        let mut row = if dmg_priority_mode {
            match scanline.checked_sub(screen_y) {
                Some(row) if row < height => row as usize,
                _ => continue,
            }
        } else {
            (scanline.wrapping_sub(screen_y) & (height - 1)) as usize
        };
        if y_flip {
            row = (height as usize - 1) - row;
        }

        let mut pixel_x = screen_x_offset as u8;
        if x_flip {
            pixel_x = 7 - pixel_x;
        }

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

        let tile_vram = if tile_vram_bank != 0 {
            vram_bank1
        } else {
            vram
        };
        let tile_addr = tile_index * 16;
        let low = tile_vram[tile_addr + row * 2];
        let high = tile_vram[tile_addr + row * 2 + 1];
        let bit = 7 - pixel_x;
        let colour_index = ((high >> bit) & 1) << 1 | ((low >> bit) & 1);

        if colour_index == 0 {
            continue;
        }

        return Some(SpritePixel {
            colour_index,
            palette: dmg_palette,
            cgb_palette,
            bg_priority,
        });
    }
    None
}

/// Flat dot cost per visible sprite (OBJ tile fetch).
const OBJ_FETCH_DOTS: u16 = 6;

/// OAM X for an OBJ whose leftmost pixel is at visible screen X=0.
const FIRST_VISIBLE_OAM_X: u8 = 8;

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
    let mut events = Vec::new();
    schedule_obj_penalties(sprite_indices, oam, scx, &mut events);
    events.iter().map(|event| event.dots).sum()
}

/// Build per-pixel OBJ fetch stall events for the current scanline.
///
/// Each event's `x` is the first visible pixel coordinate delayed by that
/// object's fetch. Off-left sprites are clamped to x=0; off-right sprites do
/// not generate events.
pub(super) fn schedule_obj_penalties(
    sprite_indices: &[usize],
    oam: &[u8; 0xA0],
    scx: u8,
    events: &mut Vec<ObjPenaltyEvent>,
) {
    schedule_obj_penalties_with_bg_fetch_wait_extra(sprite_indices, oam, scx, 0, events);
}

/// Build OBJ stall events, adding model-specific BG fetcher advance dots to
/// on-screen sprites without changing the total OBJ penalty.
pub(super) fn schedule_obj_penalties_with_bg_fetch_wait_extra(
    sprite_indices: &[usize],
    oam: &[u8; 0xA0],
    scx: u8,
    bg_fetch_wait_extra_dots: u16,
    events: &mut Vec<ObjPenaltyEvent>,
) {
    events.clear();
    debug_assert!(
        sprite_indices.len() <= 10,
        "OAM scan is capped at 10 sprites per scanline"
    );
    if sprite_indices.is_empty() {
        return;
    }

    // Process sprites left-to-right (ascending OAM X, then OAM index).
    let mut sorted: Vec<(usize, u8)> = sprite_indices
        .iter()
        .map(|&i| (i, oam[i * 4 + 1]))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // First sprite on a given BG tile pays the tile-wait; subsequent sprites share it.
    let mut seen_tiles: [i16; 10] = [i16::MIN; 10];
    let mut seen_count: usize = 0;

    for &(_, oam_x) in &sorted {
        if oam_x >= OAM_X_OFFSCREEN {
            continue;
        }

        let screen_x = sprite_screen_x(oam_x);
        let bg_x = sprite_bg_x(oam_x, scx);
        let tile_id = bg_x.div_euclid(BG_TILE_WIDTH);

        let mut event_dots = OBJ_FETCH_DOTS;
        let mut bg_fetch_wait_dots = 0;
        if !seen_tiles[..seen_count].contains(&tile_id) {
            let tile_wait_dots = tile_wait_penalty(oam_x, bg_x);
            event_dots += tile_wait_dots;
            if oam_x >= FIRST_VISIBLE_OAM_X {
                bg_fetch_wait_dots = (tile_wait_dots + bg_fetch_wait_extra_dots).min(event_dots);
            }
            if seen_count < seen_tiles.len() {
                seen_tiles[seen_count] = tile_id;
                seen_count += 1;
            }
        }

        push_obj_penalty_event(
            events,
            ObjPenaltyEvent::new(screen_x, event_dots, bg_fetch_wait_dots),
        );
    }
}

fn push_obj_penalty_event(events: &mut Vec<ObjPenaltyEvent>, event: ObjPenaltyEvent) {
    if let Some(last) = events.last_mut()
        && last.x == event.x
    {
        last.dots += event.dots;
        last.bg_fetch_wait_dots += event.bg_fetch_wait_dots;
    } else {
        events.push(event);
    }
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

fn sprite_screen_x(oam_x: u8) -> i16 {
    oam_x as i16 - 8
}

fn sprite_bg_x(oam_x: u8, scx: u8) -> i16 {
    sprite_screen_x(oam_x) + scx as i16
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
    fn dmg_y_flipped_sprite_selected_as_8x16_does_not_panic_if_lcdc_changes_to_8x8() {
        let oam = oam_with_sprite_at(16, 8, 0, 0x40);
        let vram = blank_vram();
        let indices = scan_oam_line(15, &oam, 0x06);

        let result =
            std::panic::catch_unwind(|| fetch_sprite_pixel(0, 15, &indices, &oam, &vram, 0x02));

        assert!(
            result.is_ok(),
            "fetching a Mode-2-selected 8x16 sprite must tolerate live LCDC.2 changing to 8x8"
        );
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
        let mut events = Vec::new();
        schedule_obj_penalties(&indices, &oam, 0, &mut events);
        assert_eq!(
            events,
            vec![ObjPenaltyEvent {
                x: 0,
                dots: 11,
                bg_fetch_wait_dots: 5
            }]
        );
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 11);
    }

    #[test]
    fn test_obj_penalty_extra_bg_fetch_wait_does_not_change_total_penalty() {
        let (oam, indices) = penalty_sprites(&[8]);
        let mut events = Vec::new();
        schedule_obj_penalties_with_bg_fetch_wait_extra(&indices, &oam, 0, 1, &mut events);

        assert_eq!(
            events,
            vec![ObjPenaltyEvent {
                x: 0,
                dots: 11,
                bg_fetch_wait_dots: 6
            }]
        );
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 11);
    }

    #[test]
    fn test_obj_penalty_event_is_placed_at_visible_sprite_x() {
        // OAM X=9 → screen_x=1. The same 10-dot penalty applies, but it must
        // stall pixel 1 rather than the start of the scanline.
        let (oam, indices) = penalty_sprites(&[9]);
        let mut events = Vec::new();
        schedule_obj_penalties(&indices, &oam, 0, &mut events);
        assert_eq!(
            events,
            vec![ObjPenaltyEvent {
                x: 1,
                dots: 10,
                bg_fetch_wait_dots: 4
            }]
        );
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 10);
    }

    #[test]
    fn test_obj_penalty_event_clamps_off_left_sprite_to_x0() {
        // OAM X=5 → screen_x=-3. The sprite is partly off-left, so its fetch
        // stalls before the first visible pixel.
        let (oam, indices) = penalty_sprites(&[5]);
        let mut events = Vec::new();
        schedule_obj_penalties(&indices, &oam, 0, &mut events);
        assert_eq!(
            events,
            vec![ObjPenaltyEvent {
                x: 0,
                dots: 6,
                bg_fetch_wait_dots: 0
            }]
        );
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x5_is_6_dots() {
        // OAM X=5 → screen_x=-3, bg_x=-3, pos_in_tile=5, right=2, wait=0, total=6
        let (oam, indices) = penalty_sprites(&[5]);
        assert_eq!(calculate_obj_penalty(&indices, &oam, 0), 6);
    }

    #[test]
    fn test_obj_penalty_single_sprite_at_x4_is_7_dots() {
        // OAM X=4 -> screen_x=-4, bg_x=-4, pos_in_tile=4, wait=1, total=7.
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

    // ── CGB sprite pixel tests ────────────────────────────────────────────────

    #[test]
    fn test_cgb_sprite_palette_bits_extracted_from_attrs() {
        // Sprite at (screen_x=0, screen_y=0): oam_x=8, oam_y=16
        // attrs = 0x05 → CGB palette 5 (bits 0-2)
        let mut oam = blank_oam();
        oam[0] = 16; // oam_y
        oam[1] = 8; // oam_x
        oam[2] = 0; // tile 0
        oam[3] = 0x05; // palette 5
        let mut vram = blank_vram();
        vram[0x0000] = 0xFF; // tile 0, row 0: colour 1 (all pixels non-transparent)
        let bank1 = blank_vram();
        let lcdc = 0x02u8; // sprites enabled (bit 1)
        let indices = [0usize];
        let result = fetch_sprite_pixel_cgb(0, 0, &indices, &oam, &vram, &bank1, lcdc, false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().cgb_palette, 5);
    }

    #[test]
    fn cgb_y_flipped_sprite_selected_as_8x16_does_not_panic_if_lcdc_changes_to_8x8() {
        let oam = oam_with_sprite_at(16, 8, 0, 0x40);
        let vram = blank_vram();
        let bank1 = blank_vram();
        let indices = scan_oam_line(15, &oam, 0x06);

        let result = std::panic::catch_unwind(|| {
            fetch_sprite_pixel_cgb(0, 15, &indices, &oam, &vram, &bank1, 0x02, true)
        });

        assert!(
            result.is_ok(),
            "CGB DMG-compat sprite fetch must tolerate live LCDC.2 changing after OAM scan"
        );
    }

    #[test]
    fn native_cgb_sprite_fetch_keeps_live_lcdc_row_wrapping_when_lcdc_changes_to_8x8() {
        let oam = oam_with_sprite_at(16, 8, 0, 0x40);
        let mut vram = blank_vram();
        vram[0x0000] = 0x80;
        let bank1 = blank_vram();
        let indices = scan_oam_line(15, &oam, 0x06);

        let result = fetch_sprite_pixel_cgb(0, 15, &indices, &oam, &vram, &bank1, 0x02, false);

        assert_eq!(
            result.map(|pixel| pixel.colour_index),
            Some(1),
            "native CGB sprite fetch should keep the pre-existing live-LCDC row wrapping behavior"
        );
    }

    #[test]
    fn test_cgb_sprite_tile_data_read_from_vram_bank1_when_attr_bit3_set() {
        // attrs bit 3 = VRAM bank 1. Tile 0 in bank 0 all zeros; tile 0 in bank 1 has data.
        let mut oam = blank_oam();
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 0;
        oam[3] = 0x08; // VRAM bank bit (bit 3)
        let vram = blank_vram(); // bank 0: all zero (transparent)
        let mut bank1 = blank_vram();
        bank1[0x0000] = 0xFF; // tile 0, row 0 in bank 1: colour 1
        let lcdc = 0x02u8;
        let indices = [0usize];
        // Without bank1 flag this would return None (transparent); must return Some
        let result = fetch_sprite_pixel_cgb(0, 0, &indices, &oam, &vram, &bank1, lcdc, false);
        assert!(result.is_some(), "tile data should come from VRAM bank 1");
        assert_eq!(result.unwrap().colour_index, 1);
    }

    #[test]
    fn test_cgb_sprite_oam_order_priority_when_dmg_mode_false() {
        // Two sprites overlapping at x=1.
        // Sprite 0: oam_x=9 (screen_x=1), tile 0, colour 1
        // Sprite 1: oam_x=8 (screen_x=0, covers x=0..7), tile 1, colour 3
        // In CGB (OAM order) mode: sprite 0 wins (lower OAM index).
        let mut oam = blank_oam();
        // Sprite 0 at screen_x=1
        oam[0] = 16;
        oam[1] = 9;
        oam[2] = 0;
        oam[3] = 0x01; // palette 1
        // Sprite 1 at screen_x=0
        oam[4] = 16;
        oam[5] = 8;
        oam[6] = 1;
        oam[7] = 0x02; // palette 2
        let mut vram = blank_vram();
        vram[0x0000] = 0xFF; // tile 0 row 0: colour 1
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0xFF; // tile 1 row 0: colour 3
        let bank1 = blank_vram();
        let lcdc = 0x02u8;
        let indices = [0usize, 1usize];
        let result = fetch_sprite_pixel_cgb(1, 0, &indices, &oam, &vram, &bank1, lcdc, false);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().cgb_palette,
            1,
            "OAM-order: sprite 0 (palette 1) should win"
        );
    }

    #[test]
    fn test_cgb_sprite_dmg_xcoord_priority_when_dmg_mode_true() {
        // Same two sprites; in DMG-compat (dmg_priority_mode=true) X-coord wins.
        // Sprite 1 has lower oam_x=8 < oam_x=9, so it should win.
        let mut oam = blank_oam();
        oam[0] = 16;
        oam[1] = 9;
        oam[2] = 0;
        oam[3] = 0x01; // palette 1
        oam[4] = 16;
        oam[5] = 8;
        oam[6] = 1;
        oam[7] = 0x02; // palette 2
        let mut vram = blank_vram();
        vram[0x0000] = 0xFF;
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0xFF;
        let bank1 = blank_vram();
        let lcdc = 0x02u8;
        let indices = [0usize, 1usize];
        let result = fetch_sprite_pixel_cgb(1, 0, &indices, &oam, &vram, &bank1, lcdc, true);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().cgb_palette,
            2,
            "X-coord priority: sprite 1 (palette 2, lower X) should win"
        );
    }
}
