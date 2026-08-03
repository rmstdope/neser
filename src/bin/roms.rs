// Helper functions for NES2 size parsing were moved to the centralized parser
// in `src/cartridge/ines.rs`. The local copies were removed to avoid
// dead-code warnings.

use neser::nes::cartridge::{ConsoleType, ParsedRom, RomDb, RomParseError, TimingMode};

const DEFAULT_LIST_ROOT: &str = "roms/games";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    List(std::path::PathBuf),
    Info(std::path::PathBuf),
    InfoAll,
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    match args {
        [command] if command == "list" => {
            Ok(Command::List(std::path::PathBuf::from(DEFAULT_LIST_ROOT)))
        }
        [command, path] if command == "list" => Ok(Command::List(std::path::PathBuf::from(path))),
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
        _ => Err("Usage: roms list [path] | roms info <path>".to_string()),
    }
}

fn read_rom_from_file(
    path: &std::path::Path,
    rom_db: &RomDb,
) -> Result<(ParsedRom, usize), String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;

    let parsed = ParsedRom::parse(&data, Some(rom_db)).map_err(|err| match err {
        RomParseError::InvalidHeader => "Invalid iNES header".to_string(),
        RomParseError::FileTooSmall { expected, actual } => format!(
            "!!! WARNING: FILE LENGTH DOES NOT MATCH DB/HEADER DECLARATION !!! actual={} expected={} (file too small for PRG/CHR ROM data)",
            actual, expected
        ),
    })?;

    Ok((parsed, data.len()))
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

fn expected_file_size_bytes(parsed: &ParsedRom) -> usize {
    let trainer_offset = if parsed.header.has_trainer { 512 } else { 0 };
    16 + trainer_offset + parsed.header.prg_rom_size_bytes + parsed.header.chr_rom_size_bytes
}

fn print_rom_info(path: &std::path::Path, parsed: &ParsedRom, actual_file_size_bytes: usize) {
    println!("ROM: {}", path.display());
    println!("Header version: {}", parsed.header.header_version);
    println!(
        "Mapper: {} ({})",
        parsed.header.mapper, parsed.header.submapper
    );
    println!(
        "Console type: {}",
        console_type_label(parsed.header.console_type)
    );
    println!("PRG ROM size: {} bytes", parsed.header.prg_rom_size_bytes);
    println!("CHR ROM size: {} bytes", parsed.header.chr_rom_size_bytes);
    if let Some(prg_ram_size_bytes) = parsed.header.prg_ram_size_bytes {
        println!("PRG-RAM size: {} bytes", prg_ram_size_bytes);
    }
    if let Some(prg_nvram_size_bytes) = parsed.header.prg_nvram_size_bytes {
        println!("PRG-NVRAM size: {} bytes", prg_nvram_size_bytes);
    }
    if let Some(chr_ram_size_bytes) = parsed.header.chr_ram_size_bytes {
        println!("CHR-RAM size: {} bytes", chr_ram_size_bytes);
    }
    if let Some(chr_nvram_size_bytes) = parsed.header.chr_nvram_size_bytes {
        println!("CHR-NVRAM size: {} bytes", chr_nvram_size_bytes);
    }
    println!(
        "Timing mode: {}",
        timing_mode_label(parsed.header.timing_mode)
    );
    println!("PRG+CHR CRC32: {:08X}", parsed.crc32);
    if let Some(vs_ppu_type) = parsed.header.vs_ppu_type {
        println!("Vs. PPU type: {vs_ppu_type}");
    }
    if let Some(vs_hardware_type) = parsed.header.vs_hardware_type {
        println!("Vs. hardware type: {vs_hardware_type}");
    }
    if parsed.header.misc_roms > 0 {
        println!("Misc ROMs: {}", parsed.header.misc_roms);
    }
    if parsed.header.default_expansion_device > 0 {
        println!(
            "Default expansion device: {}",
            parsed.header.default_expansion_device
        );
    }
    println!("Mirroring: {:?}", parsed.header.mirroring);
    println!(
        "Trainer: {}",
        if parsed.header.has_trainer {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "Battery-backed PRG RAM: {}",
        if parsed.header.battery_backed_prg_ram {
            "yes"
        } else {
            "no"
        }
    );

    let expected_file_size_bytes = expected_file_size_bytes(parsed);
    if actual_file_size_bytes != expected_file_size_bytes {
        println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        println!("!!! WARNING: FILE LENGTH DOES NOT MATCH HEADER DECLARATION !!!");
        println!(
            "!!! Actual size: {} bytes | Expected from DB/header: {} bytes !!!",
            actual_file_size_bytes, expected_file_size_bytes
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
        Command::List(root) => {
            let rom_db = RomDb::new()?;
            let mut roms = collect_roms(&root)?;
            roms.sort();

            for rom in roms {
                match read_rom_from_file(&rom, &rom_db) {
                    Ok((parsed, _)) => {
                        let display_path = rom.strip_prefix(&root).unwrap_or(&rom);
                        println!(
                            "{:03} {} {}",
                            parsed.header.mapper,
                            timing_mode_short_label(parsed.header.timing_mode),
                            display_path.display()
                        );
                    }
                    Err(err) => {
                        let display_path = rom.strip_prefix(&root).unwrap_or(&rom);
                        eprintln!("{}: {err}", display_path.display());
                    }
                }
            }
        }
        Command::Info(path) => {
            let rom_db = RomDb::new()?;
            let (parsed, actual_file_size_bytes) = read_rom_from_file(&path, &rom_db)
                .map_err(|err| format!("{}: {err}", path.display()))?;
            print_rom_info(&path, &parsed, actual_file_size_bytes);
        }
        Command::InfoAll => {
            let rom_db = RomDb::new()?;
            let root = std::path::Path::new(DEFAULT_LIST_ROOT);
            let mut roms = collect_roms(root)?;
            roms.sort();

            for (i, rom) in roms.iter().enumerate() {
                if i > 0 {
                    println!("============================");
                }

                match read_rom_from_file(rom, &rom_db) {
                    Ok((parsed, actual_file_size_bytes)) => {
                        print_rom_info(rom, &parsed, actual_file_size_bytes)
                    }
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
        assert_eq!(command, Command::List(PathBuf::from(DEFAULT_LIST_ROOT)));
    }

    #[test]
    fn test_parse_command_list_with_path() {
        let args = vec!["list".to_string(), "roms".to_string()];
        let command = parse_command(&args).expect("parse command");
        assert_eq!(command, Command::List(PathBuf::from("roms")));
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
    fn test_read_rom_from_file_sets_crc32() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;

        let prg_rom = vec![0xAA; 16 * 1024];
        let chr_rom = vec![0xBB; 8 * 1024];
        let expected_crc = neser::nes::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

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
        let rom_db = RomDb::new().expect("load rom db");
        let (parsed, actual_file_size_bytes) =
            read_rom_from_file(&path, &rom_db).expect("read temp rom");
        let _ = std::fs::remove_file(&path);

        assert_eq!(parsed.crc32, expected_crc);
        assert_eq!(actual_file_size_bytes, rom_bytes.len());
        assert_eq!(
            expected_file_size_bytes(&parsed),
            16 + parsed.header.prg_rom_size_bytes + parsed.header.chr_rom_size_bytes
        );
        assert_eq!(actual_file_size_bytes, expected_file_size_bytes(&parsed));
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
        let rom_db = RomDb::new().expect("load rom db");
        let (parsed, actual_file_size_bytes) =
            read_rom_from_file(&path, &rom_db).expect("read temp rom");
        let _ = std::fs::remove_file(&path);

        assert_eq!(actual_file_size_bytes, rom_bytes.len());
        assert_ne!(actual_file_size_bytes, expected_file_size_bytes(&parsed));
    }

    #[test]
    fn test_read_rom_from_file_uses_db_sizes_when_mismatching_header() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1;
        header[5] = 1;

        let prg_rom = vec![0xAA; 16 * 1024];
        let chr_rom = vec![0xBB; 8 * 1024];
        let expected_crc = neser::nes::cartridge::calculate_rom_crc32(&prg_rom, &chr_rom);

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
        let (parsed, actual_file_size_bytes) =
            read_rom_from_file(&rom_path, &rom_db).expect("read temp rom");

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&csv_path);

        assert_eq!(parsed.header.prg_rom_size_bytes, 16 * 1024);
        assert_eq!(parsed.header.chr_rom_size_bytes, 0);
        assert_eq!(parsed.header.mapper, 4);
        assert_ne!(actual_file_size_bytes, expected_file_size_bytes(&parsed));
    }
}
