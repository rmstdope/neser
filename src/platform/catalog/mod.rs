//! Load and enrich ROM catalog entries.
//!
//! This module provides the shared ROM discovery and catalog enrichment logic
//! used by both the TUI and native graphical frontends.

pub mod enrichment_cache;
pub mod favorites;
pub mod rom_entry;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::nes::cartridge::{HardwareType, ParsedRom, RomDb};
use crate::nes::console::{
    CartridgeCatalogOptions, default_catalog_csv_path, refresh_cartridge_catalog,
};

pub use rom_entry::Platform;
pub use rom_entry::RomEntry;

/// Load the ROM catalog from disk and enrich each entry with iNES + ROM DB metadata.
///
/// This reads the catalog CSV (creating/refreshing it if configured), parses each
/// ROM file's iNES header to extract mapper and CRC, then looks up the CRC in the
/// ROM database to get a display name and hardware type.
///
/// # Errors
///
/// Returns an error if the catalog CSV cannot be read or written.
pub fn load_catalog(
    search_paths: &[String],
    rebuild: bool,
    include_unofficial: bool,
) -> Result<Vec<RomEntry>, String> {
    let rom_paths = read_catalog_paths(search_paths, rebuild)?;
    let rom_db = RomDb::new().map_err(|e| format!("Failed to load ROM DB: {e}"))?;
    let entries: Vec<RomEntry> = rom_paths
        .iter()
        .map(|p| build_rom_entry(p, &rom_db))
        .collect();
    if include_unofficial {
        let mut result = entries;
        dedup_by_crc(&mut result);
        Ok(result)
    } else {
        let mut result: Vec<RomEntry> = entries
            .into_iter()
            .filter(|e| !is_unofficial_rom(e))
            .collect();
        dedup_by_crc(&mut result);
        Ok(result)
    }
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

/// Remove duplicate ROM entries that share the same CRC32 hash, keeping
/// only the first occurrence.  Entries without a CRC are always retained.
pub fn dedup_by_crc(entries: &mut Vec<RomEntry>) {
    let mut seen = HashSet::new();
    entries.retain(|e| match &e.crc {
        Some(crc) => seen.insert(crc.clone()),
        None => true,
    });
}

/// Parse one ROM file and build a `RomEntry`, falling back gracefully on errors.
pub fn build_rom_entry(path: &Path, rom_db: &RomDb) -> RomEntry {
    let platform = platform_from_path(path);

    // Only NES ROMs use the iNES format — other platforms get a basic entry.
    if platform != Platform::Nes {
        return stub_entry(path, platform);
    }

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
        search_key: display_name.to_lowercase(),
        mapper_label: mapper.map_or_else(|| "-".to_string(), |m| m.to_string()),
        display_name,
        mapper,
        hardware: Some(hardware_label(hardware)),
        crc: Some(format!("{crc:08X}")),
        recording_duration: read_recording_duration(path),
        metadata_game_id: None,
        genres: Vec::new(),
        overview: None,
        release_date: None,
        players: None,
        rating: None,
        boxart_path: None,
        screenshot_paths: Vec::new(),
        is_favorite: false,
        platform,
    }
}

fn unreadable_entry(path: &Path) -> RomEntry {
    stub_entry(path, platform_from_path(path))
}

fn invalid_entry(path: &Path) -> RomEntry {
    stub_entry(path, platform_from_path(path))
}

/// Minimal entry for ROMs that cannot be parsed or are non-NES.
fn stub_entry(path: &Path, platform: Platform) -> RomEntry {
    let display_name = file_stem(path);
    RomEntry {
        path: path.to_path_buf(),
        search_key: display_name.to_lowercase(),
        mapper_label: "-".to_string(),
        display_name,
        mapper: None,
        hardware: None,
        crc: None,
        recording_duration: read_recording_duration(path),
        metadata_game_id: None,
        genres: Vec::new(),
        overview: None,
        release_date: None,
        players: None,
        rating: None,
        boxart_path: None,
        screenshot_paths: Vec::new(),
        is_favorite: false,
        platform,
    }
}

/// Determine the platform from a ROM file's extension.
fn platform_from_path(path: &Path) -> Platform {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("gb") => Platform::Gb,
        Some("gbc") => Platform::Gbc,
        Some("gba") => Platform::Gba,
        Some("sfc") | Some("smc") => Platform::Snes,
        _ => Platform::Nes,
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string()
}

/// Keywords in ROM filenames that indicate unofficial/non-original ROMs.
/// Each keyword is checked as a case-insensitive substring of the filename
/// and display name.
const UNOFFICIAL_KEYWORDS: &[&str] = &[
    "(hack",
    "[hack",
    " hack ",
    " hack.",
    "_hack_",
    "_hack.",
    "-hack-",
    "-hack.",
    "(pirate)",
    "[pirate]",
    "(bootleg)",
    "[bootleg]",
    "(homebrew)",
    "[homebrew]",
    "(unl)",
    "[unl]",
    "(unif)",
    "[unif]",
    "(pd)",
    "[pd]",
    "(beta)",
    "[beta]",
    "(proto)",
    "[proto]",
    "(prototype)",
    "[prototype]",
    "(sample)",
    "[sample]",
    "(demo)",
    "[demo]",
    "-demo.",
    "-demo-",
    "_demo_",
    "_demo.",
    " demo.",
    "(bad)",
    "[bad]",
    "[b]",
    "-bad.",
    "-bad-",
    "_bad_",
    "_bad.",
    " bad.",
    "(overdump)",
    "[o]",
    "[!p]",
    "[t]",
    "(trainer)",
    "[t+",
    "(translation)",
    "-menu.",
    "-menu-",
    "_menu_",
    "_menu.",
    " menu.",
    "(menu)",
    "[menu]",
    "-test.",
    "-test-",
    "_test_",
    "_test.",
    " test.",
    "(test)",
    "[test]",
    "-hm00.",
    "-hm00-",
    "_hm00_",
    "_hm00.",
    "[hm00]",
    "-pd.",
    "-pd-",
    "_pd_",
    "_pd.",
    " pd.",
    "zzz-",
    "zzz_",
    "zzz ",
];

/// GoodNES/Good* numbered suffixes that indicate unofficial dumps.
/// Matches patterns like `-p1`, `-h2`, `[b3]`, `[p12]`, `[h1]` in filenames.
fn has_numbered_dump_suffix(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();
    // Check for `-p<digits>.` or `-h<digits>.` (dash-prefixed, dot-terminated)
    // e.g. "game-p1.nes", "game-h12.nes"
    for i in 0..len.saturating_sub(3) {
        if bytes[i] == b'-' && (bytes[i + 1] == b'p' || bytes[i + 1] == b'h') {
            let rest = &s[i + 2..];
            if let Some(dot) = rest.find('.') {
                let num_part = &rest[..dot];
                if !num_part.is_empty() && num_part.bytes().all(|b| b.is_ascii_digit()) {
                    return true;
                }
            }
        }
    }
    // Check for `[bN]`, `[pN]`, `[hN]` (bracket-prefixed, numbered)
    // e.g. "[b1]", "[p2]", "[h3]"
    for i in 0..len.saturating_sub(4) {
        if bytes[i] == b'['
            && (bytes[i + 1] == b'b' || bytes[i + 1] == b'p' || bytes[i + 1] == b'h')
        {
            let rest = &s[i + 2..];
            if let Some(bracket) = rest.find(']') {
                let num_part = &rest[..bracket];
                if !num_part.is_empty() && num_part.bytes().all(|b| b.is_ascii_digit()) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check whether a ROM entry appears to be an unofficial ROM based on its
/// filename and display name. Uses keyword matching against common ROM naming
/// conventions (No-Intro, GoodNES, etc.).
fn is_unofficial_rom(entry: &RomEntry) -> bool {
    let filename = entry
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let name = entry.display_name.to_lowercase();

    UNOFFICIAL_KEYWORDS
        .iter()
        .any(|kw| filename.contains(kw) || name.contains(kw))
        || has_numbered_dump_suffix(&filename)
        || has_numbered_dump_suffix(&name)
}

/// Return the duration of the `.autorun` recording for `rom_path`, or `None` if no recording
/// exists.
///
/// The v3 `.autorun` format uses RLE compression: each entry in the `frames` array has a
/// `repeat: u32` count. Duration = sum(repeat) / 60 fps.
/// The legacy v2 format has no `repeat` field — we default it to 1 (one entry = one frame).
pub fn read_recording_duration(rom_path: &Path) -> Option<std::time::Duration> {
    let autorun_path = rom_path.with_extension("autorun");
    let content = std::fs::read_to_string(&autorun_path).ok()?;

    // Parse only the `frames` array; skip large `state_bytes` in checkpoints.
    // `repeat` defaults to 1 so legacy v2 files (no repeat field) are handled correctly.
    #[derive(serde::Deserialize)]
    struct RleFrame {
        #[serde(default = "one")]
        repeat: u32,
    }
    fn one() -> u32 {
        1
    }
    #[derive(serde::Deserialize)]
    struct AutorunMeta {
        frames: Vec<RleFrame>,
    }

    let meta: AutorunMeta = serde_json::from_str(&content).ok()?;
    let frame_count: u64 = meta.frames.iter().map(|f| f.repeat as u64).sum();
    Some(std::time::Duration::from_secs_f64(
        frame_count as f64 / 60.0,
    ))
}

fn hardware_label(hw: HardwareType) -> String {
    match hw {
        HardwareType::NesNtsc => "NES NTSC",
        HardwareType::NesPal => "NES PAL",
        HardwareType::Famicom => "Famicom",
        HardwareType::VsSystem => "VS System",
        HardwareType::Dendy => "Dendy",
        HardwareType::Playchoice10 => "PlayChoice-10",
        HardwareType::NesMultiRegion => "Multi-region",
        HardwareType::FamicomNetworkSystem => "Famicom Net",
        _ => "NES NTSC",
    }
    .to_string()
}

/// Progress information during catalog enrichment.
#[cfg(feature = "native")]
#[derive(Debug, Clone)]
pub struct EnrichmentProgress {
    pub current: usize,
    pub total: usize,
    pub game_title: String,
    pub phase: EnrichmentPhase,
}

/// Current phase of the enrichment pipeline.
#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentPhase {
    MatchingMetadata,
    DownloadingImages,
}

/// Enrich a ROM catalog with TheGamesDB metadata and cached cover art.
///
/// Uses a persistent enrichment cache to skip re-processing ROMs that have
/// already been matched and had their images downloaded. Only new ROMs (not
/// present in the cache) are fuzzy-matched against the metadata DB and have
/// their images downloaded.
///
/// When `rebuild` is true, the enrichment cache is cleared and all ROMs are
/// re-processed from scratch.
///
/// The `progress_callback` is called for each ROM so the UI can display a
/// progress bar. Pass `|_| {}` if no progress reporting is needed.
#[cfg(feature = "native")]
pub fn enrich_catalog(
    catalog: &mut [RomEntry],
    metadata_db_path: &std::path::Path,
    image_cache_path: &std::path::Path,
    rebuild: bool,
    mut progress_callback: impl FnMut(EnrichmentProgress),
) {
    use crate::platform::image_cache::ImageCache;
    use crate::platform::metadata::{MetadataDb, match_title};
    use enrichment_cache::{CachedEnrichment, EnrichmentCache};

    // Load persistent enrichment cache (stored in the image cache directory).
    // When rebuilding, start with a fresh cache.
    let cache_path = image_cache_path.join("enrichment_cache.json");
    let mut ecache = if rebuild {
        EnrichmentCache::load_empty(&cache_path)
    } else {
        EnrichmentCache::load(&cache_path)
    };

    // Apply cached enrichment data for known ROMs. Cached "no match" results
    // (written by older versions) are treated as uncached so they are retried
    // against the current metadata DB — new platforms or freshly synced games
    // would otherwise never be picked up for previously scanned ROMs.
    let mut uncached_indices: Vec<usize> = Vec::new();
    for (i, entry) in catalog.iter_mut().enumerate() {
        match ecache.get(&entry.path) {
            Some(cached) if cached.metadata_game_id.is_some() => {
                entry.metadata_game_id = cached.metadata_game_id;
                if let Some(ref name) = cached.display_name {
                    entry.display_name.clone_from(name);
                    entry.search_key = entry.display_name.to_lowercase();
                }
                entry.genres.clone_from(&cached.genres);
                entry.overview.clone_from(&cached.overview);
                entry.release_date.clone_from(&cached.release_date);
                entry.players = cached.players;
                entry.rating.clone_from(&cached.rating);
                entry.boxart_path.clone_from(&cached.boxart_path);
                entry.screenshot_paths.clone_from(&cached.screenshot_paths);
            }
            _ => uncached_indices.push(i),
        }
    }

    // If everything is cached, we're done.
    if uncached_indices.is_empty() {
        return;
    }

    let db = match MetadataDb::open(metadata_db_path) {
        Some(db) => db,
        None => return,
    };

    // Build per-platform title lists so ROMs only match against their own platform.
    use std::collections::HashMap;
    let mut platform_titles: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for &idx in &uncached_indices {
        let pid = catalog[idx].platform.thegamesdb_id();
        platform_titles
            .entry(pid)
            .or_insert_with(|| db.titles_for_platform(pid));
    }

    let base_url = db.medium_base_url().to_string();
    let uncached_total = uncached_indices.len();

    // Phase 1: Match metadata for uncached ROMs only.
    for (step, &idx) in uncached_indices.iter().enumerate() {
        let entry = &mut catalog[idx];
        progress_callback(EnrichmentProgress {
            current: step + 1,
            total: uncached_total,
            game_title: entry.display_name.clone(),
            phase: EnrichmentPhase::MatchingMetadata,
        });

        let pid = entry.platform.thegamesdb_id();
        let titles = platform_titles
            .get(&pid)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        if let Some(m) = match_title(&entry.display_name, titles) {
            entry.metadata_game_id = Some(m.game_id);
            if let Some(meta) = db.get_game(m.game_id) {
                entry.display_name = meta.title;
                entry.search_key = entry.display_name.to_lowercase();
                entry.genres = meta.genres;
                entry.overview = meta.overview;
                entry.release_date = meta.release_date;
                entry.players = meta.players;
                entry.rating = meta.rating;
            }
        }
    }

    // Phase 2: Download images for uncached ROMs only.
    let image_cache = match ImageCache::new(image_cache_path.to_path_buf(), base_url) {
        Ok(cache) => cache,
        Err(e) => {
            crate::platform::debugging::log_info(format!("Failed to create image cache: {e}"));
            // Still save metadata matches to cache (unmatched ROMs are left
            // out so they are retried on the next startup).
            for &idx in &uncached_indices {
                let entry = &catalog[idx];
                if entry.metadata_game_id.is_none() {
                    continue;
                }
                ecache.insert(
                    entry.path.clone(),
                    CachedEnrichment {
                        metadata_game_id: entry.metadata_game_id,
                        display_name: Some(entry.display_name.clone()),
                        genres: entry.genres.clone(),
                        overview: entry.overview.clone(),
                        release_date: entry.release_date.clone(),
                        players: entry.players,
                        rating: entry.rating.clone(),
                        boxart_path: None,
                        screenshot_paths: Vec::new(),
                    },
                );
            }
            ecache.save();
            return;
        }
    };

    for (step, &idx) in uncached_indices.iter().enumerate() {
        let entry = &mut catalog[idx];
        progress_callback(EnrichmentProgress {
            current: step + 1,
            total: uncached_total,
            game_title: entry.display_name.clone(),
            phase: EnrichmentPhase::DownloadingImages,
        });

        if let Some(game_id) = entry.metadata_game_id
            && let Some(meta) = db.get_game(game_id)
        {
            let boxart_filename = meta.front_boxart();
            let screenshot_filenames: Vec<&str> = meta.screenshots();
            let cached_imgs =
                image_cache.ensure_game_images(boxart_filename, &screenshot_filenames);
            entry.boxart_path = cached_imgs.boxart_path;
            entry.screenshot_paths = cached_imgs.screenshot_paths;
        }

        // Store the enrichment result in the persistent cache. Unmatched
        // ROMs are not cached so they are retried on the next startup.
        if entry.metadata_game_id.is_none() {
            continue;
        }
        ecache.insert(
            entry.path.clone(),
            CachedEnrichment {
                metadata_game_id: entry.metadata_game_id,
                display_name: Some(entry.display_name.clone()),
                genres: entry.genres.clone(),
                overview: entry.overview.clone(),
                release_date: entry.release_date.clone(),
                players: entry.players,
                rating: entry.rating.clone(),
                boxart_path: entry.boxart_path.clone(),
                screenshot_paths: entry.screenshot_paths.clone(),
            },
        );
    }

    ecache.save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::RomDb;
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
        let rom_bytes = minimal_nrom_rom();
        let prg = &rom_bytes[16..16 + 16 * 1024];
        let chr = &rom_bytes[16 + 16 * 1024..];
        let crc = crate::nes::cartridge::calculate_rom_crc32(prg, chr);
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

    #[test]
    fn test_read_recording_duration_sums_rle_repeat_fields() {
        let mut tmp = NamedTempFile::with_suffix(".nes").unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let autorun_path = tmp.path().with_extension("autorun");
        let json = r#"{"version":3,"frames":[{"player1":0,"player2":0,"repeat":3480},{"player1":1,"player2":0,"repeat":240}],"checkpoints":[]}"#;
        std::fs::write(&autorun_path, json.as_bytes()).unwrap();
        let dur = read_recording_duration(tmp.path()).expect("should have duration");
        let _ = std::fs::remove_file(&autorun_path);
        assert_eq!(dur.as_secs(), 62, "3720 frames at 60fps = 62 seconds");
    }

    #[test]
    fn test_build_rom_entry_metadata_fields_are_empty_by_default() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&minimal_nrom_rom()).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);

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

    #[test]
    fn test_platform_from_path_nes() {
        assert_eq!(platform_from_path(Path::new("game.nes")), Platform::Nes);
        assert_eq!(platform_from_path(Path::new("game.NES")), Platform::Nes);
    }

    #[test]
    fn test_platform_from_path_gb() {
        assert_eq!(platform_from_path(Path::new("game.gb")), Platform::Gb);
        assert_eq!(platform_from_path(Path::new("game.GB")), Platform::Gb);
    }

    #[test]
    fn test_platform_from_path_gbc() {
        assert_eq!(platform_from_path(Path::new("game.gbc")), Platform::Gbc);
        assert_eq!(platform_from_path(Path::new("game.GBC")), Platform::Gbc);
    }

    #[test]
    fn test_platform_from_path_gba() {
        assert_eq!(platform_from_path(Path::new("game.gba")), Platform::Gba);
        assert_eq!(platform_from_path(Path::new("game.GBA")), Platform::Gba);
    }

    #[test]
    fn test_platform_from_path_snes() {
        assert_eq!(platform_from_path(Path::new("game.sfc")), Platform::Snes);
        assert_eq!(platform_from_path(Path::new("game.SFC")), Platform::Snes);
        assert_eq!(platform_from_path(Path::new("game.smc")), Platform::Snes);
        assert_eq!(platform_from_path(Path::new("game.SMC")), Platform::Snes);
    }

    #[test]
    fn test_build_rom_entry_gba_file_has_gba_platform() {
        let tmp = NamedTempFile::with_suffix(".gba").unwrap();
        std::fs::write(tmp.path(), [0u8; 256]).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);
        assert_eq!(entry.platform, Platform::Gba);
    }

    #[test]
    fn test_build_rom_entry_sfc_file_has_snes_platform() {
        let tmp = NamedTempFile::with_suffix(".sfc").unwrap();
        std::fs::write(tmp.path(), [0u8; 256]).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);
        assert_eq!(entry.platform, Platform::Snes);
    }

    #[test]
    fn test_build_rom_entry_gb_file_has_gb_platform() {
        let tmp = NamedTempFile::with_suffix(".gb").unwrap();
        std::fs::write(tmp.path(), [0u8; 256]).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);
        assert_eq!(entry.platform, Platform::Gb);
    }

    #[test]
    fn test_build_rom_entry_gbc_file_has_gbc_platform() {
        let tmp = NamedTempFile::with_suffix(".gbc").unwrap();
        std::fs::write(tmp.path(), [0u8; 256]).unwrap();
        let db = RomDb::from_csv_content("");

        let entry = build_rom_entry(tmp.path(), &db);
        assert_eq!(entry.platform, Platform::Gbc);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_enrich_catalog_with_metadata_db() {
        let db_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("metadata-scraper")
            .join("metadata.db");
        if !db_path.exists() {
            eprintln!("Skipping test: metadata.db not found at {db_path:?}");
            return;
        }

        let cache_dir = tempfile::TempDir::new().unwrap();
        let mut catalog = vec![RomEntry {
            path: std::path::PathBuf::from("super_mario_bros_3.nes"),
            display_name: "Super Mario Bros. 3".to_string(),
            search_key: "super mario bros. 3".to_string(),
            mapper_label: "4".to_string(),
            mapper: Some(4),
            hardware: Some("NES NTSC".to_string()),
            crc: Some("AABBCCDD".to_string()),
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
        }];

        let mut progress_count = 0;
        enrich_catalog(&mut catalog, &db_path, cache_dir.path(), false, |_| {
            progress_count += 1
        });

        assert!(
            progress_count > 0,
            "progress callback should have been called"
        );
        let entry = &catalog[0];
        assert!(
            entry.metadata_game_id.is_some(),
            "SMB3 should match a metadata entry"
        );
        assert!(!entry.genres.is_empty(), "SMB3 should have genres");
    }

    /// Build a minimal metadata DB fixture with one SNES game (no images,
    /// so enrichment never attempts a network download).
    #[cfg(feature = "native")]
    fn fixture_metadata_db(dir: &std::path::Path) -> std::path::PathBuf {
        let db_path = dir.join("metadata.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"CREATE TABLE games (
                   id INTEGER PRIMARY KEY,
                   game_title TEXT NOT NULL,
                   release_date TEXT,
                   platform_id INTEGER NOT NULL,
                   players INTEGER,
                   overview TEXT,
                   rating TEXT,
                   alternates TEXT
               );
               INSERT INTO games
                   (id, game_title, release_date, platform_id, players,
                    overview, rating, alternates)
               VALUES
                   (284, 'Advanced Dungeons & Dragons: Eye of the Beholder',
                    '1994-03-01', 6, 1, 'Dungeon crawler.', 'E - Everyone',
                    '["Eye of the Beholder", "AD&D: Eye of the Beholder"]');"#,
        )
        .unwrap();
        db_path
    }

    #[cfg(feature = "native")]
    fn snes_stub_entry(path: std::path::PathBuf) -> RomEntry {
        let display_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        RomEntry {
            search_key: display_name.to_lowercase(),
            display_name,
            path,
            mapper_label: "-".to_string(),
            mapper: None,
            hardware: None,
            crc: None,
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
            platform: Platform::Snes,
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn enrich_catalog_retries_cached_no_match_entries() {
        // Regression: 620 SNES ROMs were stuck without images because they
        // had been cached as "no match" before SNES support existed. Cached
        // misses must be retried, and here the retry succeeds via the
        // alternate title "AD&D: Eye of the Beholder" of game 284.
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = fixture_metadata_db(dir.path());
        let cache_dir = dir.path().join("image_cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let rom_path = dir.path().join("ad-d---eye-of-the-beholder.sfc");
        std::fs::write(&rom_path, [0u8; 64]).unwrap();

        let cache_json = format!(
            r#"{{"entries":{{"{}":{{"metadata_game_id":null,"display_name":null,"genres":[],"overview":null,"release_date":null,"players":null,"rating":null,"boxart_path":null,"screenshot_paths":[]}}}}}}"#,
            rom_path.display()
        );
        std::fs::write(cache_dir.join("enrichment_cache.json"), cache_json).unwrap();

        let mut catalog = vec![snes_stub_entry(rom_path)];
        enrich_catalog(&mut catalog, &db_path, &cache_dir, false, |_| {});

        assert_eq!(
            catalog[0].metadata_game_id,
            Some(284),
            "cached no-match entry should be re-matched against current metadata"
        );
        assert_eq!(
            catalog[0].display_name,
            "Advanced Dungeons & Dragons: Eye of the Beholder"
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn enrich_catalog_does_not_cache_unmatched_roms() {
        // Unmatched ROMs must not be pinned in the cache; leaving them out
        // means they are retried automatically on the next startup.
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = fixture_metadata_db(dir.path());
        let cache_dir = dir.path().join("image_cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let rom_path = dir.path().join("totally-unknown-homebrew.sfc");
        std::fs::write(&rom_path, [0u8; 64]).unwrap();

        let mut catalog = vec![snes_stub_entry(rom_path)];
        enrich_catalog(&mut catalog, &db_path, &cache_dir, false, |_| {});

        assert!(catalog[0].metadata_game_id.is_none());
        let cache_content =
            std::fs::read_to_string(cache_dir.join("enrichment_cache.json")).unwrap();
        assert!(
            !cache_content.contains("totally-unknown-homebrew"),
            "unmatched ROM must not be recorded in the enrichment cache"
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_enrich_catalog_skips_missing_db() {
        let mut catalog = vec![RomEntry {
            path: std::path::PathBuf::from("game.nes"),
            display_name: "Test Game".to_string(),
            search_key: "test game".to_string(),
            mapper_label: "-".to_string(),
            mapper: None,
            hardware: None,
            crc: None,
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
        }];

        // Should not panic with a non-existent DB path.
        enrich_catalog(
            &mut catalog,
            std::path::Path::new("/nonexistent/metadata.db"),
            std::path::Path::new("/nonexistent/cache"),
            false,
            |_| {},
        );

        assert!(catalog[0].metadata_game_id.is_none());
    }

    fn make_entry_with_path(path: &str, name: &str) -> RomEntry {
        RomEntry {
            path: PathBuf::from(path),
            display_name: name.to_string(),
            search_key: name.to_lowercase(),
            mapper_label: "-".to_string(),
            mapper: None,
            hardware: None,
            crc: None,
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
    fn is_unofficial_rom_detects_hack_in_filename() {
        let entry = make_entry_with_path("Super Mario (hack).nes", "Super Mario");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_hack_in_display_name() {
        let entry = make_entry_with_path("game.nes", "Super Mario (Hack)");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_pirate() {
        let entry = make_entry_with_path("Game (Pirate).nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_homebrew() {
        let entry = make_entry_with_path("Game [Homebrew].nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_prototype() {
        let entry = make_entry_with_path("Game (Proto).nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_beta() {
        let entry = make_entry_with_path("Game [Beta].nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_unl() {
        let entry = make_entry_with_path("Game (Unl).nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_allows_official_rom() {
        let entry = make_entry_with_path("Super Mario Bros. 3 (USA).nes", "Super Mario Bros. 3");
        assert!(!is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_allows_region_variants() {
        let entry = make_entry_with_path("Metroid (Europe).nes", "Metroid");
        assert!(!is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_hack_suffix_in_filename() {
        let entry =
            make_entry_with_path("arkanoid-98-arkanoid-hack.nes", "Arkanoid 98 Arkanoid Hack");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_hack_with_spaces() {
        let entry = make_entry_with_path("game hack version.nes", "Game Hack Version");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_hack_with_underscores() {
        let entry = make_entry_with_path("game_hack_v2.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_menu_in_filename() {
        let entry = make_entry_with_path("super-game-menu.nes", "Super Game Menu");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_demo_in_filename() {
        let entry = make_entry_with_path("cool-game-demo.nes", "Cool Game Demo");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_test_in_filename() {
        let entry = make_entry_with_path("mapper-test.nes", "Mapper Test");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_allows_game_with_test_in_title() {
        // "Test Drive" is an actual game — the word "test" must be delimited
        let entry = make_entry_with_path("test-drive.nes", "Test Drive");
        assert!(!is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_bad_suffix() {
        let entry = make_entry_with_path("game-bad.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_p_numbered_suffix() {
        let entry = make_entry_with_path("game-p1.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_p_multi_digit_suffix() {
        let entry = make_entry_with_path("game-p12.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_h_numbered_suffix() {
        let entry = make_entry_with_path("game-h2.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_hm00_suffix() {
        let entry = make_entry_with_path("game-hm00.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_bracketed_b_numbered() {
        let entry = make_entry_with_path("Game [b3].nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_bracketed_p_numbered() {
        let entry = make_entry_with_path("Game [p1].nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_bracketed_h_numbered() {
        let entry = make_entry_with_path("Game [h1].nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_allows_game_with_p_in_title() {
        // "P-47" is an actual game — -p<n> must be at end before extension
        let entry = make_entry_with_path("p-47.nes", "P-47");
        assert!(!is_unofficial_rom(&entry));
    }

    #[test]
    fn has_numbered_dump_suffix_matches_dash_p_digit_dot() {
        assert!(has_numbered_dump_suffix("game-p1.nes"));
        assert!(has_numbered_dump_suffix("game-p99.nes"));
        assert!(has_numbered_dump_suffix("game-h3.nes"));
    }

    #[test]
    fn has_numbered_dump_suffix_matches_bracket_b_digit() {
        assert!(has_numbered_dump_suffix("game [b1].nes"));
        assert!(has_numbered_dump_suffix("game [p2].nes"));
        assert!(has_numbered_dump_suffix("game [h3].nes"));
    }

    #[test]
    fn has_numbered_dump_suffix_ignores_non_numbered() {
        assert!(!has_numbered_dump_suffix("game-play.nes"));
        assert!(!has_numbered_dump_suffix("game-help.nes"));
        assert!(!has_numbered_dump_suffix("test-drive.nes"));
    }

    #[test]
    fn is_unofficial_rom_detects_pd_suffix() {
        let entry = make_entry_with_path("game-pd.nes", "Game");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_zzz_prefix() {
        let entry = make_entry_with_path("zzz-multicart.nes", "ZZZ Multicart");
        assert!(is_unofficial_rom(&entry));
    }

    #[test]
    fn is_unofficial_rom_detects_zzz_with_underscore() {
        let entry = make_entry_with_path("zzz_unknown_game.nes", "ZZZ Unknown Game");
        assert!(is_unofficial_rom(&entry));
    }

    fn make_entry_with_crc(name: &str, crc: Option<&str>) -> RomEntry {
        let mut entry = make_entry_with_path(&format!("{name}.nes"), name);
        entry.crc = crc.map(str::to_string);
        entry
    }

    #[test]
    fn dedup_by_crc_removes_duplicate_crc() {
        let mut entries = vec![
            make_entry_with_crc("Mario A", Some("AABBCCDD")),
            make_entry_with_crc("Mario B", Some("AABBCCDD")),
        ];
        dedup_by_crc(&mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display_name, "Mario A");
    }

    #[test]
    fn dedup_by_crc_keeps_entries_without_crc() {
        let mut entries = vec![
            make_entry_with_crc("No CRC 1", None),
            make_entry_with_crc("No CRC 2", None),
        ];
        dedup_by_crc(&mut entries);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn dedup_by_crc_keeps_distinct_crcs() {
        let mut entries = vec![
            make_entry_with_crc("Game A", Some("11111111")),
            make_entry_with_crc("Game B", Some("22222222")),
        ];
        dedup_by_crc(&mut entries);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn dedup_by_crc_mixed_entries() {
        let mut entries = vec![
            make_entry_with_crc("Alpha", Some("AABBCCDD")),
            make_entry_with_crc("Beta", None),
            make_entry_with_crc("Gamma", Some("AABBCCDD")),
            make_entry_with_crc("Delta", Some("11223344")),
            make_entry_with_crc("Epsilon", None),
        ];
        dedup_by_crc(&mut entries);
        assert_eq!(entries.len(), 4);
        let names: Vec<&str> = entries.iter().map(|e| e.display_name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Beta", "Delta", "Epsilon"]);
    }
}
