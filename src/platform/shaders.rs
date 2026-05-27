/// Single source of truth for all available shader presets.
///
/// Each entry is `(short_name, relative_path_to_slangp)`.
/// - `short_name` is used in the CLI (`--nes-filter crt`, `--gb-filter dmg`, `--gba-filter gba-lcd`) and config file (`nes-filter=crt`, `gb-filter=dmg`, `gba-filter=gba-lcd`).
/// - `path` is relative to the working directory when neser runs.
///
/// To add or remove a shader preset, edit this list only.
pub const SHADER_PRESETS: &[(&str, &str)] = &[
    ("none", "shaders/stock.slangp"),
    ("crt", "vendor/slang-shaders/crt/crt-lottes.slangp"),
    (
        "smooth",
        "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp",
    ),
    (
        "ntsc",
        "vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp",
    ),
    (
        "pal",
        "vendor/slang-shaders/pal/decoupled-guest-advanced-pal 3-RF.slangp",
    ),
    ("dmg", "vendor/slang-shaders/handheld/gameboy.slangp"),
    // GBA LCD placeholder - has its own preset path so it remains distinct from `none`
    ("gba-lcd", "shaders/gba-lcd.slangp"),
    ("agb001", "vendor/slang-shaders/handheld/agb001.slangp"),
    (
        "nso-gba-color",
        "vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp",
    ),
    (
        "sp101-color",
        "vendor/slang-shaders/handheld/color-mod/sp101-color.slangp",
    ),
    (
        "gba-lcd-grid",
        "vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp",
    ),
];

#[cfg(test)]
mod tests {
    use super::SHADER_PRESETS;

    fn assert_preset_path(name: &str, expected_path: &str) {
        let found = SHADER_PRESETS
            .iter()
            .find(|(preset_name, _)| *preset_name == name)
            .map(|(_, path)| *path);

        assert_eq!(
            found,
            Some(expected_path),
            "Preset '{name}' should resolve to '{expected_path}'"
        );
    }

    #[test]
    fn test_gba_preset_agb001_maps_to_expected_path() {
        assert_preset_path("agb001", "vendor/slang-shaders/handheld/agb001.slangp");
    }

    #[test]
    fn test_gba_preset_nso_gba_color_maps_to_expected_path() {
        assert_preset_path(
            "nso-gba-color",
            "vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp",
        );
    }

    #[test]
    fn test_gba_preset_sp101_color_maps_to_expected_path() {
        assert_preset_path(
            "sp101-color",
            "vendor/slang-shaders/handheld/color-mod/sp101-color.slangp",
        );
    }

    #[test]
    fn test_gba_preset_gba_lcd_grid_maps_to_expected_path() {
        assert_preset_path(
            "gba-lcd-grid",
            "vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp",
        );
    }
}
