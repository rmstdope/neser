// Helper functions for NES2 size parsing were moved to the centralized parser
// in `src/cartridge/ines.rs`. The local copies were removed to avoid
// dead-code warnings.

use neser::cartridge::{ConsoleType, NametableLayout, RomDb, RomDbEntry, TimingMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rom {
    mapper: u16,
    submapper: u8,
    console_type: ConsoleType,
    mirroring: NametableLayout,
    has_trainer: bool,
    header_version: &'static str,
    battery_backed_prg_ram: bool,
    prg_rom_size_bytes: usize,
    chr_rom_size_bytes: usize,
    prg_ram_size_bytes: Option<usize>,
    prg_nvram_size_bytes: Option<usize>,
    chr_ram_size_bytes: Option<usize>,
    chr_nvram_size_bytes: Option<usize>,
    timing_mode: TimingMode,
    vs_ppu_type: Option<u8>,
    vs_hardware_type: Option<u8>,
    misc_roms: u8,
    default_expansion_device: u8,
    rom_crc32: Option<u32>,
    actual_file_size_bytes: usize,
    expected_file_size_bytes: usize,
    file_length_matches_header: bool,
    header_prg_rom_size_bytes: usize,
    header_chr_rom_size_bytes: usize,
    used_db_size_override: bool,
}

fn parse_rom_header(header: &[u8; 16]) -> Option<Rom> {
    let parsed = neser::cartridge::parse_header(header)?;

    Some(Rom {
        mapper: parsed.mapper,
        submapper: parsed.submapper,
        console_type: parsed.console_type,
        mirroring: parsed.mirroring,
        has_trainer: parsed.has_trainer,
        header_version: parsed.header_version,
        battery_backed_prg_ram: parsed.battery_backed_prg_ram,
        prg_rom_size_bytes: parsed.prg_rom_size_bytes,
        chr_rom_size_bytes: parsed.chr_rom_size_bytes,
        prg_ram_size_bytes: parsed.prg_ram_size_bytes,
        prg_nvram_size_bytes: parsed.prg_nvram_size_bytes,
        chr_ram_size_bytes: parsed.chr_ram_size_bytes,
        chr_nvram_size_bytes: parsed.chr_nvram_size_bytes,
        timing_mode: parsed.timing_mode,
        vs_ppu_type: parsed.vs_ppu_type,
        vs_hardware_type: parsed.vs_hardware_type,
        misc_roms: parsed.misc_roms,
        default_expansion_device: parsed.default_expansion_device,
        rom_crc32: None,
        actual_file_size_bytes: 0,
        expected_file_size_bytes: 0,
        file_length_matches_header: true,
        header_prg_rom_size_bytes: parsed.prg_rom_size_bytes,
        header_chr_rom_size_bytes: parsed.chr_rom_size_bytes,
        used_db_size_override: false,
    })
}

fn apply_db_size_overrides(info: &mut Rom, db_entry: &RomDbEntry) {
    let mut overridden = info.used_db_size_override;

    if let Some(prg_size) = db_entry.prg_rom_size {
        let prg_size = prg_size as usize;
        if prg_size != info.prg_rom_size_bytes {
            overridden = true;
        }
        info.prg_rom_size_bytes = prg_size;
    }

    if let Some(chr_size) = db_entry.chr_rom_size {
        let chr_size = chr_size as usize;
        if chr_size != info.chr_rom_size_bytes {
            overridden = true;
        }
        info.chr_rom_size_bytes = chr_size;
    }

    info.used_db_size_override = overridden;
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    List,
    Info(std::path::PathBuf),
    InfoAll,
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    match args {
        [command] if command == "list" => Ok(Command::List),
        [command, path] if command == "info" => {
            let path = std::path::PathBuf::from(path);
            if !path.is_file() {
                return Err(format!(
                    "info requires an existing file path: {}",
                    path.display()
                ));
            }
            Ok(Command::Info(path))
        }
        [command] if command == "info" => Ok(Command::InfoAll),
        _ => Err("Usage: roms list | roms info <path>".to_string()),
    }
}

fn read_rom_from_file(path: &std::path::Path, rom_db: Option<&RomDb>) -> Result<Rom, String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;
    if data.len() < 16 {
        return Err("File too small for iNES header".to_string());
    }

    let header: [u8; 16] = data[0..16]
        .try_into()
        .map_err(|_| "Failed to read iNES header".to_string())?;
    let mut info = parse_rom_header(&header).ok_or_else(|| "Invalid iNES header".to_string())?;

    let trainer_offset = if info.has_trainer { 512 } else { 0 };
    let header_prg_rom_start = 16 + trainer_offset;
    let header_prg_rom_end = header_prg_rom_start + info.prg_rom_size_bytes;
    let header_chr_rom_start = header_prg_rom_end;
    let header_chr_rom_end = header_chr_rom_start + info.chr_rom_size_bytes;

    let payload_start = header_prg_rom_start;
    let payload = if payload_start <= data.len() {
        &data[payload_start..]
    } else {
        &[]
    };
    let fallback_crc32 = neser::cartridge::calculate_rom_crc32(payload, &[]);

    if let Some(rom_db) = rom_db
        && let Some(db_entry) = rom_db.get_by_crc(fallback_crc32)
    {
        apply_db_size_overrides(&mut info, db_entry);
    }

    if data.len() >= header_chr_rom_end {
        let header_prg_rom = &data[header_prg_rom_start..header_prg_rom_end];
        let header_chr_rom = &data[header_chr_rom_start..header_chr_rom_end];
        let crc32 = neser::cartridge::calculate_rom_crc32(header_prg_rom, header_chr_rom);
        info.rom_crc32 = Some(crc32);

        if let Some(rom_db) = rom_db
            && let Some(db_entry) = rom_db.get_by_crc(crc32)
        {
            apply_db_size_overrides(&mut info, db_entry);
        }
    } else {
        info.rom_crc32 = Some(fallback_crc32);
    }

    let prg_rom_start = 16 + trainer_offset;
    let prg_rom_end = prg_rom_start + info.prg_rom_size_bytes;
    let chr_rom_start = prg_rom_end;
    let chr_rom_end = chr_rom_start + info.chr_rom_size_bytes;

    info.actual_file_size_bytes = data.len();
    info.expected_file_size_bytes = chr_rom_end;
    info.file_length_matches_header = data.len() == chr_rom_end;

    if data.len() < chr_rom_end {
        return Err(format!(
            "!!! WARNING: FILE LENGTH DOES NOT MATCH DB/HEADER DECLARATION !!! actual={} expected={} (file too small for PRG/CHR ROM data)",
            data.len(),
            chr_rom_end
        ));
    }

    let prg_rom = &data[prg_rom_start..prg_rom_end];
    let chr_rom = &data[chr_rom_start..chr_rom_end];
    info.rom_crc32 = Some(neser::cartridge::calculate_rom_crc32(prg_rom, chr_rom));

    Ok(info)
}

fn console_type_label(console_type: ConsoleType) -> String {
    match console_type {
        ConsoleType::NesFamicom => "NES/Famicom".to_string(),
        ConsoleType::VsSystem => "Vs. System".to_string(),
        ConsoleType::Playchoice10 => "PlayChoice-10".to_string(),
        ConsoleType::Extended(value) => format!("Extended ({value})"),
    }
}

fn timing_mode_label(timing: TimingMode) -> String {
    match timing {
        TimingMode::Ntsc => "NTSC".to_string(),
        TimingMode::Pal => "PAL".to_string(),
        TimingMode::MultiRegion => "Multi-region".to_string(),
        TimingMode::Dendy => "Dendy".to_string(),
        TimingMode::Unknown(value) => format!("Unknown ({value})"),
    }
}

fn timing_mode_short_label(timing: TimingMode) -> char {
    match timing {
        TimingMode::Pal => 'P',
        TimingMode::Ntsc => 'N',
        TimingMode::MultiRegion | TimingMode::Dendy | TimingMode::Unknown(_) => '?',
    }
}

fn print_rom_info(path: &std::path::Path, info: Rom) {
    println!("ROM: {}", path.display());
    println!("Header version: {}", info.header_version);
    println!("Mapper: {} ({})", info.mapper, info.submapper);
    println!("Console type: {}", console_type_label(info.console_type));
    println!("PRG ROM size: {} bytes", info.prg_rom_size_bytes);
    println!("CHR ROM size: {} bytes", info.chr_rom_size_bytes);
    if let Some(prg_ram_size_bytes) = info.prg_ram_size_bytes {
        println!("PRG-RAM size: {} bytes", prg_ram_size_bytes);
    }
    if let Some(prg_nvram_size_bytes) = info.prg_nvram_size_bytes {
        println!("PRG-NVRAM size: {} bytes", prg_nvram_size_bytes);
    }
    if let Some(chr_ram_size_bytes) = info.chr_ram_size_bytes {
        println!("CHR-RAM size: {} bytes", chr_ram_size_bytes);
    }
    if let Some(chr_nvram_size_bytes) = info.chr_nvram_size_bytes {
        println!("CHR-NVRAM size: {} bytes", chr_nvram_size_bytes);
    }
    println!("Timing mode: {}", timing_mode_label(info.timing_mode));
    if let Some(rom_crc32) = info.rom_crc32 {
        println!("PRG+CHR CRC32: {:08X}", rom_crc32);
    }
    if let Some(vs_ppu_type) = info.vs_ppu_type {
        println!("Vs. PPU type: {vs_ppu_type}");
    }
    if let Some(vs_hardware_type) = info.vs_hardware_type {
        println!("Vs. hardware type: {vs_hardware_type}");
    }
    if info.misc_roms > 0 {
        println!("Misc ROMs: {}", info.misc_roms);
    }
    if info.default_expansion_device > 0 {
        println!(
            "Default expansion device: {}",
            info.default_expansion_device
        );
    }
    println!("Mirroring: {:?}", info.mirroring);
    println!("Trainer: {}", if info.has_trainer { "yes" } else { "no" });
    println!(
        "Battery-backed PRG RAM: {}",
        if info.battery_backed_prg_ram {
            "yes"
        } else {
            "no"
        }
    );

    if info.used_db_size_override {
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        println!("!!! WARNING: HEADER PRG/CHR SIZE DOES NOT MATCH ROM DB ENTRY !!!");
        println!(
            "!!! Header PRG/CHR: {}/{} bytes | DB PRG/CHR: {}/{} bytes !!!",
            info.header_prg_rom_size_bytes,
            info.header_chr_rom_size_bytes,
            info.prg_rom_size_bytes,
            info.chr_rom_size_bytes
        );
        println!("!!! Using ROM DB sizes for validation and display.             !!!");
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    }

    if !info.file_length_matches_header {
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        println!("!!! WARNING: FILE LENGTH DOES NOT MATCH HEADER DECLARATION !!!");
        println!(
            "!!! Actual size: {} bytes | Expected from DB/header: {} bytes !!!",
            info.actual_file_size_bytes, info.expected_file_size_bytes
        );
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    }
}

fn collect_roms(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut roms = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(roms),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            roms.extend(collect_roms(&path)?);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("nes"))
            .unwrap_or(false)
        {
            roms.push(path);
        }
    }
    Ok(roms)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = parse_command(&args).map_err(|err| {
        eprintln!("{err}");
        err
    })?;

    match command {
        Command::List => {
            let root = std::path::Path::new("roms/games");
            let mut roms = collect_roms(root)?;
            roms.sort();

            for rom in roms {
                match read_rom_from_file(&rom, None) {
                    Ok(info) => {
                        let display_path = rom.strip_prefix(root).unwrap_or(&rom);
                        println!(
                            "{:03} {} {}",
                            info.mapper,
                            timing_mode_short_label(info.timing_mode),
                            display_path.display()
                        );
                    }
                    Err(err) => {
                        let display_path = rom.strip_prefix(root).unwrap_or(&rom);
                        eprintln!("{}: {err}", display_path.display());
                    }
                }
            }
        }
        Command::Info(path) => {
            let rom_db = RomDb::new()?;
            let info = read_rom_from_file(&path, Some(&rom_db))
                .map_err(|err| format!("{}: {err}", path.display()))?;
            print_rom_info(&path, info);
        }
        Command::InfoAll => {
            let rom_db = RomDb::new()?;
            let root = std::path::Path::new("roms/games");
            let mut roms = collect_roms(root)?;
            roms.sort();

            let mut first = true;
            for rom in roms {
                if !first {
                    println!("============================");
                }
                first = false;

                match read_rom_from_file(&rom, Some(&rom_db)) {
                    Ok(info) => print_rom_info(&rom, info),
                    Err(err) => eprintln!("{}: {err}", rom.display()),
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_collect_roms_filters_nes_files() {
        let root = std::path::Path::new("roms/games");
        let roms = collect_roms(root).expect("collect roms");
        assert!(roms.iter().all(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("nes"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn test_collect_roms_missing_directory_returns_empty() {
        let root = std::path::Path::new("roms/does-not-exist");
        let roms = collect_roms(root).expect("collect roms");
        assert!(roms.is_empty());
    }

    #[test]
    fn test_parse_command_list() {
        let args = vec!["list".to_string()];
        let command = parse_command(&args).expect("parse command");
        assert!(matches!(command, Command::List));
    }

    #[test]
    fn test_parse_command_info_without_path_lists_all() {
        let args = vec!["info".to_string()];
        let command = parse_command(&args).expect("parse command");
        assert!(matches!(command, Command::InfoAll));
    }

    #[test]
    fn test_parse_command_info_requires_existing_file() {
        let missing_path = PathBuf::from("roms/does-not-exist/missing.nes");
        let args = vec!["info".to_string(), missing_path.display().to_string()];
        let err = parse_command(&args).expect_err("should fail with missing file");
        assert!(err.to_lowercase().contains("file"));
    }

    #[test]
    fn test_parse_ines_header_extracts_fields() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 2;
        header[5] = 1;
        header[6] = 0b0000_0011;
        header[7] = 0b0101_0000;
        header[8] = 2;

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.prg_rom_size_bytes, 2 * 16 * 1024);
        assert_eq!(info.chr_rom_size_bytes, 8 * 1024);
        assert_eq!(info.prg_ram_size_bytes, Some(2 * 8 * 1024));
        assert_eq!(info.mapper, 0x50);
        assert_eq!(info.mirroring, NametableLayout::Vertical);
        assert!(info.battery_backed_prg_ram);
        assert!(!info.has_trainer);
    }

    #[test]
    fn test_parse_ines_header_defaults_prg_ram_to_8kb() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;
        header[6] = 0b0000_0000;
        header[7] = 0b0000_0000;
        header[8] = 0;

        let info = parse_rom_header(&header).expect("parse header");
        assert_eq!(info.prg_ram_size_bytes, Some(8 * 1024));
    }

    #[test]
    fn test_parse_rom_header_nes2_extracts_fields() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 0x02;
        header[5] = 0x03;
        header[6] = 0b0010_0011;
        header[7] = 0b1010_1001;
        header[8] = 0b0011_0100;
        header[9] = 0b0010_0001;
        header[10] = 0b1000_0111;
        header[11] = 0b0110_0101;
        header[12] = 0x03;
        header[13] = 0b1011_0111;
        header[14] = 0x02;
        header[15] = 0x1A;

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.mapper, 0x4A2);
        assert_eq!(info.submapper, 0x3);
        assert_eq!(info.console_type, ConsoleType::VsSystem);
        assert_eq!(info.mirroring, NametableLayout::Vertical);
        assert!(info.battery_backed_prg_ram);
        assert!(!info.has_trainer);
        assert_eq!(info.prg_rom_size_bytes, 4_227_072);
        assert_eq!(info.chr_rom_size_bytes, 4_218_880);
        assert_eq!(info.prg_ram_size_bytes, Some(8_192));
        assert_eq!(info.prg_nvram_size_bytes, Some(16_384));
        assert_eq!(info.chr_ram_size_bytes, Some(2_048));
        assert_eq!(info.chr_nvram_size_bytes, Some(4_096));
        assert_eq!(info.timing_mode, TimingMode::Dendy);
        assert_eq!(info.vs_ppu_type, Some(0x7));
        assert_eq!(info.vs_hardware_type, Some(0xB));
        assert_eq!(info.misc_roms, 0x02);
        assert_eq!(info.default_expansion_device, 0x1A);
    }

    #[test]
    fn test_parse_rom_header_extended_console_type() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[7] = 0b0000_1011;
        header[13] = 0x09;

        let info = parse_rom_header(&header).expect("parse header");
        assert_eq!(info.console_type, ConsoleType::Extended(0x09));
    }

    #[test]
    fn test_read_rom_from_file_sets_crc32() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;

        let prg_rom = vec![0xAA; 16 * 1024];
        let chr_rom = vec![0xBB; 8 * 1024];
        let expected_crc = neser::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

        let mut rom_bytes = header.to_vec();
        rom_bytes.extend_from_slice(&prg_rom);
        rom_bytes.extend_from_slice(&chr_rom);

        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("neser-rom-crc-{nonce}.nes"));

        std::fs::write(&path, &rom_bytes).expect("write temp rom");
        let info = read_rom_from_file(&path, None).expect("read temp rom");
        let _ = std::fs::remove_file(&path);

        assert_eq!(info.rom_crc32, Some(expected_crc));
        assert_eq!(info.actual_file_size_bytes, rom_bytes.len());
        assert_eq!(
            info.expected_file_size_bytes,
            16 + info.prg_rom_size_bytes + info.chr_rom_size_bytes
        );
        assert!(info.file_length_matches_header);
        assert!(!info.used_db_size_override);
    }

    #[test]
    fn test_read_rom_from_file_detects_trailing_bytes_length_mismatch() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;

        let prg_rom = vec![0xAA; 16 * 1024];
        let chr_rom = vec![0xBB; 8 * 1024];
        let trailer = vec![0xCC; 128];

        let mut rom_bytes = header.to_vec();
        rom_bytes.extend_from_slice(&prg_rom);
        rom_bytes.extend_from_slice(&chr_rom);
        rom_bytes.extend_from_slice(&trailer);

        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("neser-rom-len-{nonce}.nes"));

        std::fs::write(&path, &rom_bytes).expect("write temp rom");
        let info = read_rom_from_file(&path, None).expect("read temp rom");
        let _ = std::fs::remove_file(&path);

        assert_eq!(info.actual_file_size_bytes, rom_bytes.len());
        assert_eq!(
            info.expected_file_size_bytes,
            16 + info.prg_rom_size_bytes + info.chr_rom_size_bytes
        );
        assert!(!info.file_length_matches_header);
    }

    #[test]
    fn test_read_rom_from_file_uses_db_sizes_when_mismatching_header() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;

        let prg_rom = vec![0xAA; 16 * 1024];
        let chr_rom = vec![0xBB; 8 * 1024];
        let expected_crc = neser::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

        let mut rom_bytes = header.to_vec();
        rom_bytes.extend_from_slice(&prg_rom);
        rom_bytes.extend_from_slice(&chr_rom);

        let mut rom_path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        rom_path.push(format!("neser-rom-db-override-{nonce}.nes"));
        std::fs::write(&rom_path, &rom_bytes).expect("write temp rom");

        let mut csv_path = std::env::temp_dir();
        csv_path.push(format!("neser-rom-db-override-{nonce}.csv"));
        let columns = vec![
            "1".to_string(),
            "Test".to_string(),
            "".to_string(),
            format!("{expected_crc:08X}"),
            "".to_string(),
            "".to_string(),
            "Licensed Test".to_string(),
            "4".to_string(),
            "".to_string(),
            "H".to_string(),
            "16384".to_string(),
            "00000000".to_string(),
            "".to_string(),
            "".to_string(),
            "0".to_string(),
            "00000000".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "1".to_string(),
        ];
        let csv = format!("{}\n", columns.join(","));
        std::fs::write(&csv_path, csv).expect("write temp db");

        let rom_db = RomDb::from_path(&csv_path).expect("load temp db");
        let info = read_rom_from_file(&rom_path, Some(&rom_db)).expect("read temp rom");

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&csv_path);

        assert_eq!(info.header_prg_rom_size_bytes, 16 * 1024);
        assert_eq!(info.header_chr_rom_size_bytes, 8 * 1024);
        assert_eq!(info.prg_rom_size_bytes, 16 * 1024);
        assert_eq!(info.chr_rom_size_bytes, 0);
        assert!(info.used_db_size_override);
    }
}
