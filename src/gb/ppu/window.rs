use crate::gb::ppu::background::BgPixelCgb;

/// Fetch the DMG window layer colour index (0–3) for a given screen pixel.
///
/// Returns `None` if the window does not cover pixel `(x, scanline)`.
///
/// # Arguments
/// * `x`           — Screen X coordinate (0–159)
/// * `scanline`    — Current scanline (LY, 0–143)
/// * `vram`        — Full 8 KiB VRAM ($8000–$9FFF)
/// * `lcdc`        — Current LCDC register value
/// * `wx`          — Window X (minus 7); window starts at screen column `wx - 7`
/// * `wy`          — Window Y; window starts at scanline `wy`
/// * `window_line` — Internal window line counter (incremented each rendered scanline
///   where the window was active)
pub fn fetch_window_pixel(
    x: u32,
    scanline: u8,
    vram: &[u8; 0x2000],
    lcdc: u8,
    wx: u8,
    wy: u8,
    window_line: u8,
) -> Option<u8> {
    // Window must be enabled (LCDC bit 5).
    if lcdc & 0x20 == 0 {
        return None;
    }

    // Window starts at scanline wy.
    if scanline < wy {
        return None;
    }

    // Window starts at screen column (wx - 7); wx < 7 means full-screen left edge.
    let win_x_start = wx.saturating_sub(7);
    if x < win_x_start as u32 {
        return None;
    }

    // Position within the window tile map.
    let win_x = (x as u8).wrapping_sub(win_x_start);
    let win_y = window_line;

    // Window tile map: LCDC bit 6 (0 = $9800, 1 = $9C00).
    let map_base: usize = if lcdc & 0x40 != 0 { 0x1C00 } else { 0x1800 };
    let tile_col = (win_x / 8) as usize;
    let tile_row = (win_y / 8) as usize;
    let tile_index_raw = vram[map_base + tile_row * 32 + tile_col];

    // Tile data base: same select as BG (LCDC bit 4).
    let tile_data_start: usize = if lcdc & 0x10 != 0 {
        (tile_index_raw as usize) * 16
    } else {
        (0x1000i32 + (tile_index_raw as i8 as i32) * 16) as usize
    };

    let row_in_tile = (win_y % 8) as usize;
    let bit = 7 - (win_x % 8);

    let addr = tile_data_start + row_in_tile * 2;
    let low = vram[addr];
    let high = vram[addr + 1];
    Some(((high >> bit) & 1) << 1 | ((low >> bit) & 1))
}

/// Fetch the CGB window pixel at `(x, scanline)` with full tile attribute info.
///
/// Returns `None` if the window does not cover the pixel.
/// The window uses the same tile map indexing as the background, with tile
/// attributes fetched from VRAM bank 1 at the same map address.
#[allow(clippy::too_many_arguments)]
pub fn fetch_window_pixel_cgb(
    x: u32,
    scanline: u8,
    vram: &[u8; 0x2000],
    vram_bank1: &[u8; 0x2000],
    lcdc: u8,
    wx: u8,
    wy: u8,
    window_line: u8,
) -> Option<BgPixelCgb> {
    if lcdc & 0x20 == 0 {
        return None;
    }
    if scanline < wy {
        return None;
    }
    let win_x_start = i16::from(wx) - 7;
    let visible_win_x_start = win_x_start.max(0) as u32;
    if x < visible_win_x_start {
        return None;
    }

    let win_x = ((x as i16) - win_x_start) as u8;
    let win_y = window_line;

    // Window tile map: LCDC bit 6 (0 = $9800, 1 = $9C00).
    let map_base: usize = if lcdc & 0x40 != 0 { 0x1C00 } else { 0x1800 };
    let tile_col = (win_x / 8) as usize;
    let tile_row = (win_y / 8) as usize;
    let map_offset = map_base + tile_row * 32 + tile_col;

    let tile_index_raw = vram[map_offset];
    let attrs = vram_bank1[map_offset];

    let palette_num = attrs & 0x07;
    let tile_vram_bank = (attrs >> 3) & 0x01;
    let x_flip = attrs & 0x20 != 0;
    let y_flip = attrs & 0x40 != 0;
    let bg_priority = attrs & 0x80 != 0;

    let tile_data_start: usize = if lcdc & 0x10 != 0 {
        (tile_index_raw as usize) * 16
    } else {
        (0x1000i32 + (tile_index_raw as i8 as i32) * 16) as usize
    };

    let mut row_in_tile = (win_y % 8) as usize;
    if y_flip {
        row_in_tile = 7 - row_in_tile;
    }

    let pixel_in_tile = win_x % 8;
    let bit = if x_flip {
        pixel_in_tile
    } else {
        7 - pixel_in_tile
    };

    let tile_vram = if tile_vram_bank != 0 {
        vram_bank1
    } else {
        vram
    };
    let addr = tile_data_start + row_in_tile * 2;
    let low = tile_vram[addr];
    let high = tile_vram[addr + 1];
    let colour_index = ((high >> bit) & 1) << 1 | ((low >> bit) & 1);

    Some(BgPixelCgb {
        colour_index,
        palette_num,
        vram_bank: tile_vram_bank,
        bg_priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_vram() -> [u8; 0x2000] {
        [0u8; 0x2000]
    }

    #[test]
    fn test_window_returns_none_when_disabled_in_lcdc() {
        // Given: LCDC with window disabled (bit 5 = 0)
        let vram = blank_vram();
        let lcdc = 0x81u8; // LCD on, window OFF
        // When: fetch window pixel at (0, 0) with wy=0, wx=7 (leftmost)
        let result = fetch_window_pixel(0, 0, &vram, lcdc, 7, 0, 0);
        // Then: None — window is not rendered
        assert_eq!(result, None);
    }

    #[test]
    fn test_window_returns_none_above_wy() {
        // Given: LCDC with window enabled, WY=10
        let vram = blank_vram();
        let lcdc = 0xA1u8; // LCD on, window ON, tile data $8000
        // When: fetch pixel at scanline 9 (above WY)
        let result = fetch_window_pixel(0, 9, &vram, lcdc, 7, 10, 0);
        // Then: None — above window Y start
        assert_eq!(result, None);
    }

    #[test]
    fn test_window_returns_none_left_of_wx() {
        // Given: LCDC with window enabled, WX=14 (screen X starts at 7)
        let vram = blank_vram();
        let lcdc = 0xA1u8;
        // When: fetch pixel at x=6 (left of WX-7 = 7)
        let result = fetch_window_pixel(6, 0, &vram, lcdc, 14, 0, 0);
        // Then: None — left of window X start
        assert_eq!(result, None);
    }

    #[test]
    fn test_window_returns_colour_index_for_pixel_inside_window() {
        // Given: window enabled, WY=0, WX=7 (full screen), blank tile data → index 0
        let vram = blank_vram();
        let lcdc = 0xA1u8; // window tile map $9800 (bit 6=0)
        // When: fetch pixel (0, 0) — inside window with window_line=0
        let result = fetch_window_pixel(0, 0, &vram, lcdc, 7, 0, 0);
        // Then: Some(0) — blank tile
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_window_tile_data_colour_bits_from_vram() {
        // Given: window enabled; tile 1 in window map at entry (0,0), row 0 = colour 3
        let mut vram = blank_vram();
        // Tile 1 row 0: low=0xFF, high=0xFF → colour 3
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0xFF;
        // Window tile map at $9800 (bit 6=0 in LCDC), entry (0,0) = vram[0x1800] = 1
        vram[0x1800] = 0x01;
        // LCDC 0xB1 = 1011_0001: LCD on, window ON (bit 5), tile data $8000 (bit 4), BG/win enabled (bit 0)
        let lcdc = 0xB1u8;
        // When: fetch pixel (0, 0) inside window, window_line=0
        let result = fetch_window_pixel(0, 0, &vram, lcdc, 7, 0, 0);
        // Then: colour index 3
        assert_eq!(result, Some(3));
    }

    // ── CGB window pixel tests ────────────────────────────────────────────────

    #[test]
    fn test_cgb_window_palette_num_extracted_from_bank1_attrs() {
        // Given: window tile map entry (0,0) in bank 1 has palette_num=3 (bits 0-2)
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x03; // palette 3
        // LCDC: window enabled (bit5), tile data $8000 (bit4)
        let lcdc = 0x30u8;
        let result = fetch_window_pixel_cgb(0, 0, &vram, &bank1, lcdc, 7, 0, 0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().palette_num, 3);
    }

    #[test]
    fn test_cgb_window_bg_priority_extracted_from_bank1_attrs() {
        // Given: attrs with bit 7 set
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x80; // bg_priority bit
        let lcdc = 0x30u8;
        let result = fetch_window_pixel_cgb(0, 0, &vram, &bank1, lcdc, 7, 0, 0);
        assert!(result.unwrap().bg_priority);
    }

    #[test]
    fn test_cgb_window_wx_less_than_7_shifts_tilemap_correctly() {
        // WX=0 places the window 7 pixels off-screen to the left.
        // Screen x=0 should correspond to tile-map pixel 7 (tile col 0, pixel_in_tile=7).
        // Tile 0 row 0: low=0x01 → only bit 0 has colour 1 (tile pixel 7, rightmost).
        // With signed WX math: win_x = 0 - (0-7) = 7 → pixel_in_tile=7 → bit=0 → colour=1.
        let mut vram = blank_vram();
        let bank1 = blank_vram();
        vram[0x1800] = 0; // tile index 0
        vram[0x0000] = 0x01; // low byte: only bit 0 set
        vram[0x0001] = 0x00; // high byte
        let lcdc = 0x30u8; // window on (bit5), tile data $8000 (bit4)
        let result = fetch_window_pixel_cgb(0, 0, &vram, &bank1, lcdc, 0, 0, 0);
        assert!(
            result.is_some(),
            "window should be visible at x=0 when WX=0"
        );
        assert_eq!(
            result.unwrap().colour_index,
            1,
            "screen x=0 with WX=0 must map to tile pixel 7 (colour 1)"
        );
    }

    #[test]
    fn test_cgb_window_tile_data_read_from_vram_bank1_when_attr_bit3_set() {
        // Given: tile index 0 in map; attrs with VRAM bank bit (bit 3) set
        // Tile 0 in bank 0: all zeros; tile 0 in bank 1 row 0: low=0xFF → colour 1
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x08; // VRAM bank bit
        bank1[0x0000] = 0xFF; // tile 0 row 0 in bank 1
        bank1[0x0001] = 0x00;
        let lcdc = 0x30u8;
        let result = fetch_window_pixel_cgb(0, 0, &vram, &bank1, lcdc, 7, 0, 0);
        assert!(result.is_some());
        let px = result.unwrap();
        assert_eq!(px.vram_bank, 1);
        // Leftmost pixel: bit=7 of low byte 0xFF → (0xFF>>7)&1 = 1 → colour=1
        assert_eq!(px.colour_index, 1);
    }
}
