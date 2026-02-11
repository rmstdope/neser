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

/// Apply PPUMASK color emphasis bits (0x01 = red, 0x02 = green, 0x04 = blue) to RGB output,
/// boosting the emphasized channels and attenuating the others to mimic NES tinting.
#[inline(always)]
pub(crate) fn apply_color_emphasis(r: u8, g: u8, b: u8, color_emphasis: u8) -> (u8, u8, u8) {
    if color_emphasis == 0 {
        return (r, g, b);
    }

    let emphasize_red = (color_emphasis & 0x01) != 0;
    let emphasize_green = (color_emphasis & 0x02) != 0;
    let emphasize_blue = (color_emphasis & 0x04) != 0;

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
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x01);
        assert_eq!(r, 110);
        assert_eq!(g, 75);
        assert_eq!(b, 75);
    }

    #[test]
    fn test_apply_grayscale_enabled() {
        assert_eq!(apply_grayscale(0x2f, true), 0x20);
        assert_eq!(apply_grayscale(0x2f, false), 0x2f);
    }
}
