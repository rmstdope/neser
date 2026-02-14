// Helper functions for NES2 size parsing were moved to the centralized parser
// in `src/cartridge/ines.rs`. The local copies were removed to avoid
// dead-code warnings.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsoleType {
    NesFamicom,
    VsSystem,
    Playchoice10,
    Extended(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingMode {
    Ntsc,
    Pal,
    MultiRegion,
    Dendy,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rom {
    mapper: u16,
    submapper: u8,
    console_type: ConsoleType,
    mirroring: Mirroring,
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
}

fn parse_rom_header(header: &[u8; 16]) -> Option<Rom> {
    // Delegate parsing to centralized parser and convert to local `Rom` struct.
    let parsed = neser::cartridge::parse_header(header)?;

    let console_type = match parsed.console_type {
        neser::cartridge::ConsoleType::NesFamicom => ConsoleType::NesFamicom,
        neser::cartridge::ConsoleType::VsSystem => ConsoleType::VsSystem,
        neser::cartridge::ConsoleType::Playchoice10 => ConsoleType::Playchoice10,
        neser::cartridge::ConsoleType::Extended(v) => ConsoleType::Extended(v),
    };

    let mirroring = match parsed.mirroring {
        neser::cartridge::Mirroring::Horizontal => Mirroring::Horizontal,
        neser::cartridge::Mirroring::Vertical => Mirroring::Vertical,
        neser::cartridge::Mirroring::FourScreen => Mirroring::FourScreen,
    };

    let timing_mode = match parsed.timing_mode {
        neser::cartridge::TimingMode::Ntsc => TimingMode::Ntsc,
        neser::cartridge::TimingMode::Pal => TimingMode::Pal,
        neser::cartridge::TimingMode::MultiRegion => TimingMode::MultiRegion,
        neser::cartridge::TimingMode::Dendy => TimingMode::Dendy,
        neser::cartridge::TimingMode::Unknown(v) => TimingMode::Unknown(v),
    };

    Some(Rom {
        mapper: parsed.mapper,
        submapper: parsed.submapper,
        console_type,
        mirroring,
        has_trainer: parsed.has_trainer,
        header_version: parsed.header_version,
        battery_backed_prg_ram: parsed.battery_backed_prg_ram,
        prg_rom_size_bytes: parsed.prg_rom_size_bytes,
        chr_rom_size_bytes: parsed.chr_rom_size_bytes,
        prg_ram_size_bytes: parsed.prg_ram_size_bytes,
        prg_nvram_size_bytes: parsed.prg_nvram_size_bytes,
        chr_ram_size_bytes: parsed.chr_ram_size_bytes,
        chr_nvram_size_bytes: parsed.chr_nvram_size_bytes,
        timing_mode,
        vs_ppu_type: parsed.vs_ppu_type,
        vs_hardware_type: parsed.vs_hardware_type,
        misc_roms: parsed.misc_roms,
        default_expansion_device: parsed.default_expansion_device,
        rom_crc32: None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
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

fn read_rom_from_file(path: &std::path::Path) -> Result<Rom, String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;
    if data.len() < 16 {
        return Err("File too small for iNES header".to_string());
    }

    let header: [u8; 16] = data[0..16]
        .try_into()
        .map_err(|_| "Failed to read iNES header".to_string())?;
    let mut info = parse_rom_header(&header).ok_or_else(|| "Invalid iNES header".to_string())?;

    let trainer_offset = if info.has_trainer { 512 } else { 0 };
    let prg_rom_start = 16 + trainer_offset;
    let prg_rom_end = prg_rom_start + info.prg_rom_size_bytes;
    let chr_rom_start = prg_rom_end;
    let chr_rom_end = chr_rom_start + info.chr_rom_size_bytes;

    if data.len() < chr_rom_end {
        return Err("File too small for PRG/CHR ROM data".to_string());
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
                match read_rom_from_file(&rom) {
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
            let info =
                read_rom_from_file(&path).map_err(|err| format!("{}: {err}", path.display()))?;
            print_rom_info(&path, info);
        }
        Command::InfoAll => {
            let root = std::path::Path::new("roms/games");
            let mut roms = collect_roms(root)?;
            roms.sort();

            let mut first = true;
            for rom in roms {
                if !first {
                    println!("============================");
                }
                first = false;

                match read_rom_from_file(&rom) {
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
        header[4] = 2; // 2 * 16KB PRG ROM
        header[5] = 1; // 1 * 8KB CHR ROM
        header[6] = 0b0000_0011; // vertical mirroring + battery
        header[7] = 0b0101_0000; // mapper upper nibble = 0x5
        header[8] = 2; // 16KB PRG RAM (2 * 8KB)

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.prg_rom_size_bytes, 2 * 16 * 1024);
        assert_eq!(info.chr_rom_size_bytes, 8 * 1024);
        assert_eq!(info.prg_ram_size_bytes, Some(2 * 8 * 1024));
        assert_eq!(info.mapper, 0x50);
        assert_eq!(info.mirroring, Mirroring::Vertical);
        assert!(info.battery_backed_prg_ram);
        assert!(!info.has_trainer);
    }

    #[test]
    fn test_parse_ines_header_defaults_prg_ram_to_8kb() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1; // 1 * 16KB PRG ROM
        header[5] = 1; // 1 * 8KB CHR ROM
        header[6] = 0b0000_0000; // horizontal mirroring
        header[7] = 0b0000_0000; // mapper 0
        header[8] = 0; // iNES 1.0 default PRG-RAM size

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.prg_ram_size_bytes, Some(8 * 1024));
    }

    #[test]
    fn test_parse_rom_header_nes2_extracts_fields() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 0x02;
        header[5] = 0x03;
        header[6] = 0b0010_0011; // mapper low nibble=2, vertical mirroring + battery
        header[7] = 0b1010_1001; // mapper high nibble=0xA, NES2.0 id, VS System
        header[8] = 0b0011_0100; // submapper=3, mapper MSB=4
        header[9] = 0b0010_0001; // chr msb=2, prg msb=1
        header[10] = 0b1000_0111; // PRG-NVRAM=8, PRG-RAM=7
        header[11] = 0b0110_0101; // CHR-NVRAM=6, CHR-RAM=5
        header[12] = 0x03; // Dendy
        header[13] = 0b1011_0111; // VS hardware=0xB, VS PPU=0x7
        header[14] = 0x02; // misc ROMs
        header[15] = 0x1A; // default expansion device

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.mapper, 0x4A2);
        assert_eq!(info.submapper, 0x3);
        assert_eq!(info.console_type, ConsoleType::VsSystem);
        assert_eq!(info.mirroring, Mirroring::Vertical);
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
        header[7] = 0b0000_1011; // NES2.0 id, extended console type
        header[13] = 0x09; // extended console type id

        let info = parse_rom_header(&header).expect("parse header");

        assert_eq!(info.console_type, ConsoleType::Extended(0x09));
    }

    #[test]
    fn test_read_rom_from_file_sets_crc32() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[4] = 1; // 1 * 16KB PRG ROM
        header[5] = 1; // 1 * 8KB CHR ROM

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
        let info = read_rom_from_file(&path).expect("read temp rom");
        let _ = std::fs::remove_file(&path);

        assert_eq!(info.rom_crc32, Some(expected_crc));
    }
}
