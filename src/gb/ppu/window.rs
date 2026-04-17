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
    let win_x_start = wx.saturating_sub(7);
    if x < win_x_start as u32 {
        return None;
    }

    let win_x = (x as u8).wrapping_sub(win_x_start);
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
}
