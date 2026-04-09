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
    for &i in sprite_indices {
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
}
