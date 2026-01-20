use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const AUTORUN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFrame {
    pub player1: u8,
    pub player2: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFile {
    pub version: u32,
    pub frames: Vec<AutorunFrame>,
    pub checksum: u32,
}

pub fn autorun_path_for_rom(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("autorun")
}

pub fn crc32(data: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut j = 0;
            while j < 8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };

    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    !crc
}

pub fn save_autorun_file(path: &Path, file: &AutorunFile) -> Result<(), String> {
    if file.version != AUTORUN_VERSION {
        return Err(format!(
            "Unsupported autorun version: {}",
            file.version
        ));
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
        return Err(format!(
            "Unsupported autorun version: {}",
            file.version
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
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
