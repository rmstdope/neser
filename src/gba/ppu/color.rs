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
//! ## Optional LCD Color Correction
//!
//! An optional color correction mode simulates the GBA TFT LCD's non-linear
//! physical response. Per GBATek "LCD Color Palettes", values 0–14 appear
//! nearly black on the GBA TFT display, with 15–31 giving medium to full
//! brightness. The correction applies a gamma ≈ 4 curve:
//!
//! ```text
//! corrected[c5] = c5^4 * 255 / 31^4
//! ```
//!
//! This is an optional post-processing step; the linear expansion remains the
//! default (technically correct for the digital color values).
//!
//! Reference: GBATek, "LCD Color Palettes".
//! <https://problemkaputt.de/gbatek.htm#lcdcolorpalettes>
//!
//! Since BGR555 stores the components in the order Blue (bits 10..14),
//! Green (5..9), Red (0..4) the conversion swaps to RGB byte order on
//! the way out.

/// Pre-computed 5-bit → 8-bit gamma ≈ 4 correction table for GBA LCD
/// simulation.
///
/// Approximates the GBA TFT LCD's physical response: values 0–7 map to 0
/// (black), 8–14 map to 1–10 (nearly black), and the upper range 15–31
/// covers medium through full brightness (13–255).
///
/// Formula: `c5^4 * 255 / 31^4` (integer division, no rounding).
pub const LCD_GAMMA_TABLE: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 4, 5, 7, 10, 13, 18, 23, 28, 35, 44, 53, 64, 77, 91, 107, 126,
    146, 169, 195, 223, 255,
];

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
/// Expand a 5-bit color channel to 8 bits using the GBA LCD gamma correction
/// table.
///
/// Applies the [`LCD_GAMMA_TABLE`] to simulate the GBA TFT LCD's physical
/// non-linear response. Per GBATek, values 0–14 appear nearly black on the
/// GBA TFT; this curve concentrates most visible output in the upper range
/// 15–31.
///
/// Unlike [`expand5_to_8`], this is NOT the canonical formula — use it only
/// when optional LCD color-correction is enabled.
#[inline]
pub fn expand5_to_8_corrected(c5: u8) -> u8 {
    LCD_GAMMA_TABLE[(c5 & 0x1F) as usize]
}

/// Convert a 15-bit BGR555 GBA palette entry to a 24-bit RGB888 triple using
/// GBA LCD gamma correction. See [`expand5_to_8_corrected`] for details.
#[inline]
pub fn bgr555_to_rgb888_corrected(bgr555: u16) -> (u8, u8, u8) {
    let r5 = (bgr555 & 0x1F) as u8;
    let g5 = ((bgr555 >> 5) & 0x1F) as u8;
    let b5 = ((bgr555 >> 10) & 0x1F) as u8;
    (
        expand5_to_8_corrected(r5),
        expand5_to_8_corrected(g5),
        expand5_to_8_corrected(b5),
    )
}

/// Write a single BGR555 pixel into an RGB888 byte buffer using GBA LCD gamma
/// correction. Three consecutive bytes are written at byte offset `dst`:
/// `[R, G, B]`.
///
/// # Panics
///
/// Panics if `dst + 3 > buf.len()`.
#[inline]
pub fn write_pixel_corrected(buf: &mut [u8], dst: usize, bgr555: u16) {
    let (r, g, b) = bgr555_to_rgb888_corrected(bgr555);
    buf[dst] = r;
    buf[dst + 1] = g;
    buf[dst + 2] = b;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Tests for expand5_to_8_corrected ---------------------------------

    #[test]
    fn expand5_corrected_endpoints_map_to_endpoints() {
        assert_eq!(expand5_to_8_corrected(0x00), 0x00);
        assert_eq!(expand5_to_8_corrected(0x1F), 0xFF);
    }

    #[test]
    fn expand5_corrected_ignores_high_bits() {
        assert_eq!(expand5_to_8_corrected(0x20), expand5_to_8_corrected(0x00));
        assert_eq!(expand5_to_8_corrected(0x3F), expand5_to_8_corrected(0x1F));
    }

    #[test]
    fn expand5_corrected_is_monotonic() {
        for c5 in 0u8..0x1F {
            assert!(
                expand5_to_8_corrected(c5) <= expand5_to_8_corrected(c5 + 1),
                "not monotonic at c5={c5}"
            );
        }
    }

    /// GBATek note: values 0-14 appear nearly black on GBA TFT.
    /// With gamma ≈ 4, c5=14 maps to 10 — very dark on modern displays.
    #[test]
    fn expand5_corrected_dark_range_is_nearly_black() {
        // All values 0-7 must map to 0 (black).
        for c5 in 0u8..=7 {
            assert_eq!(expand5_to_8_corrected(c5), 0, "expected 0 for c5={c5}");
        }
        // Values 8-14 are very dark (≤10).
        assert!(expand5_to_8_corrected(14) <= 10);
    }

    /// The corrected expansion must differ from the standard linear expansion
    /// for mid-range values (e.g. c5=16), confirming that the correction is
    /// actually applied.
    #[test]
    fn expand5_corrected_differs_from_linear_at_midrange() {
        let linear = expand5_to_8(16);
        let corrected = expand5_to_8_corrected(16);
        assert_ne!(
            linear, corrected,
            "corrected and linear must differ at c5=16"
        );
        assert!(
            corrected < linear,
            "corrected ({corrected}) must be darker than linear ({linear}) at c5=16"
        );
    }

    // ---- Tests for bgr555_to_rgb888_corrected -----------------------------

    #[test]
    fn bgr555_corrected_black_and_white() {
        assert_eq!(bgr555_to_rgb888_corrected(0x0000), (0, 0, 0));
        assert_eq!(bgr555_to_rgb888_corrected(0x7FFF), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn bgr555_corrected_pure_red() {
        assert_eq!(bgr555_to_rgb888_corrected(0x001F), (0xFF, 0, 0));
    }

    #[test]
    fn bgr555_corrected_pure_green() {
        assert_eq!(bgr555_to_rgb888_corrected(0x03E0), (0, 0xFF, 0));
    }

    #[test]
    fn bgr555_corrected_pure_blue() {
        assert_eq!(bgr555_to_rgb888_corrected(0x7C00), (0, 0, 0xFF));
    }

    #[test]
    fn bgr555_corrected_ignores_high_bit() {
        assert_eq!(bgr555_to_rgb888_corrected(0x8000), (0, 0, 0));
        assert_eq!(bgr555_to_rgb888_corrected(0xFFFF), (0xFF, 0xFF, 0xFF));
    }

    // ---- Tests for write_pixel_corrected -----------------------------------

    #[test]
    fn write_pixel_corrected_emits_rgb_byte_order() {
        let mut buf = [0u8; 6];
        write_pixel_corrected(&mut buf, 0, 0x001F); // red
        write_pixel_corrected(&mut buf, 3, 0x7C00); // blue
        assert_eq!(buf, [0xFF, 0, 0, 0, 0, 0xFF]);
    }

    // ---- Original tests for expand5_to_8 ----------------------------------

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
