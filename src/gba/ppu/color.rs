//! GBA palette color conversion.
//!
//! GBA palette entries are stored as 15-bit BGR555 halfwords:
//! `0bbbbbgggggrrrrr`. The frontend expects 24-bit RGB888 packed as
//! `[R, G, B]`. Each 5-bit channel is widened to 8 bits using the
//! standard "replicate top three bits into the low bits" formula:
//!
//! ```text
//! c8 = (c5 << 3) | (c5 >> 2)
//! ```
//!
//! This makes `0x00 → 0x00`, `0x1F → 0xFF` and is monotonic across the
//! full range.
//!
//! Reference: GBATek, "LCD Color Palettes".
//! <https://problemkaputt.de/gbatek.htm#lcdcolorpalettes>
//!
//! Since BGR555 stores the components in the order Blue (bits 10..14),
//! Green (5..9), Red (0..4) the conversion swaps to RGB byte order on
//! the way out.

/// Expand a 5-bit color channel to 8 bits using bit replication.
///
/// `c8 = (c5 << 3) | (c5 >> 2)` — the canonical formula required by the
/// acceptance criteria. The top three bits of the source are duplicated
/// into the low three bits of the result so that `0x1F` maps to `0xFF`.
#[inline]
pub fn expand5_to_8(c5: u8) -> u8 {
    let c5 = c5 & 0x1F;
    (c5 << 3) | (c5 >> 2)
}

/// Convert a 15-bit BGR555 GBA palette entry to a 24-bit RGB888
/// triple `(r, g, b)`. The high bit of the 16-bit input is ignored.
#[inline]
pub fn bgr555_to_rgb888(bgr555: u16) -> (u8, u8, u8) {
    let r5 = (bgr555 & 0x1F) as u8;
    let g5 = ((bgr555 >> 5) & 0x1F) as u8;
    let b5 = ((bgr555 >> 10) & 0x1F) as u8;
    (expand5_to_8(r5), expand5_to_8(g5), expand5_to_8(b5))
}

/// Write a single BGR555 pixel into an RGB888 byte buffer at byte
/// offset `dst`. Three consecutive bytes are written: `[R, G, B]`.
///
/// # Panics
///
/// Panics if `dst + 3 > buf.len()`.
#[inline]
pub fn write_pixel(buf: &mut [u8], dst: usize, bgr555: u16) {
    let (r, g, b) = bgr555_to_rgb888(bgr555);
    buf[dst] = r;
    buf[dst + 1] = g;
    buf[dst + 2] = b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand5_endpoints_map_to_endpoints() {
        // The acceptance criteria require c8 = (c5 << 3) | (c5 >> 2).
        assert_eq!(expand5_to_8(0x00), 0x00);
        assert_eq!(expand5_to_8(0x1F), 0xFF);
    }

    #[test]
    fn expand5_matches_canonical_formula() {
        for c5 in 0u8..=0x1F {
            let expected = (c5 << 3) | (c5 >> 2);
            assert_eq!(expand5_to_8(c5), expected, "c5={c5:#x}");
        }
    }

    #[test]
    fn expand5_ignores_high_bits() {
        // Values >= 0x20 must be masked to 5 bits.
        assert_eq!(expand5_to_8(0x20), expand5_to_8(0x00));
        assert_eq!(expand5_to_8(0x3F), expand5_to_8(0x1F));
    }

    #[test]
    fn expand5_is_monotonic() {
        for c5 in 0u8..0x1F {
            assert!(expand5_to_8(c5) <= expand5_to_8(c5 + 1));
        }
    }

    #[test]
    fn bgr555_black_and_white() {
        assert_eq!(bgr555_to_rgb888(0x0000), (0, 0, 0));
        assert_eq!(bgr555_to_rgb888(0x7FFF), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn bgr555_pure_red() {
        // r5 = 0x1F, others zero. BGR555: 0x001F.
        assert_eq!(bgr555_to_rgb888(0x001F), (0xFF, 0, 0));
    }

    #[test]
    fn bgr555_pure_green() {
        // g5 = 0x1F at bits 5..=9 → 0x03E0.
        assert_eq!(bgr555_to_rgb888(0x03E0), (0, 0xFF, 0));
    }

    #[test]
    fn bgr555_pure_blue() {
        // b5 = 0x1F at bits 10..=14 → 0x7C00.
        assert_eq!(bgr555_to_rgb888(0x7C00), (0, 0, 0xFF));
    }

    #[test]
    fn bgr555_ignores_high_bit() {
        // Bit 15 is unused on real hardware.
        assert_eq!(bgr555_to_rgb888(0x8000), (0, 0, 0));
        assert_eq!(bgr555_to_rgb888(0xFFFF), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn bgr555_mid_grey_uses_replication() {
        // c5 = 0x10 → c8 = (0x10 << 3) | (0x10 >> 2) = 0x80 | 0x04 = 0x84.
        // BGR555 value with all three channels at 0x10.
        let bgr = 0x10u16 | (0x10u16 << 5) | (0x10u16 << 10);
        assert_eq!(bgr555_to_rgb888(bgr), (0x84, 0x84, 0x84));
    }

    #[test]
    fn write_pixel_emits_rgb_byte_order() {
        let mut buf = [0u8; 6];
        write_pixel(&mut buf, 0, 0x001F); // red
        write_pixel(&mut buf, 3, 0x7C00); // blue
        assert_eq!(buf, [0xFF, 0, 0, 0, 0, 0xFF]);
    }
}
