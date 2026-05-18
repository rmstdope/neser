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
        // Shape 3 is prohibited — return (0, 0) to signal skip.
        _ => (0, 0),
    }
}

/// Fetch the palette index for a single pixel within an OBJ sprite.
///
/// Given a pixel coordinate (`pixel_col`, `pixel_row`) in sprite-local space,
/// looks up the tile data considering mapping mode and color depth.
/// Returns the palette index (0 = transparent).
#[allow(clippy::too_many_arguments)]
fn fetch_obj_pixel(
    tile_id: usize,
    obj_width: u32,
    pixel_col: u32,
    pixel_row: u32,
    is_8bpp: bool,
    obj_mapping_1d: bool,
    vram: &[u8],
    obj_vram_base: usize,
) -> usize {
    let tile_col = pixel_col / 8;
    let tile_row = pixel_row / 8;
    let pixel_x = (pixel_col % 8) as usize;
    let pixel_y = (pixel_row % 8) as usize;

    let col_stride: u32 = if is_8bpp { 2 } else { 1 };
    let tile_offset = if obj_mapping_1d {
        let width_in_tiles = obj_width / 8;
        let row_stride = width_in_tiles * col_stride;
        tile_id + (tile_row * row_stride + tile_col * col_stride) as usize
    } else {
        tile_id + (tile_row as usize) * 32 + (tile_col * col_stride) as usize
    };

    if is_8bpp {
        let addr = obj_vram_base + tile_offset * 32 + pixel_y * 8 + pixel_x;
        vram.get(addr).copied().unwrap_or(0) as usize
    } else {
        let addr = obj_vram_base + tile_offset * 32 + pixel_y * 4 + pixel_x / 2;
        let byte = vram.get(addr).copied().unwrap_or(0);
        if pixel_x & 1 == 0 {
            (byte & 0x0F) as usize
        } else {
            (byte >> 4) as usize
        }
    }
}

/// Read affine parameters (PA, PB, PC, PD) for a rotation group from OAM.
///
/// The 32 rotation groups are interleaved in OAM: for group `g`, the parameters
/// are at byte offsets g*32+6 (PA), g*32+14 (PB), g*32+22 (PC), g*32+30 (PD).
/// Each parameter is a signed 16-bit value in 8.8 fixed-point.
fn read_affine_params(oam: &[u8], group: usize) -> (i16, i16, i16, i16) {
    let base = group * 32;
    let pa = i16::from_le_bytes([oam[base + 6], oam[base + 7]]);
    let pb = i16::from_le_bytes([oam[base + 14], oam[base + 15]]);
    let pc = i16::from_le_bytes([oam[base + 22], oam[base + 23]]);
    let pd = i16::from_le_bytes([oam[base + 30], oam[base + 31]]);
    (pa, pb, pc, pd)
}

/// OBJ rendering cycle budget per scanline when DISPCNT bit 5 (H-Blank Interval Free) = 0.
///
/// 304×4 − 6 = 1210 cycles (GBATek "LCD OBJ - Overview").
pub const OBJ_CYCLE_BUDGET_NORMAL: u32 = 1210;

/// OBJ rendering cycle budget per scanline when DISPCNT bit 5 (H-Blank Interval Free) = 1.
///
/// 240×4 − 6 = 954 cycles (GBATek "LCD OBJ - Overview").
pub const OBJ_CYCLE_BUDGET_HBLANK_FREE: u32 = 954;

/// Render all OBJs for a single scanline.
///
/// - `y`: current scanline (0..160)
/// - `oam`: 1KB OAM data
/// - `vram`: 96KB VRAM (OBJ tiles at offset `obj_vram_base`)
/// - `pram`: 1KB palette RAM (OBJ palette at offset 0x200)
/// - `obj_mapping_1d`: true if DISPCNT bit 6 is set (1D tile mapping)
/// - `bitmap_mode`: true for modes 3-5 (OBJ tiles start at 0x14000 instead of 0x10000)
/// - `cycle_budget`: maximum rendering cycles available; use [`OBJ_CYCLE_BUDGET_NORMAL`] or
///   [`OBJ_CYCLE_BUDGET_HBLANK_FREE`] based on DISPCNT bit 5
#[allow(clippy::too_many_arguments)]
pub fn render_obj_scanline(
    y: u32,
    oam: &[u8],
    vram: &[u8],
    pram: &[u8],
    obj_mapping_1d: bool,
    bitmap_mode: bool,
    mosaic: u16,
    cycle_budget: u32,
) -> ObjScanline {
    let mut result = ObjScanline::default();
    let obj_vram_base = if bitmap_mode { 0x1_4000 } else { 0x1_0000 };
    let obj_mosaic_h = super::mosaic_size(mosaic, 8);
    let obj_mosaic_v = super::mosaic_size(mosaic, 12);
    let mut cycles_used: u32 = 0;

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

        let is_affine = obj_mode == 1 || obj_mode == 3;
        let is_double_size = obj_mode == 3;
        let mosaic_enabled = attr0 & (1 << 12) != 0;

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

        // Shape 3 is prohibited — skip.
        if obj_width == 0 || obj_height == 0 {
            continue;
        }

        // Bounding box: double-size affine uses 2× dimensions for rendering area.
        let (bound_width, bound_height) = if is_double_size {
            (obj_width * 2, obj_height * 2)
        } else {
            (obj_width, obj_height)
        };

        // Per GBATek "LCD OBJ - Overview": count cycles for every non-hidden OBJ
        // with a valid shape, even if it doesn't intersect the current scanline.
        // Cycle cost formula (GBATek):
        //   - Normal OBJ:       n×1 cycles  (n = bound_width)
        //   - Rotation/Scaling: 10 + n×2 cycles (n = bound_width, including double-size)
        let cycle_cost = if is_affine {
            10 + bound_width * 2
        } else {
            bound_width
        };
        // OBJs are processed in priority order (0 first, 127 last). Once the
        // cycle budget is exhausted, all remaining OBJs are dropped — they are
        // simply not rendered for this scanline.
        if cycles_used + cycle_cost > cycle_budget {
            break;
        }
        cycles_used += cycle_cost;

        // Y coordinate (attr0 bits 0-7), wraps at 256.
        let obj_y = (attr0 & 0xFF) as u32;
        // X coordinate (attr1 bits 0-8), sign-extended 9 bits.
        let obj_x = {
            let raw = (attr1 & 0x1FF) as i32;
            if raw >= 256 { raw - 512 } else { raw }
        };

        // Check if this scanline intersects the sprite bounding box (wrapping Y at 256).
        let rel_y = y.wrapping_sub(obj_y) & 0xFF;
        if rel_y >= bound_height {
            continue;
        }

        // Tile index (attr2 bits 0-9).
        let tile_id = (attr2 & 0x03FF) as usize;
        // Priority (attr2 bits 10-11).
        let priority = ((attr2 >> 10) & 3) as u8;
        // Palette bank (attr2 bits 12-15), used in 4bpp mode.
        let palette_bank = ((attr2 >> 12) & 0xF) as usize;

        if is_affine {
            // Affine OBJ: read rotation/scaling parameters and transform pixels.
            let affine_group = ((attr1 >> 9) & 0x1F) as usize;
            let (pa, pb, pc, pd) = read_affine_params(oam, affine_group);

            // Center of bounding box in pixels (half-width, half-height).
            let cx = bound_width as i32 / 2;
            let cy = bound_height as i32 / 2;

            // Pre-compute the row offset from center.
            let sample_rel_y = if mosaic_enabled {
                super::mosaic_anchor(rel_y, obj_mosaic_v)
            } else {
                rel_y
            };
            let iry = sample_rel_y as i32 - cy;

            for bx in 0..bound_width {
                let screen_x = obj_x + bx as i32;
                if screen_x < 0 || screen_x >= SCREEN_WIDTH as i32 {
                    continue;
                }
                let sx = screen_x as usize;

                // Offset from bounding box center.
                let sample_bx = if mosaic_enabled {
                    super::mosaic_anchor(bx, obj_mosaic_h)
                } else {
                    bx
                };
                let irx = sample_bx as i32 - cx;

                // Apply affine transform: map screen pixel to texture space.
                // Result is in 8.8 fixed-point, add half-sprite-size to center.
                let tex_x = (pa as i32 * irx + pb as i32 * iry) + ((obj_width as i32 / 2) << 8);
                let tex_y = (pc as i32 * irx + pd as i32 * iry) + ((obj_height as i32 / 2) << 8);

                // Check bounds in texture space (fixed-point).
                if tex_x < 0
                    || tex_y < 0
                    || tex_x >= (obj_width as i32) << 8
                    || tex_y >= (obj_height as i32) << 8
                {
                    continue;
                }

                // Convert from 8.8 fixed-point to integer pixel coordinates.
                let pixel_col = (tex_x >> 8) as u32;
                let pixel_row = (tex_y >> 8) as u32;

                let palette_index = fetch_obj_pixel(
                    tile_id,
                    obj_width,
                    pixel_col,
                    pixel_row,
                    is_8bpp,
                    obj_mapping_1d,
                    vram,
                    obj_vram_base,
                );

                if palette_index == 0 {
                    continue;
                }

                if is_obj_window {
                    result.obj_window[sx] = true;
                    continue;
                }

                if result.pixels[sx].opaque {
                    continue;
                }

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
        } else {
            // Regular (non-affine) OBJ rendering.
            // Flip flags (non-affine only; for affine, bits 12-13 are rotation group).
            let h_flip = (attr1 >> 12) & 1 != 0;
            let v_flip = (attr1 >> 13) & 1 != 0;

            let sample_rel_y = if mosaic_enabled {
                super::mosaic_anchor(rel_y, obj_mosaic_v)
            } else {
                rel_y
            };
            let sprite_row = if v_flip {
                obj_height - 1 - sample_rel_y
            } else {
                sample_rel_y
            };

            for sprite_col in 0..obj_width {
                let screen_x = obj_x + sprite_col as i32;
                if screen_x < 0 || screen_x >= SCREEN_WIDTH as i32 {
                    continue;
                }
                let sx = screen_x as usize;

                let sample_col = if mosaic_enabled {
                    super::mosaic_anchor(sprite_col, obj_mosaic_h)
                } else {
                    sprite_col
                };
                let pixel_col = if h_flip {
                    obj_width - 1 - sample_col
                } else {
                    sample_col
                };

                let palette_index = fetch_obj_pixel(
                    tile_id,
                    obj_width,
                    pixel_col,
                    sprite_row,
                    is_8bpp,
                    obj_mapping_1d,
                    vram,
                    obj_vram_base,
                );

                if palette_index == 0 {
                    continue;
                }

                if is_obj_window {
                    result.obj_window[sx] = true;
                    continue;
                }

                if result.pixels[sx].opaque {
                    continue;
                }

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
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OBJ VRAM base offset within the 96KB VRAM (charblock 4).
    const OBJ_VRAM_BASE: usize = 0x1_0000;

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
        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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
        let result = render_obj_scanline(5, &oam, &vram, &pram, true, false, 0, u32::MAX);

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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

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
        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
        assert_eq!(
            result.pixels[0].color, 0x03E0,
            "v-flip: scanline 0 should show row 7 data (green)"
        );

        // Scanline 7 should show original row 0 (index 1, red).
        let result7 = render_obj_scanline(7, &oam, &vram, &pram, true, false, 0, u32::MAX);
        assert_eq!(
            result7.pixels[0].color, 0x001F,
            "v-flip: scanline 7 should show row 0 data (red)"
        );
    }

    #[test]
    fn obj_mosaic_repeats_upper_left_anchor_pixel() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // OBJ 0: 8x8 at (0,0), mosaic enabled.
        write_obj(&mut oam, 0, 1 << 12, 0, 0);

        // Palette entries 1..4 are distinct colors.
        set_obj_color(&mut pram, 1, 0x001F);
        set_obj_color(&mut pram, 2, 0x03E0);
        set_obj_color(&mut pram, 3, 0x7C00);
        set_obj_color(&mut pram, 4, 0x7FFF);

        // Tile 0 rows have distinct pixels; pixel 2 verifies the next
        // horizontal mosaic block uses a new anchor.
        vram[OBJ_VRAM_BASE] = 0x21;
        vram[OBJ_VRAM_BASE + 1] = 0x32;
        vram[OBJ_VRAM_BASE + 4] = 0x43;

        // OBJ mosaic H/V sizes are both 2 pixels.
        let result = render_obj_scanline(1, &oam, &vram, &pram, true, false, 0x1100, u32::MAX);

        assert_eq!(result.pixels[0].color, 0x001F);
        assert_eq!(result.pixels[1].color, 0x001F);
        assert_eq!(result.pixels[2].color, 0x03E0);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

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
        let result = render_obj_scanline(8, &oam, &vram, &pram, false, false, 0, u32::MAX);
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
        let result = render_obj_scanline(8, &oam, &vram, &pram, true, false, 0, u32::MAX);
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
        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
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

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);
        // First 8 pixels should be opaque (tile 0).
        assert!(result.pixels[0].opaque);
        assert!(result.pixels[7].opaque);
    }

    /// Helper: write affine parameters for a rotation group into OAM.
    fn write_affine_params(oam: &mut [u8], group: usize, pa: i16, pb: i16, pc: i16, pd: i16) {
        let base = group * 32;
        oam[base + 6..base + 8].copy_from_slice(&pa.to_le_bytes());
        oam[base + 14..base + 16].copy_from_slice(&pb.to_le_bytes());
        oam[base + 22..base + 24].copy_from_slice(&pc.to_le_bytes());
        oam[base + 30..base + 32].copy_from_slice(&pd.to_le_bytes());
    }

    #[test]
    fn read_affine_params_extracts_from_oam() {
        let mut oam = make_oam();
        // Group 2: PA=0x0100 (1.0), PB=0xFF00 (-1.0), PC=0x0080 (0.5), PD=0x0200 (2.0)
        write_affine_params(&mut oam, 2, 0x0100, -0x0100, 0x0080, 0x0200);
        let (pa, pb, pc, pd) = super::read_affine_params(&oam, 2);
        assert_eq!(pa, 0x0100);
        assert_eq!(pb, -0x0100);
        assert_eq!(pc, 0x0080);
        assert_eq!(pd, 0x0200);
    }

    #[test]
    fn affine_identity_renders_same_as_regular() {
        // An affine sprite with identity matrix should produce the same output
        // as a regular sprite at the same position.
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Set palette color 1 to a known value at OBJ palette offset.
        pram[0x200 + 2] = 0x1F; // blue=31
        pram[0x200 + 3] = 0x00;

        // Fill tile 0 with palette index 1.
        fill_4bpp_tile(&mut vram, 0, 1);

        // Regular sprite: 8x8 at (10, 0), obj_mode=0
        write_obj(&mut oam, 0, 0x0000, 10, 0);
        let regular = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

        // Affine sprite: 8x8 at (10, 0), obj_mode=1, rotation group 0, identity matrix
        let mut oam2 = make_oam();
        // attr0: obj_mode=1 (bits 8-9 = 01), y=0
        // attr1: rotation group 0 (bits 9-13), x=10
        write_obj(&mut oam2, 0, 0x0100, 10, 0);
        write_affine_params(&mut oam2, 0, 0x0100, 0, 0, 0x0100); // identity

        let affine = render_obj_scanline(0, &oam2, &vram, &pram, true, false, 0, u32::MAX);

        // Both should render opaque pixels at x=10..17
        for x in 10..18 {
            assert_eq!(
                regular.pixels[x].opaque, affine.pixels[x].opaque,
                "mismatch at x={x}"
            );
            if regular.pixels[x].opaque {
                assert_eq!(regular.pixels[x].color, affine.pixels[x].color);
            }
        }
    }

    #[test]
    fn affine_2x_scale_renders_larger() {
        // A 2× scale (PA=PD=0x0080 = 0.5 in 8.8) makes the sprite appear
        // twice as large on screen (inverse of scale factor).
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        pram[0x200 + 2] = 0x1F;
        pram[0x200 + 3] = 0x00;
        fill_4bpp_tile(&mut vram, 0, 1);

        // 8x8 affine sprite with double-size (obj_mode=3) at (0,0), group 0.
        // attr0: obj_mode=3 (bits 8-9 = 11), y=0 → 0x0300
        // attr1: group 0 (bits 9-13 = 0), x=0 → 0x0000
        write_obj(&mut oam, 0, 0x0300, 0, 0);
        // Scale 0.5 means the texture is sampled at half rate → 2× zoom.
        write_affine_params(&mut oam, 0, 0x0080, 0, 0, 0x0080);

        // Double-size 8x8 gives bounding box 16x16. At scanline 0:
        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

        // With 2× scaling and double-size bounding box (16×16), the 8×8 texture
        // should be stretched to cover roughly 16 pixels horizontally.
        // Pixels at x=0..15 should be mostly opaque (texture maps to center).
        let opaque_count: usize = (0..16).filter(|&x| result.pixels[x].opaque).count();
        assert!(
            opaque_count >= 14,
            "Expected most of 16 pixels opaque, got {opaque_count}"
        );
    }

    #[test]
    fn affine_obj_window_sets_mask() {
        // An affine sprite in OBJ Window gfx mode should set the obj_window mask.
        let mut oam = make_oam();
        let mut vram = make_vram();
        let pram = make_pram();

        // Hide all OBJ entries first (obj_mode=2 = hidden).
        for i in 0..128 {
            write_obj(&mut oam, i, 0x0200, 0, 0);
        }

        fill_4bpp_tile(&mut vram, 0, 1);

        // attr0: obj_mode=1 (affine), gfx_mode=2 (OBJ window), y=0
        // obj_mode=1 → bits 8-9 = 01, gfx_mode=2 → bits 10-11 = 10
        // 0x0100 | 0x0800 = 0x0900
        write_obj(&mut oam, 0, 0x0900, 0, 0);
        write_affine_params(&mut oam, 0, 0x0100, 0, 0, 0x0100);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, u32::MAX);

        // OBJ Window mask should be set for the sprite area.
        assert!(result.obj_window[0]);
        assert!(result.obj_window[7]);
        // But pixels should NOT be opaque (OBJ Window doesn't draw).
        assert!(!result.pixels[0].opaque);
    }

    #[test]
    fn affine_double_size_extends_bounding_box() {
        // Double-size (obj_mode=3) should check a 2× bounding box for scanline
        // intersection, allowing the sprite to be visible on more scanlines.
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        pram[0x200 + 2] = 0x1F;
        pram[0x200 + 3] = 0x00;
        fill_4bpp_tile(&mut vram, 0, 1);

        // 8×8 double-size at y=0: bounding box is 16×16.
        write_obj(&mut oam, 0, 0x0300, 0, 0);
        write_affine_params(&mut oam, 0, 0x0100, 0, 0, 0x0100); // identity

        // Scanline 12 is within 16-tall bounding box but outside original 8-tall.
        let result = render_obj_scanline(12, &oam, &vram, &pram, true, false, 0, u32::MAX);

        // With identity transform on double-size, the texture (8×8) is centered
        // in the 16×16 box. Row 12 maps to texture row 12-4=8 which is OOB → transparent.
        // But rows 4..11 should be visible. Let's test row 6:
        let result6 = render_obj_scanline(6, &oam, &vram, &pram, true, false, 0, u32::MAX);
        let opaque_count: usize = (0..16).filter(|&x| result6.pixels[x].opaque).count();
        assert!(opaque_count > 0, "Should have opaque pixels at scanline 6");

        // Row 12 should be all transparent (texture y = (12-8) + 4 = 8 which is OOB).
        let opaque_at_12: usize = (0..16).filter(|&x| result.pixels[x].opaque).count();
        assert_eq!(
            opaque_at_12, 0,
            "Row 12 should be transparent for identity 8x8"
        );
    }

    #[test]
    fn affine_obj_mosaic_anchors_are_applied_before_affine_transform() {
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // 8x8 affine OBJ at (0,0), mosaic enabled.
        write_obj(&mut oam, 0, 0x1100, 0, 0);
        // Shear in X from Y so vertical anchoring affects transformed X.
        write_affine_params(&mut oam, 0, 0x0100, 0x0100, 0x0000, 0x0100);

        // Row 0: x=2 -> idx1, x=3 -> idx2, x=4 -> idx3.
        vram[OBJ_VRAM_BASE + 1] = 0x21;
        vram[OBJ_VRAM_BASE + 2] = 0x03;
        // Row 1 differs to ensure vertical anchoring is observable.
        vram[OBJ_VRAM_BASE + 4 + 1] = 0x54;

        set_obj_color(&mut pram, 1, 0x001F);
        set_obj_color(&mut pram, 2, 0x03E0);
        set_obj_color(&mut pram, 3, 0x7C00);
        set_obj_color(&mut pram, 4, 0x7FFF);
        set_obj_color(&mut pram, 5, 0x03FF);

        let result = render_obj_scanline(1, &oam, &vram, &pram, true, false, 0x1100, u32::MAX);

        assert_eq!(result.pixels[6].color, 0x001F);
        assert_eq!(result.pixels[7].color, 0x001F);
    }

    #[test]
    fn fetch_obj_pixel_4bpp_returns_correct_index() {
        let mut vram = make_vram();
        // Tile 0: set pixel (3,2) to palette index 5.
        // 4bpp: byte at tile_base + row*4 + col/2, high/low nibble.
        let addr = OBJ_VRAM_BASE + 2 * 4 + 3 / 2; // row 2, col 3 → byte at offset 9
        // col 3 is odd → high nibble
        vram[addr] = 0x50; // low nibble=0, high nibble=5

        let idx = fetch_obj_pixel(0, 8, 3, 2, false, true, &vram, OBJ_VRAM_BASE);
        assert_eq!(idx, 5);
    }

    #[test]
    fn fetch_obj_pixel_8bpp_returns_correct_index() {
        let mut vram = make_vram();
        // Tile 0 in 8bpp: 64 bytes/tile. Pixel (5, 3) = byte at offset row*8+col.
        let addr = OBJ_VRAM_BASE + 3 * 8 + 5;
        vram[addr] = 42;

        let idx = fetch_obj_pixel(0, 8, 5, 3, true, true, &vram, OBJ_VRAM_BASE);
        assert_eq!(idx, 42);
    }

    // --- Cycle budget tests (TDD RED phase) ---

    #[test]
    fn cycle_budget_stops_rendering_when_exhausted() {
        // OBJ 0: 8x8 at (0, 0) — costs 8 cycles; OBJ 1: 8x8 at (10, 0) — costs 8
        // cycles. Budget of 10 allows OBJ 0 (total 8) but not OBJ 1 (would be 16).
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        write_obj(&mut oam, 0, 0, 0, 0); // 8x8 at (x=0, y=0)
        write_obj(&mut oam, 1, 0, 10, 1); // 8x8 at (x=10, y=0)
        fill_4bpp_tile(&mut vram, 0, 1);
        fill_4bpp_tile(&mut vram, 1, 2);
        set_obj_color(&mut pram, 1, 0x001F); // red
        set_obj_color(&mut pram, 2, 0x03E0); // green

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, 10);

        // OBJ 0 should be rendered (8 cycles ≤ budget of 10).
        assert!(result.pixels[0].opaque, "OBJ 0 should render within budget");
        // OBJ 1 should NOT be rendered (8+8=16 > 10).
        assert!(
            !result.pixels[10].opaque,
            "OBJ 1 should be skipped after budget exhausted"
        );
    }

    #[test]
    fn cycle_budget_counts_off_scanline_obj() {
        // OBJ 0: 8x8 at y=100 (off scanline 0) — still costs 8 cycles.
        // OBJ 1: 8x8 at y=0 (on scanline 0) — costs 8 cycles.
        // Budget of 10: OBJ 0 consumes 8 cycles (no pixels), OBJ 1 can't fit (16 > 10).
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        write_obj(&mut oam, 0, 100, 0, 0); // 8x8 at (x=0, y=100) — off scanline 0
        write_obj(&mut oam, 1, 0, 10, 1); // 8x8 at (x=10, y=0) — on scanline 0
        fill_4bpp_tile(&mut vram, 0, 1);
        fill_4bpp_tile(&mut vram, 1, 2);
        set_obj_color(&mut pram, 1, 0x001F);
        set_obj_color(&mut pram, 2, 0x03E0);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, 10);

        // OBJ 1 should NOT render; its budget was consumed by off-scanline OBJ 0.
        assert!(
            !result.pixels[10].opaque,
            "OBJ 1 should not render: budget consumed by off-scanline OBJ 0"
        );
    }

    #[test]
    fn cycle_budget_affine_cost_is_10_plus_2n() {
        // Affine 8x8 OBJ costs 10 + 8*2 = 26 cycles.
        // With budget=25 it should NOT render; with budget=26 it should.
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        write_obj(&mut oam, 0, 0x0100, 0, 0); // attr0 obj_mode=1 (affine), y=0
        write_affine_params(&mut oam, 0, 0x0100, 0, 0, 0x0100); // identity
        fill_4bpp_tile(&mut vram, 0, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        // Budget too small by 1: should not render.
        let result_no = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, 25);
        assert!(
            !result_no.pixels[0].opaque,
            "Affine OBJ should not render with budget=25 (cost=26)"
        );

        // Budget exactly matching: should render.
        let result_yes = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, 26);
        assert!(
            result_yes.pixels[0].opaque,
            "Affine OBJ should render with budget=26 (cost=26)"
        );
    }

    #[test]
    fn cycle_budget_hidden_obj_does_not_consume_cycles() {
        // OBJ 0: hidden (mode=2) — should NOT consume cycles.
        // OBJ 1: 8x8 at (0, 0) — costs 8 cycles.
        // Budget=8: OBJ 0 doesn't consume, OBJ 1 renders.
        let mut oam = make_oam();
        let mut vram = make_vram();
        let mut pram = make_pram();

        write_obj(&mut oam, 0, 2 << 8, 0, 0); // hidden (obj_mode=2)
        write_obj(&mut oam, 1, 0, 0, 1); // 8x8 at (x=0, y=0)
        fill_4bpp_tile(&mut vram, 1, 1);
        set_obj_color(&mut pram, 1, 0x001F);

        let result = render_obj_scanline(0, &oam, &vram, &pram, true, false, 0, 8);

        assert!(
            result.pixels[0].opaque,
            "OBJ 1 should render: hidden OBJ 0 did not consume cycles"
        );
    }
}
