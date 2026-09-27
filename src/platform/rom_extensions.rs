//! Which ROM file extension means which console: the one table every
//! frontend reads.
//!
//! The desktop loader, the ROM catalog and the web page (through the wasm
//! binding `rom_extension_table`) all ask this module, so adding a console's
//! extension is one row here.

use std::path::Path;

use crate::platform::emulator::SystemType;

/// The console platform a ROM targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Nes,
    Gb,
    Gbc,
    Gba,
    Snes,
}

impl Platform {
    /// Short display label for the platform.
    pub fn label(self) -> &'static str {
        match self {
            Platform::Nes => "NES",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Gba => "GBA",
            Platform::Snes => "SNES",
        }
    }

    /// TheGamesDB platform ID for metadata matching.
    /// Must stay in sync with PLATFORMS in scripts/metadata_scraper/main.py.
    pub fn thegamesdb_id(self) -> i64 {
        match self {
            Platform::Nes => 7,
            Platform::Gb => 4,
            Platform::Gbc => 41,
            Platform::Gba => 5,
            Platform::Snes => 6,
        }
    }
}

/// Every supported ROM extension (lower case, no dot) and its platform, in
/// the order players read them.
pub const ROM_EXTENSIONS: &[(&str, Platform)] = &[
    ("nes", Platform::Nes),
    ("gb", Platform::Gb),
    ("gbc", Platform::Gbc),
    ("cgb", Platform::Gbc),
    ("gba", Platform::Gba),
    ("sfc", Platform::Snes),
    ("smc", Platform::Snes),
];

/// The platform whose ROMs carry `extension` (any case, no dot).
pub fn platform_for_extension(extension: &str) -> Option<Platform> {
    ROM_EXTENSIONS
        .iter()
        .find(|(known, _)| extension.eq_ignore_ascii_case(known))
        .map(|&(_, platform)| platform)
}

/// The platform of the ROM at `path`, from its extension.
pub fn platform_for_path(path: &Path) -> Option<Platform> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(platform_for_extension)
}

impl Platform {
    /// The emulated system that runs this platform's ROMs.
    pub fn system_type(self) -> SystemType {
        match self {
            Platform::Nes => SystemType::Nes,
            Platform::Gb | Platform::Gbc => SystemType::GameBoy,
            Platform::Gba => SystemType::Gba,
            Platform::Snes => SystemType::Snes,
        }
    }
}

/// The key the web frontend names `system` by (its `ConsoleKind`).
pub fn console_key(system: SystemType) -> &'static str {
    match system {
        SystemType::Nes => "nes",
        SystemType::GameBoy => "gb",
        SystemType::Gba => "gba",
        SystemType::Snes => "snes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_lists_every_extension_in_reading_order() {
        let extensions: Vec<&str> = ROM_EXTENSIONS.iter().map(|(ext, _)| *ext).collect();
        assert_eq!(extensions, ["nes", "gb", "gbc", "cgb", "gba", "sfc", "smc"]);
    }

    #[test]
    fn each_extension_names_its_platform() {
        assert_eq!(platform_for_extension("nes"), Some(Platform::Nes));
        assert_eq!(platform_for_extension("gb"), Some(Platform::Gb));
        assert_eq!(platform_for_extension("gbc"), Some(Platform::Gbc));
        assert_eq!(platform_for_extension("gba"), Some(Platform::Gba));
        assert_eq!(platform_for_extension("sfc"), Some(Platform::Snes));
        assert_eq!(platform_for_extension("smc"), Some(Platform::Snes));
    }

    #[test]
    fn cgb_is_a_game_boy_color_rom() {
        assert_eq!(platform_for_extension("cgb"), Some(Platform::Gbc));
    }

    #[test]
    fn extension_matching_ignores_case() {
        assert_eq!(platform_for_extension("SFC"), Some(Platform::Snes));
        assert_eq!(platform_for_extension("Gbc"), Some(Platform::Gbc));
    }

    #[test]
    fn unknown_extension_names_no_platform() {
        assert_eq!(platform_for_extension("txt"), None);
        assert_eq!(platform_for_extension(""), None);
    }

    #[test]
    fn path_platform_reads_the_extension() {
        assert_eq!(
            platform_for_path(Path::new("/roms/Zelda.SMC")),
            Some(Platform::Snes)
        );
        assert_eq!(platform_for_path(Path::new("/roms/noext")), None);
    }

    #[test]
    fn each_platform_runs_on_its_system() {
        assert_eq!(Platform::Nes.system_type(), SystemType::Nes);
        assert_eq!(Platform::Gb.system_type(), SystemType::GameBoy);
        assert_eq!(Platform::Gbc.system_type(), SystemType::GameBoy);
        assert_eq!(Platform::Gba.system_type(), SystemType::Gba);
        assert_eq!(Platform::Snes.system_type(), SystemType::Snes);
    }

    #[test]
    fn console_keys_are_the_web_console_kinds() {
        assert_eq!(console_key(SystemType::Nes), "nes");
        assert_eq!(console_key(SystemType::GameBoy), "gb");
        assert_eq!(console_key(SystemType::Gba), "gba");
        assert_eq!(console_key(SystemType::Snes), "snes");
    }
}
