/// Apply PPUMASK grayscale behavior by masking palette output to the high brightness bits (0x30),
/// preserving luminance while removing chroma.
#[inline(always)]
pub(crate) fn apply_grayscale(color_value: u8, grayscale: bool) -> u8 {
    if grayscale {
        color_value & 0x30
    } else {
        color_value
    }
}

/// How much each emphasis bit dims the two channels it does not emphasise, as Mesen2's
/// `NesDefaultVideoFilter::GenerateFullColorPalette` does for the 2C02.
const EMPHASIS_ATTENUATION: f64 = 0.84;

/// Apply PPUMASK color emphasis bits to the RGB of palette entry `color_value`.
///
/// Per nesdev (Colour emphasis) each bit emphasises one colour by darkening the other two;
/// nothing is brightened, so with all three bits set the whole picture darkens. Columns
/// `$xE` and `$xF` are not affected. The factor and the order of the multiplications follow
/// Mesen2, so the `mesen` palette matches its captures exactly.
///
/// On NES: bit layout is 0x01 = red, 0x02 = green, 0x04 = blue.
/// On Famicom: green and blue are swapped (0x02 = blue, 0x04 = green).
/// Set `swap_green_blue` to `true` for Famicom emphasis behavior.
#[inline(always)]
pub(crate) fn apply_color_emphasis(
    color_value: u8,
    r: u8,
    g: u8,
    b: u8,
    color_emphasis: u8,
    swap_green_blue: bool,
) -> (u8, u8, u8) {
    if color_emphasis == 0 || (color_value & 0x0F) >= 0x0E {
        return (r, g, b);
    }

    // On Famicom the green and blue emphasis bits are swapped vs NES.
    let emphasis = if swap_green_blue {
        (color_emphasis & 0x01) | ((color_emphasis & 0x04) >> 1) | ((color_emphasis & 0x02) << 1)
    } else {
        color_emphasis
    };

    let mut fr = f64::from(r);
    let mut fg = f64::from(g);
    let mut fb = f64::from(b);

    if emphasis & 0x01 != 0 {
        fg *= EMPHASIS_ATTENUATION;
        fb *= EMPHASIS_ATTENUATION;
    }
    if emphasis & 0x02 != 0 {
        fr *= EMPHASIS_ATTENUATION;
        fb *= EMPHASIS_ATTENUATION;
    }
    if emphasis & 0x04 != 0 {
        fr *= EMPHASIS_ATTENUATION;
        fg *= EMPHASIS_ATTENUATION;
    }

    // Truncation, as Mesen2's `(uint8_t)` cast; the values only ever shrink.
    (fr as u8, fg as u8, fb as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_color_emphasis_red_only() {
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x01, false);
        assert_eq!((r, g, b), (100, 84, 84));
    }

    #[test]
    fn test_apply_color_emphasis_green_only() {
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x02, false);
        assert_eq!((r, g, b), (84, 100, 84));
    }

    #[test]
    fn test_apply_color_emphasis_blue_only() {
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x04, false);
        assert_eq!((r, g, b), (84, 84, 100));
    }

    #[test]
    fn test_apply_color_emphasis_red_green() {
        // Blue is dimmed by both bits, red and green by one each.
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x03, false);
        assert_eq!((r, g, b), (84, 84, 70));
    }

    #[test]
    fn test_apply_color_emphasis_none() {
        let (r, g, b) = apply_color_emphasis(0x00, 123, 45, 67, 0x00, false);
        assert_eq!((r, g, b), (123, 45, 67));
    }

    #[test]
    fn emphasis_leaves_columns_xe_and_xf_alone() {
        // nesdev: "$1D black is affected by color emphasis, but $0F black is not."
        for color in [0x0E, 0x0F, 0x1E, 0x1F, 0x2E, 0x2F, 0x3E, 0x3F] {
            assert_eq!(
                apply_color_emphasis(color, 100, 100, 100, 0x07, false),
                (100, 100, 100),
                "colour ${color:02X}"
            );
        }
        assert_eq!(
            apply_color_emphasis(0x3D, 100, 100, 100, 0x07, false),
            (70, 70, 70)
        );
    }

    #[test]
    fn test_famicom_emphasis_bit_0x02_emphasizes_blue_not_green() {
        // On Famicom, bit 0x02 = blue (swapped from NES green)
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x02, true);
        assert_eq!((r, g, b), (84, 84, 100));
    }

    #[test]
    fn test_famicom_emphasis_bit_0x04_emphasizes_green_not_blue() {
        // On Famicom, bit 0x04 = green (swapped from NES blue)
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x04, true);
        assert_eq!((r, g, b), (84, 100, 84));
    }

    #[test]
    fn test_famicom_emphasis_red_unchanged() {
        // Red (bit 0x01) is the same on both NES and Famicom
        let (r, g, b) = apply_color_emphasis(0x00, 100, 100, 100, 0x01, true);
        assert_eq!((r, g, b), (100, 84, 84));
    }

    /// Mesen2 `NesDefaultVideoFilter::GenerateFullColorPalette` (2C02): each emphasis bit
    /// attenuates the two other channels by 0.84 and boosts nothing, so with R, G and B all
    /// set every channel is darkened twice. White must turn grey, never stay white.
    #[test]
    fn all_three_emphasis_bits_darken_white_to_grey() {
        let (r, g, b) = apply_color_emphasis(0x30, 0xFF, 0xFE, 0xFF, 0x07, false);
        assert_eq!((r, g, b), (179, 179, 179));
    }

    #[test]
    fn test_apply_grayscale_enabled() {
        assert_eq!(apply_grayscale(0x2f, true), 0x20);
        assert_eq!(apply_grayscale(0x2f, false), 0x2f);
    }
}
