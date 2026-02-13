use super::types::{AUTORUN_VERSION, AutorunFile};
use crate::cartridge::calculate_rom_crc32;
use std::path::{Path, PathBuf};

pub fn autorun_path_for_rom(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("autorun")
}

pub fn crc32(data: &[u8]) -> u32 {
    calculate_rom_crc32(data, &[])
}

pub fn save_autorun_file(path: &Path, file: &AutorunFile) -> Result<(), String> {
    if file.version != AUTORUN_VERSION {
        return Err(format!("Unsupported autorun version: {}", file.version));
    }

    let data = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("Failed to serialize autorun file: {e}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create autorun directory: {e}"))?;
    }
    std::fs::write(path, data)
        .map_err(|e| format!("Failed to write autorun file {}: {e}", path.display()))
}

pub fn load_autorun_file(path: &Path) -> Result<AutorunFile, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("Failed to read autorun file {}: {e}", path.display()))?;
    let file: AutorunFile = serde_json::from_slice(&data)
        .map_err(|e| format!("Failed to deserialize autorun file: {e}"))?;
    if file.version != AUTORUN_VERSION {
        return Err(format!("Unsupported autorun version: {}", file.version));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autorun::{AUTORUN_VERSION, AutorunFile, AutorunFrame};
    use tempfile::NamedTempFile;

    #[test]
    fn test_autorun_path_for_rom_replaces_extension() {
        let rom_path = Path::new("roms/games/pac-man.nes");
        let expected = Path::new("roms/games/pac-man.autorun");
        assert_eq!(autorun_path_for_rom(rom_path), expected);
    }

    #[test]
    fn test_crc32_matches_known_value() {
        let value = crc32(b"NESER");
        assert_eq!(value, 0xEBBAA24B);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0b0000_0001,
                    player2: 0b0001_0000,
                },
                AutorunFrame {
                    player1: 0b0000_0010,
                    player2: 0b0010_0000,
                },
            ],
            checksum: 0x8BB98613,
        };

        save_autorun_file(temp.path(), &file).expect("save autorun file");
        let loaded = load_autorun_file(temp.path()).expect("load autorun file");

        assert_eq!(loaded, file);
    }
}
