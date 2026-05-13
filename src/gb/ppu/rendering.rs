/// DMG grey shades indexed by colour index (0=white, 3=black).
const DMG_GREY: [u8; 4] = [0xFF, 0xAA, 0x55, 0x00];

/// Extract a 2-bit palette colour from a DMG palette register.
///
/// `palette_reg` — raw BGP / OBP0 / OBP1 value
/// `colour_index` — 2-bit index (0–3)
pub(crate) fn dmg_palette_index(palette_reg: u8, colour_index: u8) -> u8 {
    (palette_reg >> (colour_index * 2)) & 0x03
}

pub(crate) fn dmg_grey(shade: u8) -> u8 {
    DMG_GREY[shade as usize]
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
pub(crate) fn cgb_palette_lookup(
    palette_ram: &[u8; 64],
    palette_num: u8,
    colour_index: u8,
) -> (u8, u8, u8) {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── palette_lookup ────────────────────────────────────────────────────────

    #[test]
    fn test_palette_index_0_maps_to_white_with_default_bgp() {
        // BGP = 0xE4 (0b11100100): colour 0→0 (white), 1→1, 2→2, 3→3
        assert_eq!(dmg_grey(dmg_palette_index(0xE4, 0)), DMG_GREY[0]);
    }

    #[test]
    fn test_palette_index_3_maps_to_black_with_default_bgp() {
        // BGP = 0xE4: colour 3 maps to shade 3 (black)
        assert_eq!(dmg_grey(dmg_palette_index(0xE4, 3)), DMG_GREY[3]);
    }

    #[test]
    fn test_palette_inverted_bgp_maps_0_to_black() {
        // BGP = 0x1B (0b00011011): colour 0→3 (black)
        assert_eq!(dmg_grey(dmg_palette_index(0x1B, 0)), DMG_GREY[3]);
    }

    #[test]
    fn test_palette_all_white_bgp_maps_every_index_to_white() {
        // BGP = 0x00: all colours map to shade 0 (white)
        for i in 0..4 {
            assert_eq!(dmg_grey(dmg_palette_index(0x00, i)), DMG_GREY[0]);
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
}
