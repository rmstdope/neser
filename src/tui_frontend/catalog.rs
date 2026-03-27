//! Load and enrich ROM catalog entries for TUI display.

use std::path::{Path, PathBuf};

use crate::cartridge::{HardwareType, ParsedRom, RomDb};
use crate::console::{
    CartridgeCatalogOptions, default_catalog_csv_path, refresh_cartridge_catalog,
};

use super::rom_entry::RomEntry;

/// Load the ROM catalog from disk and enrich each entry with iNES + ROM DB metadata.
///
/// This reads the catalog CSV (creating/refreshing it if configured), parses each
/// ROM file's iNES header to extract mapper and CRC, then looks up the CRC in the
/// ROM database to get a display name and hardware type.
///
/// # Errors
///
/// Returns an error if the catalog CSV cannot be read or written.
pub fn load_catalog(search_paths: &[String], rebuild: bool) -> Result<Vec<RomEntry>, String> {
    let rom_paths = read_catalog_paths(search_paths, rebuild)?;
    let rom_db = RomDb::new().map_err(|e| format!("Failed to load ROM DB: {e}"))?;
    Ok(rom_paths
        .iter()
        .map(|p| build_rom_entry(p, &rom_db))
        .collect())
}

fn read_catalog_paths(search_paths: &[String], rebuild: bool) -> Result<Vec<PathBuf>, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME environment variable not set".to_string())?;

    let catalog_path = default_catalog_csv_path(&home);
    let mut paths: Vec<PathBuf> = search_paths.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        paths.push(home.join(".neser").join("roms"));
    }

    let mut opts = CartridgeCatalogOptions::new(paths, catalog_path);
    opts.scan_enabled = true;
    opts.rebuild_catalog = rebuild;

    refresh_cartridge_catalog(&opts).map_err(|e| format!("Catalog refresh failed: {e}"))
}

/// Parse one ROM file and build a `RomEntry`, falling back gracefully on errors.
pub(super) fn build_rom_entry(path: &Path, rom_db: &RomDb) -> RomEntry {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return unreadable_entry(path),
    };

    let parsed = match ParsedRom::parse(&bytes, Some(rom_db)) {
        Ok(p) => p,
        Err(_) => return invalid_entry(path),
    };

    let crc = parsed.crc32;
    let db_entry = rom_db.get_by_crc(crc);

    let display_name = db_entry
        .and_then(|e| e.name.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| file_stem(path));

    let mapper = Some(parsed.header.mapper);

    let hardware = db_entry.and_then(|e| e.hardware).unwrap_or_else(|| {
        HardwareType::from_console_type_and_timing(
            parsed.header.console_type,
            parsed.header.timing_mode,
        )
    });

    RomEntry {
        path: path.to_path_buf(),
        display_name,
        mapper,
        hardware: Some(hardware_label(hardware)),
        crc: Some(format!("{crc:08X}")),
        recording_duration: read_recording_duration(path),
    }
}

fn unreadable_entry(path: &Path) -> RomEntry {
    RomEntry {
        path: path.to_path_buf(),
        display_name: file_stem(path),
        mapper: None,
        hardware: None,
        crc: None,
        recording_duration: read_recording_duration(path),
    }
}

fn invalid_entry(path: &Path) -> RomEntry {
    RomEntry {
        path: path.to_path_buf(),
        display_name: file_stem(path),
        mapper: None,
        hardware: None,
        crc: None,
        recording_duration: read_recording_duration(path),
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string()
}

/// Return the duration of the `.autorun` recording for `rom_path`, or `None` if no recording
/// exists. Reads only the `frames` array length to avoid loading large checkpoint `state_bytes`.
pub(crate) fn read_recording_duration(rom_path: &Path) -> Option<std::time::Duration> {
    let autorun_path = rom_path.with_extension("autorun");
    let content = std::fs::read_to_string(&autorun_path).ok()?;

    // Lightweight parse: only count frames, skip large state_bytes in checkpoints.
    #[derive(serde::Deserialize)]
    struct AutorunMeta {
        frames: Vec<serde::de::IgnoredAny>,
    }
    let meta: AutorunMeta = serde_json::from_str(&content).ok()?;
    let frame_count = meta.frames.len() as f64;
    Some(std::time::Duration::from_secs_f64(frame_count / 60.0))
}

fn hardware_label(hw: HardwareType) -> String {
    match hw {
        HardwareType::NesNtsc => "NES NTSC".to_string(),
        HardwareType::NesPal => "NES PAL".to_string(),
        HardwareType::Famicom => "Famicom".to_string(),
        HardwareType::VsSystem => "VS System".to_string(),
        HardwareType::FamicomNetworkSystem => "FDS".to_string(),
        _ => "NES NTSC".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::RomDb;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn minimal_nrom_rom() -> Vec<u8> {
        // Minimal valid iNES header: NROM (mapper 0), 1×16KB PRG, 1×8KB CHR, NTSC
        let mut rom = Vec::new();
        rom.extend_from_slice(b"NES\x1A"); // magic
        rom.push(1); // 1 × 16KB PRG-ROM
        rom.push(1); // 1 × 8KB CHR-ROM
        rom.push(0); // flags6: mapper lo = 0, horizontal mirroring
        rom.push(0); // flags7: mapper hi = 0, NES console
        rom.extend_from_slice(&[0u8; 8]); // flags8–15
        rom.extend_from_slice(&[0xAAu8; 16 * 1024]); // 16KB PRG-ROM
        rom.extend_from_slice(&[0x55u8; 8 * 1024]); // 8KB CHR-ROM
        rom
    }

    #[test]
    fn test_build_rom_entry_valid_rom_has_mapper_and_crc() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);

        assert_eq!(entry.mapper, Some(0), "mapper should be 0 (NROM)");
        assert!(entry.crc.is_some(), "CRC should be present");
        let crc = entry.crc.as_deref().unwrap();
        assert_eq!(crc.len(), 8, "CRC should be 8 hex chars");
    }

    #[test]
    fn test_build_rom_entry_missing_file_returns_stub() {
        let db = RomDb::from_csv_content("");
        let entry = build_rom_entry(Path::new("/nonexistent/game.nes"), &db);

        assert_eq!(entry.display_name, "game");
        assert!(entry.crc.is_none());
        assert!(entry.mapper.is_none());
    }

    #[test]
    fn test_build_rom_entry_invalid_file_returns_stub() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"NOT A NES ROM").unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);

        assert!(entry.crc.is_none());
        assert!(entry.mapper.is_none());
    }

    #[test]
    fn test_build_rom_entry_uses_db_name_when_available() {
        // Build a CSV entry for the CRC of our minimal ROM
        let rom_bytes = minimal_nrom_rom();
        let prg = &rom_bytes[16..16 + 16 * 1024];
        let chr = &rom_bytes[16 + 16 * 1024..];
        let crc = crate::cartridge::calculate_rom_crc32(prg, chr);
        let csv = format!(
            "rom_id,name,country,crc,hardware,rom_class,mapper,submapper,nametable_layout,\
             prg_rom_size,prg_rom_crc,prg_nvram_size,prg_ram_size,chr_rom_size,chr_rom_crc,\
             chr_nvram_size,chr_ram_size,battery,vs_hardware_type,vs_ppu_type,expansion_type\n\
             1,My Test ROM,USA,{crc:08X},,,0,,,,,,,,,,,,,,"
        );
        let db = RomDb::from_csv_content(&csv);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&rom_bytes).unwrap();

        let entry = build_rom_entry(tmp.path(), &db);
        assert_eq!(entry.display_name, "My Test ROM", "should use ROM DB name");
    }

    #[test]
    fn test_build_rom_entry_falls_back_to_filename_when_not_in_db() {
        let mut tmp = NamedTempFile::with_suffix(".nes").unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);
        // display_name should be the file stem (not from DB)
        let stem = tmp
            .path()
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(entry.display_name, stem);
    }

    #[test]
    fn test_build_rom_entry_no_recording_duration_when_no_autorun_file() {
        let mut tmp = NamedTempFile::with_suffix(".nes").unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let db = RomDb::from_csv_content("");
        let entry = build_rom_entry(tmp.path(), &db);
        assert!(
            entry.recording_duration.is_none(),
            "no .autorun file should mean recording_duration=None"
        );
    }

    #[test]
    fn test_build_rom_entry_recording_duration_when_autorun_file_exists() {
        let mut tmp = NamedTempFile::with_suffix(".nes").unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        // Create a minimal valid .autorun JSON with 360 frames (~6 seconds at 60fps)
        let autorun_path = tmp.path().with_extension("autorun");
        let frames_json: String = (0..360)
            .map(|_| r#"{"player1":0,"player2":0}"#)
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(r#"{{"version":3,"frames":[{frames_json}],"checkpoints":[]}}"#);
        std::fs::write(&autorun_path, json.as_bytes()).unwrap();
        let db = RomDb::from_csv_content("");
        let entry = build_rom_entry(tmp.path(), &db);
        let _ = std::fs::remove_file(&autorun_path);
        let dur = entry.recording_duration.expect("should have duration");
        assert_eq!(dur.as_secs(), 6, "360 frames at 60fps = 6 seconds");
    }

    #[test]
    fn test_read_recording_duration_returns_none_for_invalid_json() {
        let mut tmp = NamedTempFile::with_suffix(".nes").unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let autorun_path = tmp.path().with_extension("autorun");
        std::fs::write(&autorun_path, b"not valid json").unwrap();
        let dur = read_recording_duration(tmp.path());
        let _ = std::fs::remove_file(&autorun_path);
        assert!(dur.is_none(), "invalid JSON should return None");
    }
}
