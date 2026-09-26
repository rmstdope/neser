//! Preset shade palettes for original Game Boy (DMG) games.
//!
//! The DMG draws four shades, lightest to darkest. A [`GbPalette`] maps those
//! four shades to RGB so a player can choose how an original Game Boy game
//! looks (`gb-palette=` / `--gb-palette`, F8 at runtime). Game Boy Color games
//! are never drawn with these.

/// A selectable preset shade palette for original Game Boy (DMG) games.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GbPalette {
    /// Neutral grey shades, the emulator's original look.
    #[default]
    Grey,
    /// The original 1989 Game Boy's green screen.
    DmgGreen,
    /// Game Boy Pocket's olive-grey screen.
    Pocket,
    /// Game Boy Light's backlit blue-green screen.
    Light,
}

impl GbPalette {
    /// All presets in F8 cycle order.
    pub const ALL: [GbPalette; 4] = [
        GbPalette::Grey,
        GbPalette::DmgGreen,
        GbPalette::Pocket,
        GbPalette::Light,
    ];

    /// The four shades, lightest (shade 0) to darkest (shade 3).
    pub fn shades(self) -> [(u8, u8, u8); 4] {
        match self {
            GbPalette::Grey => [
                (0xFF, 0xFF, 0xFF),
                (0xAA, 0xAA, 0xAA),
                (0x55, 0x55, 0x55),
                (0x00, 0x00, 0x00),
            ],
            GbPalette::DmgGreen => [
                (0x9B, 0xBC, 0x0F),
                (0x8B, 0xAC, 0x0F),
                (0x30, 0x62, 0x30),
                (0x0F, 0x38, 0x0F),
            ],
            GbPalette::Pocket => [
                (0xC4, 0xCF, 0xA1),
                (0x8B, 0x95, 0x6D),
                (0x4D, 0x53, 0x3C),
                (0x1F, 0x1F, 0x1F),
            ],
            GbPalette::Light => [
                (0x00, 0xB5, 0x81),
                (0x00, 0x9A, 0x71),
                (0x00, 0x69, 0x4A),
                (0x00, 0x4F, 0x3B),
            ],
        }
    }

    /// Human-readable name, as shown in the toast.
    pub fn display_name(self) -> &'static str {
        match self {
            GbPalette::Grey => "Grey",
            GbPalette::DmgGreen => "DMG Green",
            GbPalette::Pocket => "Pocket",
            GbPalette::Light => "Light",
        }
    }

    /// Lowercase config/CLI identifier.
    pub fn config_id(self) -> &'static str {
        match self {
            GbPalette::Grey => "grey",
            GbPalette::DmgGreen => "dmg-green",
            GbPalette::Pocket => "pocket",
            GbPalette::Light => "light",
        }
    }

    /// Parses a config/CLI identifier, case-insensitively; "gray" is Grey.
    pub fn from_config_id(id: &str) -> Option<GbPalette> {
        let id = id.trim().to_ascii_lowercase();
        if id == "gray" {
            return Some(GbPalette::Grey);
        }
        GbPalette::ALL.into_iter().find(|p| p.config_id() == id)
    }

    /// Every identifier in cycle order, joined with ", ".
    pub fn config_id_list() -> String {
        GbPalette::ALL.map(GbPalette::config_id).join(", ")
    }

    /// The next preset in cycle order, wrapping around.
    pub fn next(self) -> GbPalette {
        let idx = GbPalette::ALL.iter().position(|&p| p == self).unwrap_or(0);
        GbPalette::ALL[(idx + 1) % GbPalette::ALL.len()]
    }

    /// The `[background, foreground]` colours for the Game Boy LCD filter's
    /// `COLOR_PALETTE` texture.
    ///
    /// The background is the lightest shade. The shader multiplies the
    /// foreground by the background, so the foreground is darkest / lightest
    /// per channel: the darkest dots then land on the darkest shade.
    pub fn lcd_filter_colors(self) -> [(u8, u8, u8); 2] {
        let [light, _, _, dark] = self.shades();
        let divide = |d: u8, l: u8| -> u8 {
            if l == 0 {
                0
            } else {
                ((u32::from(d) * 255 + u32::from(l) / 2) / u32::from(l)).min(255) as u8
            }
        };
        [
            light,
            (
                divide(dark.0, light.0),
                divide(dark.1, light.1),
                divide(dark.2, light.2),
            ),
        ]
    }
}

/// Toast shown when F8 changes the Game Boy palette.
pub fn palette_toast_message(palette: GbPalette) -> String {
    format!("Palette: {}", palette.display_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_f8_order() {
        assert_eq!(
            GbPalette::ALL,
            [
                GbPalette::Grey,
                GbPalette::DmgGreen,
                GbPalette::Pocket,
                GbPalette::Light
            ]
        );
    }

    #[test]
    fn default_is_grey() {
        assert_eq!(GbPalette::default(), GbPalette::Grey);
    }

    #[test]
    fn grey_is_todays_shades() {
        assert_eq!(
            GbPalette::Grey.shades(),
            [
                (0xFF, 0xFF, 0xFF),
                (0xAA, 0xAA, 0xAA),
                (0x55, 0x55, 0x55),
                (0x00, 0x00, 0x00)
            ]
        );
    }

    #[test]
    fn preset_shades_match_the_agreed_colours() {
        assert_eq!(
            GbPalette::DmgGreen.shades(),
            [
                (0x9B, 0xBC, 0x0F),
                (0x8B, 0xAC, 0x0F),
                (0x30, 0x62, 0x30),
                (0x0F, 0x38, 0x0F)
            ]
        );
        assert_eq!(
            GbPalette::Pocket.shades(),
            [
                (0xC4, 0xCF, 0xA1),
                (0x8B, 0x95, 0x6D),
                (0x4D, 0x53, 0x3C),
                (0x1F, 0x1F, 0x1F)
            ]
        );
        assert_eq!(
            GbPalette::Light.shades(),
            [
                (0x00, 0xB5, 0x81),
                (0x00, 0x9A, 0x71),
                (0x00, 0x69, 0x4A),
                (0x00, 0x4F, 0x3B)
            ]
        );
    }

    #[test]
    fn display_names_are_the_toast_words() {
        let names: Vec<_> = GbPalette::ALL.map(GbPalette::display_name).to_vec();
        assert_eq!(names, ["Grey", "DMG Green", "Pocket", "Light"]);
    }

    #[test]
    fn config_ids_round_trip() {
        for p in GbPalette::ALL {
            assert_eq!(GbPalette::from_config_id(p.config_id()), Some(p));
        }
        assert_eq!(GbPalette::config_id_list(), "grey, dmg-green, pocket, light");
    }

    #[test]
    fn config_ids_are_case_insensitive_and_trimmed() {
        assert_eq!(
            GbPalette::from_config_id(" DMG-Green "),
            Some(GbPalette::DmgGreen)
        );
        assert_eq!(GbPalette::from_config_id("POCKET"), Some(GbPalette::Pocket));
    }

    #[test]
    fn gray_is_another_spelling_of_grey() {
        assert_eq!(GbPalette::from_config_id("gray"), Some(GbPalette::Grey));
        assert_eq!(GbPalette::from_config_id("Gray"), Some(GbPalette::Grey));
    }

    #[test]
    fn unknown_ids_are_rejected() {
        assert_eq!(GbPalette::from_config_id("bogus"), None);
        assert_eq!(GbPalette::from_config_id(""), None);
    }

    #[test]
    fn next_cycles_and_wraps() {
        assert_eq!(GbPalette::Grey.next(), GbPalette::DmgGreen);
        assert_eq!(GbPalette::DmgGreen.next(), GbPalette::Pocket);
        assert_eq!(GbPalette::Pocket.next(), GbPalette::Light);
        assert_eq!(GbPalette::Light.next(), GbPalette::Grey);
    }

    #[test]
    fn toast_uses_the_display_name() {
        assert_eq!(palette_toast_message(GbPalette::Grey), "Palette: Grey");
        assert_eq!(
            palette_toast_message(GbPalette::DmgGreen),
            "Palette: DMG Green"
        );
        assert_eq!(palette_toast_message(GbPalette::Pocket), "Palette: Pocket");
        assert_eq!(palette_toast_message(GbPalette::Light), "Palette: Light");
    }

    #[test]
    fn lcd_filter_background_is_the_lightest_shade() {
        for p in GbPalette::ALL {
            assert_eq!(p.lcd_filter_colors()[0], p.shades()[0], "{p:?}");
        }
    }

    #[test]
    fn lcd_filter_foreground_times_background_is_the_darkest_shade() {
        // The LCD shader multiplies the foreground by the background colour,
        // so the darkest dots land on the preset's darkest shade.
        for p in GbPalette::ALL {
            let [bg, fg] = p.lcd_filter_colors();
            let dark = p.shades()[3];
            let product = |f: u8, b: u8| (u32::from(f) * u32::from(b) + 127) / 255;
            for (f, b, d) in [(fg.0, bg.0, dark.0), (fg.1, bg.1, dark.1), (fg.2, bg.2, dark.2)] {
                let got = product(f, b) as i32;
                assert!((got - i32::from(d)).abs() <= 1, "{p:?}: {got} vs {d}");
            }
        }
    }

    #[test]
    fn grey_lcd_filter_is_white_paper_with_black_dots() {
        assert_eq!(
            GbPalette::Grey.lcd_filter_colors(),
            [(0xFF, 0xFF, 0xFF), (0x00, 0x00, 0x00)]
        );
    }
}
