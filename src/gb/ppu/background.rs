/// Fetch the DMG background colour index (0–3) for a given screen pixel.
///
/// # Arguments
/// * `x`        — Screen X coordinate (0–159)
/// * `scanline` — Current scanline (LY, 0–143)
/// * `vram`     — Full 8 KiB VRAM ($8000–$9FFF)
/// * `lcdc`     — Current LCDC register value
/// * `scx`      — Horizontal scroll (SCX)
/// * `scy`      — Vertical scroll (SCY)
pub fn fetch_bg_pixel(x: u32, scanline: u8, vram: &[u8; 0x2000], lcdc: u8, scx: u8, scy: u8) -> u8 {
    // Wrap BG position within 256×256 tile map space.
    let bg_x = scx.wrapping_add(x as u8);
    let bg_y = scy.wrapping_add(scanline);

    // BG tile map base: LCDC bit 3 (0 = $9800, 1 = $9C00).
    let map_base: usize = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
    let tile_col = (bg_x / 8) as usize;
    let tile_row = (bg_y / 8) as usize;
    let tile_index_raw = vram[map_base + tile_row * 32 + tile_col];

    // Tile data base address within VRAM.
    // LCDC bit 4: 1 = $8000 unsigned (tile 0 at vram[0x0000]),
    //             0 = $8800 signed  (tile 0 at vram[0x1000]).
    let tile_data_start: usize = if lcdc & 0x10 != 0 {
        (tile_index_raw as usize) * 16
    } else {
        (0x1000i32 + (tile_index_raw as i8 as i32) * 16) as usize
    };

    let row_in_tile = (bg_y % 8) as usize;
    let bit = 7 - (bg_x % 8); // pixel 0 is at bit 7

    let addr = tile_data_start + row_in_tile * 2;
    let low = vram[addr];
    let high = vram[addr + 1];
    ((high >> bit) & 1) << 1 | ((low >> bit) & 1)
}

/// CGB background pixel with full tile attribute information.
#[derive(Debug, Clone, Copy)]
pub struct BgPixelCgb {
    /// Colour index (0–3) from the tile data.
    pub colour_index: u8,
    /// BG palette number (0–7) from VRAM bank 1 tile attribute bits 0–2.
    pub palette_num: u8,
    /// Whether the tile data comes from VRAM bank 1 (attribute bit 3).
    pub vram_bank: u8,
    /// Background-to-OAM priority flag (attribute bit 7).
    /// When true and the BG colour index is non-zero, the BG pixel is drawn
    /// on top of OBJ pixels (in master-priority mode).
    pub bg_priority: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CgbPixelFetch {
    pub x: u32,
    pub scanline: u8,
    pub scx: u8,
    pub fine_scx: u8,
    pub scy: u8,
    pub lcdc: u8,
}

/// Fetch a CGB background pixel and its tile attributes for a given screen pixel.
///
/// Tile data is fetched from VRAM bank 0 or bank 1 according to the attribute's
/// VRAM bank bit.  Tile attributes are always read from VRAM bank 1 at the same
/// tile map address as the tile index in bank 0.
///
/// # Arguments
/// * `x`           — Screen X coordinate (0–159)
/// * `scanline`    — Current scanline (LY, 0–143)
/// * `vram`        — VRAM bank 0 ($8000–$9FFF)
/// * `vram_bank1`  — VRAM bank 1
/// * `lcdc`        — Current LCDC register value
/// * `scx`         — Horizontal scroll (SCX)
/// * `scy`         — Vertical scroll (SCY)
pub fn fetch_bg_pixel_cgb(
    x: u32,
    scanline: u8,
    vram: &[u8; 0x2000],
    vram_bank1: &[u8; 0x2000],
    lcdc: u8,
    scx: u8,
    scy: u8,
) -> BgPixelCgb {
    fetch_bg_pixel_cgb_with_fine_scx(
        vram,
        vram_bank1,
        CgbPixelFetch {
            x,
            scanline,
            scx,
            fine_scx: scx,
            scy,
            lcdc,
        },
    )
}

/// Fetch a CGB background pixel with SCX tile-map sampling split from the
/// latched fine-scroll bits used to select the pixel within the tile.
pub(super) fn fetch_bg_pixel_cgb_with_fine_scx(
    vram: &[u8; 0x2000],
    vram_bank1: &[u8; 0x2000],
    fetch: CgbPixelFetch,
) -> BgPixelCgb {
    let bg_x = fetch.scx.wrapping_add(fetch.x as u8);
    let bg_y = fetch.scy.wrapping_add(fetch.scanline);

    let map_base: usize = if fetch.lcdc & 0x08 != 0 {
        0x1C00
    } else {
        0x1800
    };
    let tile_col = (bg_x / 8) as usize;
    let tile_row = (bg_y / 8) as usize;
    let map_offset = map_base + tile_row * 32 + tile_col;

    let tile_index_raw = vram[map_offset];
    // Tile attributes live at the same map offset in VRAM bank 1.
    let attrs = vram_bank1[map_offset];

    let palette_num = attrs & 0x07;
    let tile_vram_bank = (attrs >> 3) & 0x01;
    let x_flip = attrs & 0x20 != 0;
    let y_flip = attrs & 0x40 != 0;
    let bg_priority = attrs & 0x80 != 0;

    let tile_data_start: usize = if fetch.lcdc & 0x10 != 0 {
        (tile_index_raw as usize) * 16
    } else {
        (0x1000i32 + (tile_index_raw as i8 as i32) * 16) as usize
    };

    let mut row_in_tile = (bg_y % 8) as usize;
    if y_flip {
        row_in_tile = 7 - row_in_tile;
    }

    let pixel_in_tile = fetch.fine_scx.wrapping_add(fetch.x as u8) % 8;
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

    BgPixelCgb {
        colour_index,
        palette_num,
        vram_bank: tile_vram_bank,
        bg_priority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_vram() -> [u8; 0x2000] {
        [0u8; 0x2000]
    }

    /// In blank VRAM with all-zero tile data and all-zero tile map,
    /// every BG pixel should have colour index 0.
    #[test]
    fn test_blank_vram_returns_colour_index_0() {
        // Given: all-zero VRAM, LCDC = 0x91 (BG on, tile data at $8000, map at $9800)
        let vram = blank_vram();
        // When: fetch BG pixel at (0, 0)
        let idx = fetch_bg_pixel(0, 0, &vram, 0x91, 0, 0);
        // Then: colour index is 0
        assert_eq!(idx, 0);
    }

    /// Verify that a non-zero tile in the first map entry produces the expected
    /// colour bits from that tile's row data.
    #[test]
    fn test_tile_data_colour_bits_are_fetched_correctly() {
        // Given: VRAM with tile 1 at $8010 (tile data at $8000 mode)
        // Tile 1 row 0: low byte = 0xFF, high byte = 0x00 → all pixels colour index 1
        let mut vram = blank_vram();
        // Tile data base $8000 → vram offset 0x0000; tile 1 is at offset 0x0010
        vram[0x0010] = 0xFF; // low byte of row 0
        vram[0x0011] = 0x00; // high byte of row 0 → colour index for each pixel = (0<<1)|1 = 1
        // BG tile map at $9800 → vram offset 0x1800; entry (0,0) = tile 1
        vram[0x1800] = 0x01;
        // LCDC: LCD on, tile data $8000 (bit 4=1), BG map $9800 (bit 3=0)
        let lcdc = 0x91u8; // 0b10010001
        // When: fetch BG pixel at x=0 (leftmost pixel of tile)
        let idx = fetch_bg_pixel(0, 0, &vram, lcdc, 0, 0);
        // Then: low bit set, high bit clear → colour index 1
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_scx_shifts_bg_map_lookup() {
        // Given: tile 1 at $8010, tile map entry (1,0) = tile 2 at $8020
        // But entry (0,0) = 0 (blank).
        // SCX = 8 means we start reading from X=8 in BG space (tile column 1).
        let mut vram = blank_vram();
        // Tile 2 at offset $0020: row 0 low = 0xFF, high = 0xFF → colour 3
        vram[0x0020] = 0xFF;
        vram[0x0021] = 0xFF;
        // Map entry col=1, row=0 → tile 2
        vram[0x1801] = 0x02;
        let lcdc = 0x91u8;
        // When: fetch pixel at screen x=0 with SCX=8 (places us at BG col 8 = tile 1)
        let idx = fetch_bg_pixel(0, 0, &vram, lcdc, 8, 0);
        // Then: colour index is 3 (from tile 2)
        assert_eq!(idx, 3);
    }

    #[test]
    fn test_scy_shifts_bg_map_row_lookup() {
        // Given: tile 1 loaded at $8000 area, map row 1 col 0 = tile 0x01
        let mut vram = blank_vram();
        // Tile 1 row 0 → colour 2 (low=0x00, high=0xFF)
        vram[0x0010] = 0x00;
        vram[0x0011] = 0xFF;
        // Map row 1, col 0 ($9820 = vram[0x1820]) = tile 1
        vram[0x1820] = 0x01;
        let lcdc = 0x91u8;
        // When: fetch pixel at scanline=0 with SCY=8 (places us at BG row 8 = tile row 1)
        let idx = fetch_bg_pixel(0, 0, &vram, lcdc, 0, 8);
        // Then: colour index 2
        assert_eq!(idx, 2);
    }

    // ── CGB background pixel tests ────────────────────────────────────────────

    #[test]
    fn test_cgb_bg_palette_num_extracted_from_attrs() {
        // Given: tile attrs at map slot (0,0) in VRAM bank 1 with palette_num=5 (bits 0-2 = 0b101)
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x05; // palette bits 0-2 = 5
        // When: fetch at (0, 0)
        let px = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        // Then: palette_num == 5
        assert_eq!(px.palette_num, 5);
    }

    #[test]
    fn test_cgb_bg_priority_flag_extracted_from_attrs() {
        // Given: tile attrs with bit 7 set (bg_priority)
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x80; // bg_priority bit set
        let px = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert!(px.bg_priority);
        // Without bit 7: false
        bank1[0x1800] = 0x00;
        let px2 = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert!(!px2.bg_priority);
    }

    #[test]
    fn test_cgb_bg_tile_data_read_from_vram_bank1_when_attr_bit3_set() {
        // Given: tile index 0 in map bank 0; attrs with VRAM bank bit (bit 3) set
        // Tile 0 in bank 0: all zeros
        // Tile 0 in bank 1: row 0 low=0xFF → colour 1 for all pixels
        let vram = blank_vram();
        let mut bank1 = blank_vram();
        bank1[0x1800] = 0x08; // VRAM bank bit set, palette 0
        bank1[0x0000] = 0xFF; // tile 0 row 0 low byte in bank 1
        bank1[0x0001] = 0x00; // tile 0 row 0 high byte in bank 1
        let px = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        // Pixel 0 (leftmost): bit = 7, colour = (0>>7)&1 << 1 | (0xFF>>7)&1 = 1
        assert_eq!(px.colour_index, 1);
        assert_eq!(px.vram_bank, 1);
    }

    #[test]
    fn test_cgb_bg_x_flip_reverses_pixel_order() {
        // Tile 0 row 0: low=0x01 → only bit 0 (rightmost pixel, pixel_in_tile=7) has colour 1.
        // Without x_flip: pixel at x=7 (rightmost) has colour 1; x=0 has colour 0.
        // With x_flip: pixel at x=0 (leftmost) has colour 1.
        let mut vram = blank_vram();
        let mut bank1 = blank_vram();
        vram[0x0000] = 0x01; // only bit 0 set → colour 1 at rightmost pixel
        // No flip (attrs=0): x=0 should see bit 7 → colour 0
        let px_no_flip = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert_eq!(px_no_flip.colour_index, 0);
        // x_flip set (attrs bit 5): x=0 should see bit 0 → colour 1
        bank1[0x1800] = 0x20; // x_flip bit
        let px_x_flip = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert_eq!(px_x_flip.colour_index, 1);
    }

    #[test]
    fn test_cgb_bg_y_flip_reverses_row_order() {
        // Tile 0: row 7 (bottom) has colour 1; row 0 (top) has colour 0.
        let mut vram = blank_vram();
        let mut bank1 = blank_vram();
        vram[0x000E] = 0xFF; // tile 0 row 7 low byte (row_offset = 7*2 = 14)
        vram[0x000F] = 0x00;
        // No y_flip: scanline=0 → row 0 → colour 0
        let px_no = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert_eq!(px_no.colour_index, 0);
        // y_flip set (attrs bit 6): scanline=0 → row_in_tile = 7-0 = 7 → colour 1
        bank1[0x1800] = 0x40; // y_flip bit
        let px_yf = fetch_bg_pixel_cgb(0, 0, &vram, &bank1, 0x91, 0, 0);
        assert_eq!(px_yf.colour_index, 1);
    }
}
