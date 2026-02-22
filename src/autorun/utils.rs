use super::types::{AUTORUN_VERSION, AutorunFile};
use crate::cartridge::calculate_rom_crc32;
use std::path::{Path, PathBuf};

pub fn autorun_path_for_rom(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("autorun")
}

/// Compute a CRC-32 checksum of arbitrary bytes (used for screen CRC comparisons).
#[allow(dead_code)]
pub fn crc32(data: &[u8]) -> u32 {
    calculate_rom_crc32(data, &[])
}

/// Back up an existing autorun file by copying it to `<path>.bak`.
///
/// Does nothing if the file does not exist.
pub fn backup_autorun_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let backup = path.with_extension("autorun.bak");
    std::fs::copy(path, &backup)
        .map(|_| ())
        .map_err(|e| format!("Failed to backup autorun file {}: {e}", path.display()))
}

/// Trim the last `n` checkpoints from a recording, also removing the corresponding frames.
///
/// After trimming, the recording ends at the frame just before the first removed checkpoint.
/// If `n` is zero or greater than the number of checkpoints, the recording is cleared completely.
pub fn trim_recording(file: &mut AutorunFile, n: usize) {
    if n == 0 {
        return;
    }
    let keep = file.checkpoints.len().saturating_sub(n);
    file.checkpoints.truncate(keep);
    // Trim frames to the last remaining checkpoint boundary (or zero if none left).
    let frame_limit = file
        .checkpoints
        .last()
        .map(|cp| cp.frame_index as usize + 1)
        .unwrap_or(0);
    file.frames.truncate(frame_limit);
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
    use super::super::types::{AutorunCheckpoint, AutorunFrame};
    use super::*;
    use tempfile::NamedTempFile;

    fn sample_file_with_checkpoints() -> AutorunFile {
        AutorunFile {
            version: AUTORUN_VERSION,
            frames: (0..600)
                .map(|i| AutorunFrame {
                    player1: (i % 256) as u8,
                    player2: 0,
                })
                .collect(),
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 299,
                    screen_crc: 0x1111,
                    state_bytes: vec![],
                },
                AutorunCheckpoint {
                    frame_index: 599,
                    screen_crc: 0x2222,
                    state_bytes: vec![],
                },
            ],
        }
    }

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
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 1,
                screen_crc: 0x8BB98613,
                state_bytes: vec![10, 20, 30],
            }],
        };

        save_autorun_file(temp.path(), &file).expect("save autorun file");
        let loaded = load_autorun_file(temp.path()).expect("load autorun file");

        assert_eq!(loaded, file);
    }

    #[test]
    fn test_backup_creates_bak_file() {
        let temp = NamedTempFile::new().expect("create temp file");
        let path = temp.path();
        std::fs::write(path, b"test data").expect("write test data");

        backup_autorun_file(path).expect("backup should succeed");

        let bak = path.with_extension("autorun.bak");
        assert!(bak.exists(), "backup file should exist");
        assert_eq!(
            std::fs::read(&bak).unwrap(),
            b"test data",
            "backup content should match"
        );
        let _ = std::fs::remove_file(bak);
    }

    #[test]
    fn test_backup_does_nothing_if_file_absent() {
        let path = Path::new("/tmp/nonexistent_autorun_test_file_xyz.autorun");
        assert!(backup_autorun_file(path).is_ok());
    }

    #[test]
    fn test_trim_recording_removes_last_checkpoint_and_its_frames() {
        let mut file = sample_file_with_checkpoints();
        assert_eq!(file.checkpoints.len(), 2);
        assert_eq!(file.frames.len(), 600);

        trim_recording(&mut file, 1);

        assert_eq!(file.checkpoints.len(), 1, "one checkpoint should remain");
        assert_eq!(
            file.checkpoints[0].frame_index, 299,
            "first checkpoint should remain"
        );
        // Frames up to and including frame 299 remain (300 frames total)
        assert_eq!(file.frames.len(), 300);
    }

    #[test]
    fn test_trim_recording_all_checkpoints_clears_frames() {
        let mut file = sample_file_with_checkpoints();

        trim_recording(&mut file, 2);

        assert!(file.checkpoints.is_empty());
        assert!(file.frames.is_empty());
    }

    #[test]
    fn test_trim_recording_n_zero_does_nothing() {
        let mut file = sample_file_with_checkpoints();
        trim_recording(&mut file, 0);
        assert_eq!(file.checkpoints.len(), 2);
        assert_eq!(file.frames.len(), 600);
    }

    #[test]
    fn test_trim_recording_n_exceeds_checkpoints_clears_all() {
        let mut file = sample_file_with_checkpoints();
        trim_recording(&mut file, 100);
        assert!(file.checkpoints.is_empty());
        assert!(file.frames.is_empty());
    }
}
