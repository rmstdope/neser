use crate::trace_ppu;

use super::background;
use super::registers::Registers;
use super::screen_buffer::ScreenBuffer;
use super::sprites;
use super::window;

/// DMG grey shades indexed by colour index (0=white, 3=black).
const DMG_GREY: [u8; 4] = [0xFF, 0xAA, 0x55, 0x00];

/// Extract a 2-bit palette colour from a DMG palette register.
///
/// `palette_reg` — raw BGP / OBP0 / OBP1 value
/// `colour_index` — 2-bit index (0–3)
pub fn palette_lookup(palette_reg: u8, colour_index: u8) -> u8 {
    let shade = (palette_reg >> (colour_index * 2)) & 0x03;
    DMG_GREY[shade as usize]
}

/// Render one full scanline (0–143) into `screen_buffer`.
///
/// `window_line` is the internal window-line counter; it is incremented here
/// whenever the window contributes to the current scanline.
pub fn render_scanline(
    scanline: u8,
    vram: &[u8; 0x2000],
    oam: &[u8; 0xA0],
    registers: &Registers,
    window_line: &mut u8,
    screen_buffer: &mut ScreenBuffer,
) {
    let lcdc = registers.lcdc;
    let bg_window_enabled = lcdc & 0x01 != 0;
    let obj_enabled = lcdc & 0x02 != 0;
    let win_enabled = lcdc & 0x20 != 0;

    let sprite_indices = if obj_enabled {
        sprites::scan_oam_line(scanline, oam, lcdc)
    } else {
        Vec::new()
    };

    let mut window_active = false;

    for x in 0..ScreenBuffer::WIDTH {
        // BG/Window colour index.
        let bg_idx = if bg_window_enabled {
            background::fetch_bg_pixel(x, scanline, vram, lcdc, registers.scx, registers.scy)
        } else {
            0 // BG disabled → colour 0 (white on DMG)
        };

        // Window may override BG.
        let bw_idx = if win_enabled {
            match window::fetch_window_pixel(
                x,
                scanline,
                vram,
                lcdc,
                registers.wx,
                registers.wy,
                *window_line,
            ) {
                Some(idx) => {
                    window_active = true;
                    idx
                }
                None => bg_idx,
            }
        } else {
            bg_idx
        };

        // Sprite pixel (highest-priority non-transparent sprite at this column).
        let sprite_px = sprites::fetch_sprite_pixel(x, scanline, &sprite_indices, oam, vram, lcdc);

        // Compose: sprite wins unless it is behind BG colours 1–3.
        let (final_idx, is_sprite, sprite_pal) = if let Some(sp) = sprite_px {
            if sp.bg_priority && bw_idx != 0 {
                (bw_idx, false, 0u8)
            } else {
                (sp.colour_index, true, sp.palette)
            }
        } else {
            (bw_idx, false, 0u8)
        };

        let grey = if is_sprite {
            let pal = if sprite_pal == 0 {
                registers.obp0
            } else {
                registers.obp1
            };
            palette_lookup(pal, final_idx)
        } else {
            palette_lookup(registers.bgp, final_idx)
        };

        screen_buffer.set_pixel(x, scanline as u32, grey, grey, grey);
    }

    // Level 4: Per-scanline rendering summary
    // For simplicity, we trace sprite count and window active flag.
    // Tile count would require tracking unique tiles, which adds complexity.
    trace_ppu!(4; "render y={} sprites={} window={}",
        scanline, sprite_indices.len(), window_active);

    if window_active {
        *window_line = window_line.wrapping_add(1);
    }
}

/// Convert a 5-bit CGB palette component to 8-bit.
///
/// Uses the formula specified by the cgb-acid2 test: `(c5 << 3) | (c5 >> 2)`.
#[inline]
fn cgb_5bit_to_8bit(c5: u8) -> u8 {
    (c5 << 3) | (c5 >> 2)
}

/// Look up an RGB color from CGB palette RAM.
///
/// `palette_ram` — 64-byte palette RAM (8 palettes × 4 colors × 2 bytes, 5-5-5 LE).
/// `palette_num` — Palette index (0–7).
/// `colour_index` — Color slot within the palette (0–3; 0 is transparent for OBJ).
///
/// Returns `(r8, g8, b8)` with 8-bit components.
fn cgb_palette_lookup(palette_ram: &[u8; 64], palette_num: u8, colour_index: u8) -> (u8, u8, u8) {
    let base = (palette_num as usize) * 8 + (colour_index as usize) * 2;
    let lo = palette_ram[base];
    let hi = palette_ram[base + 1];
    let color = u16::from_le_bytes([lo, hi]);
    let r5 = (color & 0x1F) as u8;
    let g5 = ((color >> 5) & 0x1F) as u8;
    let b5 = ((color >> 10) & 0x1F) as u8;
    (
        cgb_5bit_to_8bit(r5),
        cgb_5bit_to_8bit(g5),
        cgb_5bit_to_8bit(b5),
    )
}

/// Render one full CGB scanline (0–143) into `screen_buffer`.
///
/// Implements CGB compositing rules:
/// - `LCDC bit 0` is the **master priority** flag (not BG enable as in DMG).
///   - 0: OBJ pixels always win over BG/Window pixels.
///   - 1: Normal priority — BG tile priority bit and/or OBJ priority bit can
///     cause BG to appear on top when the BG colour index is non-zero.
/// - OBJ priority is determined by OAM order when `opri_dmg_mode` is false
///   (CGB default), or by X-coordinate when true.
/// - When `dmg_compat` is true (DMG-only game on CGB hardware), OBJ palette
///   selection uses OAM attribute bit 4 (0=palette 0, 1=palette 1) instead of
///   CGB attribute bits 0-2.
#[allow(clippy::too_many_arguments)]
pub fn render_scanline_cgb(
    scanline: u8,
    vram: &[u8; 0x2000],
    vram_bank1: &[u8; 0x2000],
    oam: &[u8; 0xA0],
    registers: &Registers,
    bg_palette_ram: &[u8; 64],
    obj_palette_ram: &[u8; 64],
    window_line: &mut u8,
    opri_dmg_mode: bool,
    dmg_compat: bool,
    screen_buffer: &mut ScreenBuffer,
) {
    let lcdc = registers.lcdc;
    let obj_enabled = lcdc & 0x02 != 0;
    let win_enabled = lcdc & 0x20 != 0;
    // In CGB mode, LCDC bit 0 is master priority (not BG enable).
    let master_priority = lcdc & 0x01 != 0;

    let sprite_indices = if obj_enabled {
        sprites::scan_oam_line(scanline, oam, lcdc)
    } else {
        Vec::new()
    };

    let mut window_active = false;

    for x in 0..ScreenBuffer::WIDTH {
        // Fetch BG pixel (always rendered in CGB mode regardless of LCDC bit 0).
        let bg_px = background::fetch_bg_pixel_cgb(
            x,
            scanline,
            vram,
            vram_bank1,
            lcdc,
            registers.scx,
            registers.scy,
        );

        // Window may override BG.
        let bw_px = if win_enabled {
            match window::fetch_window_pixel_cgb(
                x,
                scanline,
                vram,
                vram_bank1,
                lcdc,
                registers.wx,
                registers.wy,
                *window_line,
            ) {
                Some(win_px) => {
                    window_active = true;
                    win_px
                }
                None => bg_px,
            }
        } else {
            bg_px
        };

        // Fetch OBJ pixel.
        let sprite_px = if obj_enabled {
            sprites::fetch_sprite_pixel_cgb(
                x,
                scanline,
                &sprite_indices,
                oam,
                vram,
                vram_bank1,
                lcdc,
                opri_dmg_mode,
            )
        } else {
            None
        };

        // CGB compositing:
        // - If no OBJ pixel → render BG/Window.
        // - If BG colour index == 0 → OBJ wins.
        // - If master priority == 0 → OBJ always wins.
        // - If master priority == 1 and (BG tile priority or OBJ priority) and BG colour != 0 → BG wins.
        // - Otherwise OBJ wins.
        let (r, g, b) = if let Some(sp) = sprite_px {
            let bg_wins =
                bw_px.colour_index != 0 && master_priority && (bw_px.bg_priority || sp.bg_priority);
            if bg_wins {
                cgb_palette_lookup(bg_palette_ram, bw_px.palette_num, bw_px.colour_index)
            } else {
                // In DMG-compat mode, use OAM attribute bit 4 (sp.palette: 0=OBP0, 1=OBP1)
                // instead of CGB attribute bits 0-2 (sp.cgb_palette).
                let obj_pal = if dmg_compat {
                    sp.palette
                } else {
                    sp.cgb_palette
                };
                cgb_palette_lookup(obj_palette_ram, obj_pal, sp.colour_index)
            }
        } else {
            cgb_palette_lookup(bg_palette_ram, bw_px.palette_num, bw_px.colour_index)
        };

        screen_buffer.set_pixel(x, scanline as u32, r, g, b);
    }

    // Level 4: Per-scanline rendering summary
    trace_ppu!(4; "render y={} sprites={} window={}",
        scanline, sprite_indices.len(), window_active);

    if window_active {
        *window_line = window_line.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_registers() -> Registers {
        Registers::new()
    }

    fn blank_vram() -> [u8; 0x2000] {
        [0u8; 0x2000]
    }

    fn blank_oam() -> [u8; 0xA0] {
        [0u8; 0xA0]
    }

    // ── palette_lookup ────────────────────────────────────────────────────────

    #[test]
    fn test_palette_index_0_maps_to_white_with_default_bgp() {
        // BGP = 0xE4 (0b11100100): colour 0→0 (white), 1→1, 2→2, 3→3
        assert_eq!(palette_lookup(0xE4, 0), DMG_GREY[0]); // white = 0xFF
    }

    #[test]
    fn test_palette_index_3_maps_to_black_with_default_bgp() {
        // BGP = 0xE4: colour 3 maps to shade 3 (black)
        assert_eq!(palette_lookup(0xE4, 3), DMG_GREY[3]); // black = 0x00
    }

    #[test]
    fn test_palette_inverted_bgp_maps_0_to_black() {
        // BGP = 0x1B (0b00011011): colour 0→3 (black)
        assert_eq!(palette_lookup(0x1B, 0), DMG_GREY[3]);
    }

    #[test]
    fn test_palette_all_white_bgp_maps_every_index_to_white() {
        // BGP = 0x00: all colours map to shade 0 (white)
        for i in 0..4 {
            assert_eq!(palette_lookup(0x00, i), DMG_GREY[0]);
        }
    }

    // ── render_scanline ───────────────────────────────────────────────────────

    #[test]
    fn test_render_scanline_fills_160_pixels() {
        // Given: blank VRAM/OAM, default registers, fresh screen buffer
        let vram = blank_vram();
        let oam = blank_oam();
        let regs = default_registers();
        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;
        // When: render scanline 0
        render_scanline(0, &vram, &oam, &regs, &mut wl, &mut sb);
        // Then: all 160 pixels in row 0 have been written (non-garbage)
        // With default BGP=0xFC (colour 0→3 for bits 1:0) and blank VRAM (all index 0)
        // palette_lookup(0xFC, 0): 0xFC = 0b11111100, bits 1:0 = 0b00 = shade 0 = 0xFF
        for x in 0..160 {
            let (r, g, b) = sb.get_pixel(x, 0);
            // All should be the same (all-blank BG)
            assert_eq!(r, g, "pixel ({x},0) r≠g");
            assert_eq!(g, b, "pixel ({x},0) g≠b");
        }
    }

    #[test]
    fn test_render_scanline_with_inverted_bgp_paints_black() {
        // Given: BGP = 0xFF (colour 0 → shade 3 = black)
        let vram = blank_vram();
        let oam = blank_oam();
        let mut regs = default_registers();
        regs.bgp = 0xFF; // all colours → shade 3 (0x00)
        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;
        // When: render scanline 0
        render_scanline(0, &vram, &oam, &regs, &mut wl, &mut sb);
        // Then: every pixel in row 0 is (0x00, 0x00, 0x00) = black
        for x in 0..160 {
            assert_eq!(
                sb.get_pixel(x, 0),
                (0x00, 0x00, 0x00),
                "pixel ({x},0) should be black"
            );
        }
    }

    #[test]
    fn test_render_scanline_bg_disabled_paints_white() {
        // Given: LCDC bit 0 clear = BG/Window disabled → all pixels colour 0 shade 0 = white
        // (On DMG, disabling BG makes it all white regardless of tile data)
        let vram = blank_vram();
        let oam = blank_oam();
        let mut regs = default_registers();
        regs.lcdc = 0x80; // LCD on, BG off
        regs.bgp = 0xE4; // standard palette
        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;
        render_scanline(0, &vram, &oam, &regs, &mut wl, &mut sb);
        for x in 0..160 {
            assert_eq!(
                sb.get_pixel(x, 0),
                (0xFF, 0xFF, 0xFF),
                "pixel ({x},0) should be white when BG disabled"
            );
        }
    }

    // ── CGB rendering helper tests ────────────────────────────────────────────

    #[test]
    fn test_cgb_5bit_to_8bit_known_values() {
        assert_eq!(cgb_5bit_to_8bit(0), 0);
        assert_eq!(cgb_5bit_to_8bit(31), 255); // (31<<3)|(31>>2) = 248|7 = 255
        assert_eq!(cgb_5bit_to_8bit(1), 8); // (1<<3)|(1>>2) = 8|0 = 8
        assert_eq!(cgb_5bit_to_8bit(16), 132); // (16<<3)|(16>>2) = 128|4 = 132
    }

    #[test]
    fn test_cgb_palette_lookup_decodes_5_5_5_le() {
        // palette 0, colour 1: R=31, G=0, B=0 → lo=0x1F, hi=0x00
        let mut palette_ram = [0u8; 64];
        palette_ram[2] = 0x1F; // colour 1 lo byte: R=31 (bits 0-4)
        palette_ram[3] = 0x00; // colour 1 hi byte
        let (r, g, b) = cgb_palette_lookup(&palette_ram, 0, 1);
        assert_eq!(r, 255, "R should be max");
        assert_eq!(g, 0, "G should be 0");
        assert_eq!(b, 0, "B should be 0");
    }

    fn blank_vram_bank1() -> [u8; 0x2000] {
        [0u8; 0x2000]
    }

    fn blank_palette_ram() -> [u8; 64] {
        [0u8; 64]
    }

    /// Build palette_ram with colour `slot` in palette 0 = max white (0x7FFF in 5-5-5 LE).
    fn palette_ram_with_white_at(slot: usize) -> [u8; 64] {
        let mut p = blank_palette_ram();
        p[slot * 2] = 0xFF;
        p[slot * 2 + 1] = 0x7F;
        p
    }

    #[test]
    fn test_cgb_master_priority_off_obj_always_wins() {
        // LCDC bit 0 = 0 → master_priority = false → OBJ always wins over BG.
        // Sprite at (screen_x=0, screen_y=0); bg tile at (0,0).
        // BG palette colour 1 = white; OBJ palette colour 1 = black.
        // Even with bg_priority flag on the BG tile, OBJ should win.
        let mut vram = blank_vram();
        let mut oam = blank_oam();
        // Sprite at screen (0,0): oam_y=16, oam_x=8, tile 0, attrs=0x80 (bg_priority set)
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 0;
        oam[3] = 0x80;
        // BG tile 0 row 0: colour 1 (bank 0 attr in bank1[0x1800] has bg_priority bit set)
        vram[0x0000] = 0xFF; // tile 0 row 0 low: colour 1 for all pixels
        let mut bank1_with_bgprio = blank_vram_bank1();
        bank1_with_bgprio[0x1800] = 0x80; // BG tile attr: bg_priority
        // Sprite tile 0 row 0: black via obj_palette_ram (all zeros)
        // vram tile 0 is reused (colour 1); OBJ also colour 1 → both non-transparent

        let mut regs = default_registers();
        regs.lcdc = 0x92u8; // 1001_0010: LCD on, tile_data $8000 (bit4), sprites on (bit1), master_priority=0 (bit0=0)

        let bg_palette = palette_ram_with_white_at(1); // palette 0, colour 1 = white
        let obj_palette = blank_palette_ram(); // palette 0, colour 1 = black (all zeros)

        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;
        render_scanline_cgb(
            0,
            &vram,
            &bank1_with_bgprio,
            &oam,
            &regs,
            &bg_palette,
            &obj_palette,
            &mut wl,
            false,
            false, // dmg_compat
            &mut sb,
        );
        // master_priority=false → OBJ always wins → black
        assert_eq!(
            sb.get_pixel(0, 0),
            (0, 0, 0),
            "OBJ (black) must win when master_priority=false"
        );
    }

    #[test]
    fn test_cgb_bg_priority_beats_obj_when_master_priority_set() {
        // LCDC bit 0 = 1 → master_priority = true.
        // BG tile has bg_priority=true and colour_index != 0 → BG wins.
        // BG palette colour 1 = white; OBJ palette colour 1 = black.
        let mut vram = blank_vram();
        let mut oam = blank_oam();
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 0;
        oam[3] = 0x00; // sprite at (0,0), no priority flag
        vram[0x0000] = 0xFF; // tile 0 row 0 low: colour 1
        let mut bank1 = blank_vram_bank1();
        bank1[0x1800] = 0x80; // BG tile attr: bg_priority bit set

        let mut regs = default_registers();
        // 1001_0011: LCD on, tile data $8000 (bit4), sprites on (bit1), master_priority=1 (bit0)
        regs.lcdc = 0x93u8;

        let bg_palette = palette_ram_with_white_at(1); // colour 1 = white
        let obj_palette = blank_palette_ram(); // colour 1 = black

        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;
        render_scanline_cgb(
            0,
            &vram,
            &bank1,
            &oam,
            &regs,
            &bg_palette,
            &obj_palette,
            &mut wl,
            false,
            false, // dmg_compat
            &mut sb,
        );
        assert_eq!(
            sb.get_pixel(0, 0),
            (255, 255, 255),
            "BG (white) must win with master_priority=true and bg_priority"
        );
    }

    #[test]
    fn test_dmg_compat_uses_oam_bit4_for_obj_palette() {
        // In DMG-compat mode, sprites select OBJ palette via OAM attr bit 4 (not bits 0-2).
        // Sprite with attr bit 4 set should use OBJ palette 1, not palette 0.
        let mut vram = blank_vram();
        let mut oam = blank_oam();
        // Sprite at (0,0) with attr bit 4 set (DMG palette 1)
        oam[0] = 16; // y
        oam[1] = 8; // x
        oam[2] = 0; // tile
        oam[3] = 0x10; // attr: bit 4 set → DMG palette 1 (CGB bits 0-2 = 0)

        // Tile 0 row 0: colour index 1
        vram[0x0000] = 0xFF; // low byte
        vram[0x0001] = 0x00; // high byte

        let bank1 = blank_vram_bank1();

        let mut regs = default_registers();
        regs.lcdc = 0x92; // LCD on, sprites on, BG on

        // BG palette: colour 1 = black (default zeros)
        let bg_palette = blank_palette_ram();
        // OBJ palette 0: colour 1 = black (zeros)
        // OBJ palette 1: colour 1 = white (at bytes 10-11)
        let mut obj_palette = blank_palette_ram();
        obj_palette[8 + 2] = 0xFF; // palette 1, colour 1, low byte
        obj_palette[8 + 3] = 0x7F; // palette 1, colour 1, high byte (0x7FFF = white)

        let mut sb = ScreenBuffer::new();
        let mut wl = 0u8;

        // Without dmg_compat, CGB uses bits 0-2 → palette 0 → black
        render_scanline_cgb(
            0,
            &vram,
            &bank1,
            &oam,
            &regs,
            &bg_palette,
            &obj_palette,
            &mut wl,
            false,
            false, // dmg_compat = false
            &mut sb,
        );
        assert_eq!(
            sb.get_pixel(0, 0),
            (0, 0, 0),
            "CGB mode: attr bits 0-2 (= 0) selects palette 0 → black"
        );

        // With dmg_compat, uses attr bit 4 → palette 1 → white
        let mut sb2 = ScreenBuffer::new();
        wl = 0;
        render_scanline_cgb(
            0,
            &vram,
            &bank1,
            &oam,
            &regs,
            &bg_palette,
            &obj_palette,
            &mut wl,
            false,
            true, // dmg_compat = true
            &mut sb2,
        );
        assert_eq!(
            sb2.get_pixel(0, 0),
            (255, 255, 255),
            "DMG-compat mode: attr bit 4 selects palette 1 → white"
        );
    }
}
