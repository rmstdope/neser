//! ROM entry: a single enriched ROM catalog item.

use std::path::PathBuf;

/// A discovered ROM enriched with metadata from the iNES header and ROM database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomEntry {
    /// Path to the ROM file on disk.
    pub path: PathBuf,
    /// Display name — from ROM DB if available, otherwise the file stem.
    pub display_name: String,
    /// Mapper number parsed from the iNES header.
    pub mapper: Option<u16>,
    /// Hardware type (e.g. NES NTSC, Famicom, VS System) from ROM DB or header.
    pub hardware: Option<String>,
    /// CRC32 of the PRG + CHR ROM data (hex, uppercase, 8 chars).
    pub crc: Option<String>,
}

impl RomEntry {
    /// Return a short hardware label suitable for display in a table column.
    pub fn hardware_label(&self) -> &str {
        self.hardware.as_deref().unwrap_or("-")
    }

    /// Return a mapper label suitable for display in a table column.
    pub fn mapper_label(&self) -> String {
        match self.mapper {
            Some(m) => m.to_string(),
            None => "-".to_string(),
        }
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
        RomEntry {
            path: PathBuf::from("/roms/test.nes"),
            display_name: "Test ROM".to_string(),
            mapper,
            hardware: hardware.map(str::to_string),
            crc: crc.map(str::to_string),
        }
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
        assert_eq!(entry.mapper_label(), "4");
    }

    #[test]
    fn test_mapper_label_absent() {
        let entry = make_entry(None, None, None);
        assert_eq!(entry.mapper_label(), "-");
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
}
