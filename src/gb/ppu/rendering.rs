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
}
