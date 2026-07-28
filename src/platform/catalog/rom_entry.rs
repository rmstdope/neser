//! ROM entry: a single enriched ROM catalog item.

use std::path::PathBuf;

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
    /// Must stay in sync with PLATFORMS in scripts/metadata-scraper/main.py.
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

/// A discovered ROM enriched with metadata from the iNES header and ROM database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomEntry {
    /// Path to the ROM file on disk.
    pub path: PathBuf,
    /// Display name — from ROM DB if available, otherwise the file stem.
    pub display_name: String,
    /// Pre-computed lowercase version of `display_name` used for fast case-insensitive filtering.
    pub search_key: String,
    /// Pre-computed mapper label (e.g. "4" or "-") to avoid per-frame allocations.
    pub mapper_label: String,
    /// Mapper number parsed from the iNES header.
    pub mapper: Option<u16>,
    /// Hardware type (e.g. NES NTSC, Famicom, VS System) from ROM DB or header.
    pub hardware: Option<String>,
    /// CRC32 of the PRG + CHR ROM data (hex, uppercase, 8 chars).
    pub crc: Option<String>,
    /// Duration of the `.autorun` recording, or `None` if no recording exists.
    pub recording_duration: Option<std::time::Duration>,
    /// TheGamesDB game ID matched via fuzzy title matching, if any.
    pub metadata_game_id: Option<i64>,
    /// Genre names from TheGamesDB (e.g. "Action", "Adventure").
    pub genres: Vec<String>,
    /// Game overview/description from TheGamesDB.
    pub overview: Option<String>,
    /// Release date string from TheGamesDB (e.g. "1990-02-12").
    pub release_date: Option<String>,
    /// Number of players from TheGamesDB.
    pub players: Option<u32>,
    /// Content rating from TheGamesDB (e.g. "E - Everyone").
    pub rating: Option<String>,
    /// Path to cached front boxart image, if downloaded.
    pub boxart_path: Option<PathBuf>,
    /// Paths to cached screenshot images, if downloaded.
    pub screenshot_paths: Vec<PathBuf>,
    /// Whether this ROM is marked as a favorite by the user.
    pub is_favorite: bool,
    /// The console platform this ROM targets.
    pub platform: Platform,
}

impl RomEntry {
    /// Return a short hardware label suitable for display in a table column.
    pub fn hardware_label(&self) -> &str {
        self.hardware.as_deref().unwrap_or("-")
    }

    /// Return a CRC label suitable for display in a table column.
    pub fn crc_label(&self) -> &str {
        self.crc.as_deref().unwrap_or("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(mapper: Option<u16>, hardware: Option<&str>, crc: Option<&str>) -> RomEntry {
        let display_name = "Test ROM".to_string();
        let search_key = display_name.to_lowercase();
        let mapper_label = mapper.map_or_else(|| "-".to_string(), |m| m.to_string());
        RomEntry {
            path: PathBuf::from("/roms/test.nes"),
            display_name,
            search_key,
            mapper_label,
            mapper,
            hardware: hardware.map(str::to_string),
            crc: crc.map(str::to_string),
            recording_duration: None,
            metadata_game_id: None,
            genres: Vec::new(),
            overview: None,
            release_date: None,
            players: None,
            rating: None,
            boxart_path: None,
            screenshot_paths: Vec::new(),
            is_favorite: false,
            platform: Platform::Nes,
        }
    }

    #[test]
    fn test_platform_labels_cover_all_platforms() {
        assert_eq!(Platform::Nes.label(), "NES");
        assert_eq!(Platform::Gb.label(), "GB");
        assert_eq!(Platform::Gbc.label(), "GBC");
        assert_eq!(Platform::Gba.label(), "GBA");
        assert_eq!(Platform::Snes.label(), "SNES");
    }

    #[test]
    fn test_platform_thegamesdb_ids_match_scraper_registry() {
        // Must match PLATFORMS in scripts/metadata-scraper/main.py.
        assert_eq!(Platform::Nes.thegamesdb_id(), 7);
        assert_eq!(Platform::Gb.thegamesdb_id(), 4);
        assert_eq!(Platform::Gbc.thegamesdb_id(), 41);
        assert_eq!(Platform::Gba.thegamesdb_id(), 5);
        assert_eq!(Platform::Snes.thegamesdb_id(), 6);
    }

    #[test]
    fn test_hardware_label_present() {
        let entry = make_entry(None, Some("NES NTSC"), None);
        assert_eq!(entry.hardware_label(), "NES NTSC");
    }

    #[test]
    fn test_hardware_label_absent() {
        let entry = make_entry(None, None, None);
        assert_eq!(entry.hardware_label(), "-");
    }

    #[test]
    fn test_mapper_label_present() {
        let entry = make_entry(Some(4), None, None);
        assert_eq!(entry.mapper_label, "4");
    }

    #[test]
    fn test_mapper_label_absent() {
        let entry = make_entry(None, None, None);
        assert_eq!(entry.mapper_label, "-");
    }

    #[test]
    fn test_crc_label_present() {
        let entry = make_entry(None, None, Some("DEADBEEF"));
        assert_eq!(entry.crc_label(), "DEADBEEF");
    }

    #[test]
    fn test_crc_label_absent() {
        let entry = make_entry(None, None, None);
        assert_eq!(entry.crc_label(), "-");
    }

    #[test]
    fn test_recording_duration_none_by_default() {
        let entry = make_entry(None, None, None);
        assert!(entry.recording_duration.is_none());
    }

    #[test]
    fn test_search_key_is_lowercase_display_name() {
        let entry = make_entry(None, None, None);
        assert_eq!(entry.search_key, "test rom");
    }

    #[test]
    fn test_metadata_fields_default_to_none_or_empty() {
        let entry = make_entry(None, None, None);
        assert!(entry.metadata_game_id.is_none());
        assert!(entry.genres.is_empty());
        assert!(entry.overview.is_none());
        assert!(entry.release_date.is_none());
        assert!(entry.players.is_none());
        assert!(entry.rating.is_none());
        assert!(entry.boxart_path.is_none());
        assert!(entry.screenshot_paths.is_empty());
        assert!(!entry.is_favorite);
    }
}
