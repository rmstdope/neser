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

/// Apply PPUMASK color emphasis bits to RGB output,
/// boosting the emphasized channels and attenuating the others to mimic NES/Famicom tinting.
///
/// On NES: bit layout is 0x01 = red, 0x02 = green, 0x04 = blue.
/// On Famicom: green and blue are swapped (0x02 = blue, 0x04 = green).
/// Set `swap_green_blue` to `true` for Famicom emphasis behavior.
#[inline(always)]
pub(crate) fn apply_color_emphasis(
    r: u8,
    g: u8,
    b: u8,
    color_emphasis: u8,
    swap_green_blue: bool,
) -> (u8, u8, u8) {
    if color_emphasis == 0 {
        return (r, g, b);
    }

    // On Famicom the green and blue emphasis bits are swapped vs NES.
    let emphasis = if swap_green_blue {
        (color_emphasis & 0x01) | ((color_emphasis & 0x04) >> 1) | ((color_emphasis & 0x02) << 1)
    } else {
        color_emphasis
    };

    let emphasize_red = (emphasis & 0x01) != 0;
    let emphasize_green = (emphasis & 0x02) != 0;
    let emphasize_blue = (emphasis & 0x04) != 0;

    const ATTENUATION: f32 = 0.75;
    const BOOST: f32 = 1.1;

    let mut fr = r as f32;
    let mut fg = g as f32;
    let mut fb = b as f32;

    if emphasize_red {
        fr = (fr * BOOST).min(255.0);
        if !emphasize_green {
            fg *= ATTENUATION;
        }
        if !emphasize_blue {
            fb *= ATTENUATION;
        }
    }
    if emphasize_green {
        fg = (fg * BOOST).min(255.0);
        if !emphasize_red {
            fr *= ATTENUATION;
        }
        if !emphasize_blue {
            fb *= ATTENUATION;
        }
    }
    if emphasize_blue {
        fb = (fb * BOOST).min(255.0);
        if !emphasize_red {
            fr *= ATTENUATION;
        }
        if !emphasize_green {
            fg *= ATTENUATION;
        }
    }

    (fr as u8, fg as u8, fb as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_color_emphasis_red_only() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x01, false);
        assert_eq!(r, 110);
        assert_eq!(g, 75);
        assert_eq!(b, 75);
    }

    #[test]
    fn test_apply_color_emphasis_green_only() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x02, false);
        assert_eq!(r, 75);
        assert_eq!(g, 110);
        assert_eq!(b, 75);
    }

    #[test]
    fn test_apply_color_emphasis_blue_only() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x04, false);
        assert_eq!(r, 75);
        assert_eq!(g, 75);
        assert_eq!(b, 110);
    }

    #[test]
    fn test_apply_color_emphasis_red_green() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x03, false);
        assert_eq!(r, 110);
        assert_eq!(g, 110);
        assert_eq!(b, 56);
    }

    #[test]
    fn test_apply_color_emphasis_all() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x07, false);
        assert_eq!(r, 110);
        assert_eq!(g, 110);
        assert_eq!(b, 110);
    }

    #[test]
    fn test_apply_color_emphasis_none() {
        let (r, g, b) = apply_color_emphasis(123, 45, 67, 0x00, false);
        assert_eq!((r, g, b), (123, 45, 67));
    }

    #[test]
    fn test_famicom_emphasis_bit_0x02_emphasizes_blue_not_green() {
        // On Famicom, bit 0x02 = blue (swapped from NES green)
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x02, true);
        assert_eq!(r, 75); // attenuated
        assert_eq!(g, 75); // attenuated
        assert_eq!(b, 110); // boosted (blue on Famicom)
    }

    #[test]
    fn test_famicom_emphasis_bit_0x04_emphasizes_green_not_blue() {
        // On Famicom, bit 0x04 = green (swapped from NES blue)
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x04, true);
        assert_eq!(r, 75); // attenuated
        assert_eq!(g, 110); // boosted (green on Famicom)
        assert_eq!(b, 75); // attenuated
    }

    #[test]
    fn test_famicom_emphasis_red_unchanged() {
        // Red (bit 0x01) is the same on both NES and Famicom
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x01, true);
        assert_eq!(r, 110); // boosted
        assert_eq!(g, 75); // attenuated
        assert_eq!(b, 75); // attenuated
    }

    #[test]
    fn test_apply_grayscale_enabled() {
        assert_eq!(apply_grayscale(0x2f, true), 0x20);
        assert_eq!(apply_grayscale(0x2f, false), 0x2f);
    }
}
