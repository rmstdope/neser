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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
