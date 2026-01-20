fn parse_mapper_number(header: &[u8; 16]) -> Option<u8> {
    if &header[0..4] != b"NES\x1A" {
        return None;
    }

    let flags6 = header[6];
    let flags7 = header[7];
    Some((flags6 >> 4) | (flags7 & 0xF0))
}

fn read_mapper_from_file(path: &std::path::Path) -> Result<u8, String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;
    if data.len() < 16 {
        return Err("File too small for iNES header".to_string());
    }

    let header: [u8; 16] = data[0..16]
        .try_into()
        .map_err(|_| "Failed to read iNES header".to_string())?;
    parse_mapper_number(&header).ok_or_else(|| "Invalid iNES header".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InesInfo {
    prg_rom_banks: u8,
    chr_rom_banks: u8,
    prg_ram_banks_8k: u8,
    mapper: u8,
    mirroring: Mirroring,
    has_trainer: bool,
    battery_backed_prg_ram: bool,
}

fn parse_ines_header(header: &[u8; 16]) -> Option<InesInfo> {
    if &header[0..4] != b"NES\x1A" {
        return None;
    }

    let prg_rom_banks = header[4];
    let chr_rom_banks = header[5];
    let flags6 = header[6];
    let flags7 = header[7];
    let prg_ram_banks_8k = header[8].max(1);

    let mapper = (flags6 >> 4) | (flags7 & 0xF0);
    let has_trainer = (flags6 & 0x04) != 0;
    let battery_backed_prg_ram = (flags6 & 0x02) != 0;
    let mirroring = if (flags6 & 0x08) != 0 {
        Mirroring::FourScreen
    } else if (flags6 & 0x01) != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    Some(InesInfo {
        prg_rom_banks,
        chr_rom_banks,
        prg_ram_banks_8k,
        mapper,
        mirroring,
        has_trainer,
        battery_backed_prg_ram,
    })
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

fn read_ines_info_from_file(path: &std::path::Path) -> Result<InesInfo, String> {
    let data = std::fs::read(path).map_err(|err| err.to_string())?;
    if data.len() < 16 {
        return Err("File too small for iNES header".to_string());
    }

    let header: [u8; 16] = data[0..16]
        .try_into()
        .map_err(|_| "Failed to read iNES header".to_string())?;
    parse_ines_header(&header).ok_or_else(|| "Invalid iNES header".to_string())
}

fn print_ines_info(path: &std::path::Path, info: InesInfo) {
    println!("ROM: {}", path.display());
    println!("Mapper: {}", info.mapper);
    println!(
        "PRG ROM banks: {} ({} bytes)",
        info.prg_rom_banks,
        info.prg_rom_banks as usize * 16_384
    );
    println!(
        "CHR ROM banks: {} ({} bytes)",
        info.chr_rom_banks,
        info.chr_rom_banks as usize * 8_192
    );
    println!(
        "PRG RAM banks (8KB): {} ({} bytes)",
        info.prg_ram_banks_8k,
        info.prg_ram_banks_8k as usize * 8_192
    );
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
                match read_mapper_from_file(&rom) {
                    Ok(mapper) => {
                        let display_path = rom.strip_prefix(root).unwrap_or(&rom);
                        println!("{mapper:03} {}", display_path.display());
                    }
                    Err(err) => {
                        let display_path = rom.strip_prefix(root).unwrap_or(&rom);
                        eprintln!("{}: {err}", display_path.display());
                    }
                }
            }
        }
        Command::Info(path) => {
            let info = read_ines_info_from_file(&path)
                .map_err(|err| format!("{}: {err}", path.display()))?;
            print_ines_info(&path, info);
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

                match read_ines_info_from_file(&rom) {
                    Ok(info) => print_ines_info(&rom, info),
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
    fn test_parse_mapper_number_from_ines_header() {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(b"NES\x1A");
        header[6] = 0x20; // lower mapper nibble = 0x2
        header[7] = 0x10; // upper mapper nibble = 0x1

        assert_eq!(parse_mapper_number(&header), Some(0x12));
    }

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

        let info = parse_ines_header(&header).expect("parse header");

        assert_eq!(info.prg_rom_banks, 2);
        assert_eq!(info.chr_rom_banks, 1);
        assert_eq!(info.prg_ram_banks_8k, 2);
        assert_eq!(info.mapper, 0x50);
        assert_eq!(info.mirroring, Mirroring::Vertical);
        assert!(info.battery_backed_prg_ram);
        assert!(!info.has_trainer);
    }
}
