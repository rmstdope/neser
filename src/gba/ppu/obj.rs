//! GBA OBJ (sprite) rendering.
//!
//! Handles decoding OAM attributes, rendering regular (non-affine) sprites
//! into a per-scanline buffer, and generating the OBJ Window mask.
//!
//! Per GBATek "LCD OBJ - OAM Attributes":
//! - 128 OBJ entries × 3 attributes (6 bytes each, 8 bytes with affine fill)
//! - Sprites use VRAM starting at 0x0601_0000 (charblock 4)
//! - Tile indices in 32-byte units (s-tile sized)
//! - 4bpp: 16 colors from one of 16 OBJ sub-palettes
//! - 8bpp: 256 colors from the single OBJ palette
//!
//! References:
//! - GBATek: <https://problemkaputt.de/gbatek.htm#lcdobjoamattributes>
//! - TONC: <https://www.coranac.com/tonc/text/regobj.htm>

/// Screen width in pixels.
const SCREEN_WIDTH: usize = 240;

/// OBJ VRAM base offset within the 96KB VRAM.
/// Sprites use charblock 4 (0x10000) and 5 (0x14000).
const OBJ_VRAM_BASE: usize = 0x1_0000;

/// Number of OBJ entries in OAM.
const OBJ_COUNT: usize = 128;

/// A single pixel in the OBJ scanline buffer.
#[derive(Clone, Copy, Default)]
pub struct ObjPixel {
    /// BGR555 color (only valid if `opaque` is true).
    pub color: u16,
    /// Whether this pixel is opaque (has a non-transparent sprite pixel).
    pub opaque: bool,
    /// OBJ priority (0-3, from attr2 bits 10-11).
    pub priority: u8,
    /// Whether this pixel is semi-transparent (OBJ mode 1).
    pub semi_transparent: bool,
}

/// Per-scanline OBJ rendering result.
pub struct ObjScanline {
    /// The rendered OBJ pixels for this scanline.
    pub pixels: [ObjPixel; SCREEN_WIDTH],
    /// OBJ Window mask: true if an OBJ Window sprite covers this pixel.
    pub obj_window: [bool; SCREEN_WIDTH],
}

impl Default for ObjScanline {
    fn default() -> Self {
        Self {
            pixels: [ObjPixel::default(); SCREEN_WIDTH],
            obj_window: [false; SCREEN_WIDTH],
        }
    }
}

/// Sprite size in pixels (width, height).
fn obj_size(shape: u8, size: u8) -> (u32, u32) {
    match (shape, size) {
        // Square
        (0, 0) => (8, 8),
        (0, 1) => (16, 16),
        (0, 2) => (32, 32),
        (0, 3) => (64, 64),
        // Wide (horizontal)
        (1, 0) => (16, 8),
        (1, 1) => (32, 8),
        (1, 2) => (32, 16),
        (1, 3) => (64, 32),
        // Tall (vertical)
        (2, 0) => (8, 16),
        (2, 1) => (8, 32),
        (2, 2) => (16, 32),
        (2, 3) => (32, 64),
        _ => (8, 8), // fallback
    }
}

/// Render all OBJs for a single scanline.
///
/// - `y`: current scanline (0..160)
/// - `oam`: 1KB OAM data
/// - `vram`: 96KB VRAM (OBJ tiles at offset 0x10000)
/// - `pram`: 1KB palette RAM (OBJ palette at offset 0x200)
/// - `obj_mapping_1d`: true if DISPCNT bit 6 is set (1D tile mapping)
pub fn render_obj_scanline(
    y: u32,
    oam: &[u8],
    vram: &[u8],
    pram: &[u8],
    obj_mapping_1d: bool,
) -> ObjScanline {
    let mut result = ObjScanline::default();

    // Process OBJs in order 0..127. Lower number = higher priority at same
    // priority level, so first writer wins for each pixel.
    for obj_idx in 0..OBJ_COUNT {
        let base = obj_idx * 8;
        if base + 5 >= oam.len() {
            break;
        }

        let attr0 = u16::from_le_bytes([oam[base], oam[base + 1]]);
        let attr1 = u16::from_le_bytes([oam[base + 2], oam[base + 3]]);
        let attr2 = u16::from_le_bytes([oam[base + 4], oam[base + 5]]);

        // Object mode (attr0 bits 8-9).
        let obj_mode = (attr0 >> 8) & 3;

        // Mode 2 = hidden (disabled), skip.
        if obj_mode == 2 {
            continue;
        }

        // Mode 1 = affine — skip for now (not implemented in this increment).
        // Mode 3 = affine double-size — skip.
        if obj_mode == 1 || obj_mode == 3 {
            continue;
        }

        // GFX mode (attr0 bits 10-11).
        let gfx_mode = (attr0 >> 10) & 3;
        // Mode 3 = prohibited, skip.
        if gfx_mode == 3 {
            continue;
        }

        let is_obj_window = gfx_mode == 2;
        let is_semi_transparent = gfx_mode == 1;

        // Color mode: 0 = 4bpp, 1 = 8bpp.
        let is_8bpp = (attr0 >> 13) & 1 != 0;

        // Shape and size.
        let shape = ((attr0 >> 14) & 3) as u8;
        let size_bits = ((attr1 >> 14) & 3) as u8;
        let (obj_width, obj_height) = obj_size(shape, size_bits);

        // Y coordinate (attr0 bits 0-7), wraps at 256.
        let obj_y = (attr0 & 0xFF) as u32;
        // X coordinate (attr1 bits 0-8), sign-extended 9 bits.
        let obj_x = {
            let raw = (attr1 & 0x1FF) as i32;
            if raw >= 256 { raw - 512 } else { raw }
        };

        // Check if this scanline intersects the sprite (wrapping Y at 256).
        let rel_y = y.wrapping_sub(obj_y) & 0xFF;
        if rel_y >= obj_height {
            continue;
        }

        // Flip flags (non-affine only).
        let h_flip = (attr1 >> 12) & 1 != 0;
        let v_flip = (attr1 >> 13) & 1 != 0;

        // Tile index (attr2 bits 0-9).
        let tile_id = (attr2 & 0x03FF) as usize;
        // Priority (attr2 bits 10-11).
        let priority = ((attr2 >> 10) & 3) as u8;
        // Palette bank (attr2 bits 12-15), used in 4bpp mode.
        let palette_bank = ((attr2 >> 12) & 0xF) as usize;

        // Apply vertical flip to the row within the sprite.
        let sprite_row = if v_flip {
            obj_height - 1 - rel_y
        } else {
            rel_y
        };

        // Render each pixel of this sprite on the current scanline.
        for sprite_col in 0..obj_width {
            let screen_x = obj_x + sprite_col as i32;
            if screen_x < 0 || screen_x >= SCREEN_WIDTH as i32 {
                continue;
            }
            let sx = screen_x as usize;

            // Apply horizontal flip.
            let pixel_col = if h_flip {
                obj_width - 1 - sprite_col
            } else {
                sprite_col
            };

            // Calculate tile and pixel within tile.
            let tile_col = pixel_col / 8;
            let tile_row = sprite_row / 8;
            let pixel_x = (pixel_col % 8) as usize;
            let pixel_y = (sprite_row % 8) as usize;

            // Calculate tile offset based on mapping mode.
            let tile_offset = if obj_mapping_1d {
                // 1D: tiles are consecutive. Row stride = width_in_tiles.
                let width_in_tiles = obj_width / 8;
                let stride = if is_8bpp {
                    width_in_tiles * 2 // 8bpp tiles take 2 s-tile slots
                } else {
                    width_in_tiles
                };
                tile_id + (tile_row * stride + tile_col) as usize
            } else {
                // 2D: tiles laid out in a 32-tile wide virtual bitmap.
                let stride = if is_8bpp { 16 } else { 32 };
                tile_id + (tile_row as usize) * stride + tile_col as usize
            };

            // Get palette index from tile data.
            let palette_index = if is_8bpp {
                // 8bpp: 64 bytes per tile, 1 byte per pixel.
                let addr = OBJ_VRAM_BASE + tile_offset * 32 + pixel_y * 8 + pixel_x;
                vram.get(addr).copied().unwrap_or(0) as usize
            } else {
                // 4bpp: 32 bytes per tile, 4 bits per pixel.
                let addr = OBJ_VRAM_BASE + tile_offset * 32 + pixel_y * 4 + pixel_x / 2;
                let byte = vram.get(addr).copied().unwrap_or(0);
                if pixel_x & 1 == 0 {
                    (byte & 0x0F) as usize
                } else {
                    (byte >> 4) as usize
                }
            };

            // Palette index 0 is transparent.
            if palette_index == 0 {
                continue;
            }

            // For OBJ Window mode, just set the mask bit.
            if is_obj_window {
                result.obj_window[sx] = true;
                continue;
            }

            // First OBJ to write a pixel wins (lower OBJ number = higher priority).
            if result.pixels[sx].opaque {
                continue;
            }

            // Look up color from OBJ palette (starts at PRAM offset 0x200).
            let pram_offset = if is_8bpp {
                0x200 + palette_index * 2
            } else {
                0x200 + (palette_bank * 16 + palette_index) * 2
            };

            let bgr555 = if pram_offset + 1 < pram.len() {
                u16::from_le_bytes([pram[pram_offset], pram[pram_offset + 1]])
            } else {
                0
            };

            result.pixels[sx] = ObjPixel {
                color: bgr555,
                opaque: true,
                priority,
                semi_transparent: is_semi_transparent,
            };
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_oam() -> Vec<u8> {
        vec![0u8; 1024]
    }

    fn make_vram() -> Vec<u8> {
        vec![0u8; 96 * 1024]
    }

    fn make_pram() -> Vec<u8> {
        vec![0u8; 1024]
    }

    /// Helper: write an OBJ_ATTR entry into OAM.
    fn write_obj(oam: &mut [u8], idx: usize, attr0: u16, attr1: u16, attr2: u16) {
        let base = idx * 8;
        oam[base..base + 2].copy_from_slice(&attr0.to_le_bytes());
        oam[base + 2..base + 4].copy_from_slice(&attr1.to_le_bytes());
        oam[base + 4..base + 6].copy_from_slice(&attr2.to_le_bytes());
    }

    /// Helper: fill a 4bpp tile in OBJ VRAM with a single palette index.
    fn fill_4bpp_tile(vram: &mut [u8], tile_id: usize, pal_idx: u8) {
        let base = OBJ_VRAM_BASE + tile_id * 32;
        let nibble_pair = pal_idx | (pal_idx << 4);
        for i in 0..32 {
            vram[base + i] = nibble_pair;
        }
    }

    /// Helper: fill an 8bpp tile in OBJ VRAM with a single palette index.
    fn fill_8bpp_tile(vram: &mut [u8], tile_id: usize, pal_idx: u8) {
        let base = OBJ_VRAM_BASE + tile_id * 32;
        for i in 0..64 {
            vram[base + i] = pal_idx;
        }
    }

    /// Helper: set an OBJ palette color.
    fn set_obj_color(pram: &mut [u8], index: usize, bgr555: u16) {
        let offset = 0x200 + index * 2;
        pram[offset] = bgr555 as u8;
        pram[offset + 1] = (bgr555 >> 8) as u8;
    }

    #[test]
    fn empty_oam_produces_no_pixels() {
        let oam = make_oam();
        let vram = make_vram();
        let pram = make_pram();
        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        assert!(result.pixels.iter().all(|p| !p.opaque));
        assert!(result.obj_window.iter().all(|&w| !w));
    }

    #[test]
    fn hidden_obj_not_rendered() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: hidden (obj_mode = 2 in attr0 bits 8-9).
        // 8x8, at (100,0), 4bpp, tile 1.
        let attr0 = 2 << 8; // y=0, obj_mode=2 (hidden)
        let attr1 = 100; // x=100
        let attr2 = 1; // tile 1
        write_obj(&mut oam, 0, attr0, attr1, attr2);
        fill_4bpp_tile(&mut vram, 1, 1);
        set_obj_color(&mut pram, 1, 0x001F); // red

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        assert!(!result.pixels[100].opaque, "hidden OBJ should not render");
    }

    #[test]
    fn basic_8x8_4bpp_sprite_renders() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8, at (10, 5), 4bpp, tile 0, palette bank 0, priority 0.
        let attr0 = 5; // y=5, normal mode, 4bpp, square shape
        let attr1 = 10; // x=10, size=0 (8x8)
        let attr2 = 0; // tile 0, priority 0, pal bank 0
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // Fill tile 0 with palette index 3.
        fill_4bpp_tile(&mut vram, 0, 3);
        // Set OBJ palette bank 0, color 3 = green.
        set_obj_color(&mut pram, 3, 0x03E0);

        // Render scanline 5 (first row of sprite).
        let result = render_obj_scanline(5, &oam, &vram, &pram, true);

        // Pixels 10-17 should be green.
        for x in 10..18 {
            assert!(result.pixels[x].opaque, "pixel {x} should be opaque");
            assert_eq!(result.pixels[x].color, 0x03E0, "pixel {x} should be green");
            assert_eq!(result.pixels[x].priority, 0);
        }
        // Pixel 9 should not be affected.
        assert!(!result.pixels[9].opaque);
        // Pixel 18 should not be affected.
        assert!(!result.pixels[18].opaque);
    }

    #[test]
    fn basic_8x8_8bpp_sprite_renders() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8, at (0, 0), 8bpp (attr0 bit 13), tile 0.
        let attr0 = 1 << 13; // y=0, 8bpp
        let attr1 = 0; // x=0, size=0 (8x8)
        let attr2 = 0; // tile 0
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // Fill tile 0 with palette index 5 (8bpp takes 2 s-tile slots).
        fill_8bpp_tile(&mut vram, 0, 5);
        // Set OBJ palette color 5 = blue.
        set_obj_color(&mut pram, 5, 0x7C00);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);

        for x in 0..8 {
            assert!(result.pixels[x].opaque, "pixel {x} should be opaque");
            assert_eq!(result.pixels[x].color, 0x7C00, "pixel {x} should be blue");
        }
    }

    #[test]
    fn horizontal_flip_reverses_pixels() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 8x8 sprite at (0,0), 4bpp, h-flip (attr1 bit 12).
        let attr0 = 0;
        let attr1 = 1 << 12; // h-flip
        let attr2 = 0;
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // Fill tile 0: left half with index 1, right half with index 2.
        let base = OBJ_VRAM_BASE;
        for row in 0..8 {
            for col in 0..4 {
                let addr = base + row * 4 + col;
                // 4bpp: 2 pixels per byte.
                let px = col * 2;
                if px < 4 {
                    vram[addr] = 0x11; // index 1 for both nibbles
                } else {
                    vram[addr] = 0x22; // index 2 for both nibbles
                }
            }
        }

        set_obj_color(&mut pram, 1, 0x001F); // red
        set_obj_color(&mut pram, 2, 0x03E0); // green

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);

        // With h-flip, right side (index 2, green) appears on left.
        // Pixel 0-3 should be green (was right side), 4-7 should be red (was left side).
        assert_eq!(
            result.pixels[0].color, 0x03E0,
            "flipped: left should be green"
        );
        assert_eq!(
            result.pixels[4].color, 0x001F,
            "flipped: right should be red"
        );
    }

    #[test]
    fn vertical_flip_reverses_rows() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 8x8 sprite at (0,0), 4bpp, v-flip (attr1 bit 13).
        let attr0 = 0;
        let attr1 = 1 << 13; // v-flip
        let attr2 = 0;
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // Fill tile: row 0 with index 1, row 7 with index 2.
        let base = OBJ_VRAM_BASE;
        for col in 0..4 {
            vram[base + col] = 0x11; // row 0: index 1
            vram[base + 7 * 4 + col] = 0x22; // row 7: index 2
        }

        set_obj_color(&mut pram, 1, 0x001F); // red
        set_obj_color(&mut pram, 2, 0x03E0); // green

        // With v-flip, row 7 (index 2) becomes the top row (scanline 0).
        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        assert_eq!(
            result.pixels[0].color, 0x03E0,
            "v-flip: scanline 0 should show row 7 data (green)"
        );

        // Scanline 7 should show original row 0 (index 1, red).
        let result7 = render_obj_scanline(7, &oam, &vram, &pram, true);
        assert_eq!(
            result7.pixels[0].color, 0x001F,
            "v-flip: scanline 7 should show row 0 data (red)"
        );
    }

    #[test]
    fn lower_obj_number_wins_same_pixel() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8 at (0,0), tile 0, priority 0.
        write_obj(&mut oam, 0, 0, 0, 0);
        fill_4bpp_tile(&mut vram, 0, 1);
        set_obj_color(&mut pram, 1, 0x001F); // red

        // OBJ 1: 8x8 at (0,0), tile 1, priority 0.
        write_obj(&mut oam, 1, 0, 0, 1);
        fill_4bpp_tile(&mut vram, 1, 2);
        set_obj_color(&mut pram, 2, 0x03E0); // green

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        // OBJ 0 wins (lower index).
        assert_eq!(
            result.pixels[0].color, 0x001F,
            "OBJ 0 should win over OBJ 1"
        );
    }

    #[test]
    fn transparent_pixel_does_not_block() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8 at (0,0), tile 0 — all transparent (palette index 0).
        write_obj(&mut oam, 0, 0, 0, 0);
        // Tile 0 left as zeros (transparent).

        // OBJ 1: 8x8 at (0,0), tile 1 — opaque green.
        write_obj(&mut oam, 1, 0, 0, 1);
        fill_4bpp_tile(&mut vram, 1, 2);
        set_obj_color(&mut pram, 2, 0x03E0);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        // OBJ 0 is transparent, so OBJ 1 should show through.
        assert_eq!(
            result.pixels[0].color, 0x03E0,
            "transparent OBJ 0 should not block OBJ 1"
        );
    }

    #[test]
    fn obj_window_sets_mask_not_pixel() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8 at (100,0), gfx_mode=2 (OBJ Window), tile 1.
        let attr0 = 2 << 10; // gfx_mode = 2
        let attr1 = 100u16; // x=100
        let attr2 = 1u16; // tile 1
        write_obj(&mut oam, 0, attr0, attr1, attr2);
        fill_4bpp_tile(&mut vram, 1, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);

        // OBJ Window should not produce visible pixels at x=100.
        assert!(
            !result.pixels[100].opaque,
            "OBJ Window should not be visible"
        );
        // But should set the window mask.
        assert!(result.obj_window[100], "OBJ Window should set mask bit");
    }

    #[test]
    fn obj_2d_mapping_uses_32_tile_stride() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 16x16 sprite (shape=0, size=1) at (0,0), 4bpp, tile 0.
        let attr0 = 0; // square
        let attr1 = 1 << 14; // size = 1 (16x16)
        let attr2 = 0; // tile 0
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // In 2D mode with 4bpp, row 1 of tiles is at tile_id + 32.
        // Fill tile at row 1, col 0 (offset 32) with palette index 3.
        fill_4bpp_tile(&mut vram, 32, 3);
        set_obj_color(&mut pram, 3, 0x7C00); // blue

        // Scanline 8 = second tile row.
        let result = render_obj_scanline(8, &oam, &vram, &pram, false);
        assert!(
            result.pixels[0].opaque,
            "2D mapping should find tile at stride 32"
        );
        assert_eq!(result.pixels[0].color, 0x7C00);
    }

    #[test]
    fn obj_1d_mapping_uses_width_stride() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 16x16 sprite (shape=0, size=1) at (0,0), 4bpp, tile 0.
        let attr0 = 0;
        let attr1 = 1 << 14; // size = 1 (16x16)
        let attr2 = 0; // tile 0
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // In 1D mode with 4bpp, 16px wide = 2 tiles. Row 1 at tile_id + 2.
        fill_4bpp_tile(&mut vram, 2, 4);
        set_obj_color(&mut pram, 4, 0x03E0); // green

        // Scanline 8 = second tile row.
        let result = render_obj_scanline(8, &oam, &vram, &pram, true);
        assert!(
            result.pixels[0].opaque,
            "1D mapping should find tile at width stride"
        );
        assert_eq!(result.pixels[0].color, 0x03E0);
    }

    #[test]
    fn sprite_wraps_y_at_256() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ at Y=252, 8x8. Should appear on scanlines 252-255 and 0-3.
        let attr0 = 252u16; // y=252
        write_obj(&mut oam, 0, attr0, 0, 0);
        fill_4bpp_tile(&mut vram, 0, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        // Scanline 0: relative Y = (0 - 252) & 0xFF = 4. 4 < 8, so visible.
        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        assert!(
            result.pixels[0].opaque,
            "sprite at Y=252 should wrap to scanline 0"
        );
    }

    #[test]
    fn sprite_negative_x_partially_visible() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Disable all OBJs by setting them to hidden mode, then enable just OBJ 0.
        for i in 0..OBJ_COUNT {
            write_obj(&mut oam, i, 2 << 8, 0, 0); // obj_mode=2 (hidden)
        }

        // OBJ 0: at X=508 (sign-extended: 508-512 = -4), Y=0, 8x8, tile 1.
        let attr0 = 0;
        let attr1 = 508u16; // 9-bit: 508, sign-extended = -4
        let attr2 = 1u16; // tile 1
        write_obj(&mut oam, 0, attr0, attr1, attr2);
        fill_4bpp_tile(&mut vram, 1, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);

        // X=-4: pixels at screen 0,1,2,3 should be visible (sprite cols 4,5,6,7).
        assert!(
            result.pixels[0].opaque,
            "partially visible sprite at negative X"
        );
        assert!(result.pixels[3].opaque);
        // But only 4 pixels should be visible.
        assert!(!result.pixels[4].opaque);
    }

    #[test]
    fn priority_field_is_read_from_attr2() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: priority 2 (attr2 bits 10-11 = 2).
        let attr2 = 2 << 10;
        write_obj(&mut oam, 0, 0, 0, attr2);
        fill_4bpp_tile(&mut vram, 0, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        assert_eq!(result.pixels[0].priority, 2);
    }

    #[test]
    fn large_64x64_sprite_covers_area() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 64x64 (shape=0, size=3) at (0,0), 4bpp, tile 0, 1D mapping.
        let attr0 = 0;
        let attr1 = 3 << 14; // size = 3
        let attr2 = 0;
        write_obj(&mut oam, 0, attr0, attr1, attr2);

        // Fill tile 0 only (first tile of row 0).
        fill_4bpp_tile(&mut vram, 0, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true);
        // First 8 pixels should be opaque (tile 0).
        assert!(result.pixels[0].opaque);
        assert!(result.pixels[7].opaque);
    }
}
