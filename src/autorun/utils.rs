use super::types::{AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFrame};
use crate::cartridge::calculate_rom_crc32;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AutorunRleFrame {
    player1: u8,
    player2: u8,
    repeat: u32,
}

/// Serialization-only view of a v3 autorun file.  Borrows `checkpoints` to avoid cloning
/// potentially large `state_bytes` blobs that live inside each checkpoint.
#[derive(Debug, Serialize)]
struct AutorunFileV3Ser<'a> {
    version: u32,
    frames: Vec<AutorunRleFrame>,
    checkpoints: &'a [AutorunCheckpoint],
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AutorunFileV3OnDisk {
    version: u32,
    frames: Vec<AutorunRleFrame>,
    checkpoints: Vec<AutorunCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AutorunFileV2 {
    version: u32,
    frames: Vec<AutorunFrame>,
    checkpoints: Vec<AutorunCheckpoint>,
}

fn build_rle_frame(frame: &AutorunFrame, repeat_count: u32) -> AutorunRleFrame {
    AutorunRleFrame {
        player1: frame.player1,
        player2: frame.player2,
        repeat: repeat_count,
    }
}

fn build_input_frame(rle_frame: &AutorunRleFrame) -> AutorunFrame {
    AutorunFrame {
        player1: rle_frame.player1,
        player2: rle_frame.player2,
    }
}

fn encode_rle_frames(frames: &[AutorunFrame]) -> Vec<AutorunRleFrame> {
    if frames.is_empty() {
        return Vec::new();
    }

    let mut encoded_frames = Vec::new();
    let mut current_frame = &frames[0];
    let mut repeat_count: u32 = 1;

    for frame in &frames[1..] {
        if frame == current_frame && repeat_count < u32::MAX {
            repeat_count += 1;
            continue;
        }

        encoded_frames.push(build_rle_frame(current_frame, repeat_count));
        current_frame = frame;
        repeat_count = 1;
    }

    encoded_frames.push(build_rle_frame(current_frame, repeat_count));

    encoded_frames
}

/// Maximum total frames that `decode_rle_frames` will expand.
///
/// At 60 fps, 10 000 000 frames ≈ 46 hours — far more than any real NES recording.
/// This prevents a crafted file with enormous `repeat` values from causing OOM.
const MAX_DECODED_FRAMES: usize = 10_000_000;

fn decode_rle_frames(rle_frames: &[AutorunRleFrame]) -> Result<Vec<AutorunFrame>, String> {
    // First pass: validate every entry and accumulate the total so we can pre-allocate
    // exactly the right capacity without risking OOM from a malicious `repeat` value.
    let mut total_frames: usize = 0;
    for rle_frame in rle_frames {
        if rle_frame.repeat == 0 {
            return Err("Invalid autorun RLE frame with repeat=0".to_string());
        }
        total_frames = total_frames
            .checked_add(rle_frame.repeat as usize)
            .filter(|&t| t <= MAX_DECODED_FRAMES)
            .ok_or_else(|| {
                format!("Autorun file exceeds maximum of {MAX_DECODED_FRAMES} decoded frames")
            })?;
    }

    let mut decoded_frames = Vec::with_capacity(total_frames);
    for rle_frame in rle_frames {
        decoded_frames.extend(std::iter::repeat_n(
            build_input_frame(rle_frame),
            rle_frame.repeat as usize,
        ));
    }

    Ok(decoded_frames)
}

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

    let encoded_file = AutorunFileV3Ser {
        version: AUTORUN_VERSION,
        frames: encode_rle_frames(&file.frames),
        checkpoints: &file.checkpoints,
    };

    let data = serde_json::to_vec_pretty(&encoded_file)
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

    // Parse the JSON once into a generic Value so we can inspect the version field and then
    // re-use the same in-memory representation for the version-specific deserialization —
    // avoiding a second scan of the raw bytes.
    let json_value: serde_json::Value = serde_json::from_slice(&data)
        .map_err(|e| format!("Failed to deserialize autorun file: {e}"))?;

    let version = json_value["version"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| "Missing or invalid version field in autorun file".to_string())?;

    match version {
        2 => {
            let v2: AutorunFileV2 = serde_json::from_value(json_value)
                .map_err(|e| format!("Failed to deserialize autorun v2 file: {e}"))?;
            Ok(AutorunFile {
                version: AUTORUN_VERSION,
                frames: v2.frames,
                checkpoints: v2.checkpoints,
            })
        }
        3 => {
            let v3: AutorunFileV3OnDisk = serde_json::from_value(json_value)
                .map_err(|e| format!("Failed to deserialize autorun v3 file: {e}"))?;
            Ok(AutorunFile {
                version: AUTORUN_VERSION,
                frames: decode_rle_frames(&v3.frames)?,
                checkpoints: v3.checkpoints,
            })
        }
        _ => Err(format!("Unsupported autorun version: {version}")),
    }
}

pub fn convert_autorun_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Autorun file not found: {}", path.display()));
    }

    let autorun_file = load_autorun_file(path)?;
    save_autorun_file(path, &autorun_file)
}

#[cfg(test)]
mod tests {
    use super::super::types::{AutorunCheckpoint, AutorunFrame};
    use super::*;
    use serde_json::json;
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

    #[test]
    fn test_save_writes_version_3_with_rle_frames() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0,
                },
                AutorunFrame {
                    player1: 0,
                    player2: 0,
                },
                AutorunFrame {
                    player1: 0,
                    player2: 0,
                },
                AutorunFrame {
                    player1: 1,
                    player2: 0,
                },
            ],
            checkpoints: vec![],
        };

        save_autorun_file(temp.path(), &file).expect("save autorun file");
        let raw = std::fs::read_to_string(temp.path()).expect("read saved file as text");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse saved json");

        assert_eq!(parsed["version"], json!(3));
        assert_eq!(parsed["frames"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            parsed["frames"][0],
            json!({"player1": 0, "player2": 0, "repeat": 3})
        );
        assert_eq!(
            parsed["frames"][1],
            json!({"player1": 1, "player2": 0, "repeat": 1})
        );
    }

    #[test]
    fn test_load_accepts_version_2_and_expands_to_per_frame_sequence() {
        let temp = NamedTempFile::new().expect("create temp file");
        let legacy_v2 = json!({
            "version": 2,
            "frames": [
                {"player1": 4, "player2": 0},
                {"player1": 4, "player2": 0},
                {"player1": 7, "player2": 1}
            ],
            "checkpoints": []
        });
        std::fs::write(
            temp.path(),
            serde_json::to_vec_pretty(&legacy_v2).expect("serialize legacy json"),
        )
        .expect("write legacy v2 file");

        let loaded = load_autorun_file(temp.path()).expect("load v2 file");

        assert_eq!(loaded.version, 3);
        assert_eq!(loaded.frames.len(), 3);
        assert_eq!(
            loaded.frames,
            vec![
                AutorunFrame {
                    player1: 4,
                    player2: 0
                },
                AutorunFrame {
                    player1: 4,
                    player2: 0
                },
                AutorunFrame {
                    player1: 7,
                    player2: 1
                },
            ]
        );
    }

    #[test]
    fn test_load_rejects_v3_file_exceeding_max_decoded_frames() {
        let temp = NamedTempFile::new().expect("create temp file");
        // A single RLE entry with repeat > MAX_DECODED_FRAMES should be rejected.
        let oversized = json!({
            "version": 3,
            "frames": [
                {"player1": 0, "player2": 0, "repeat": MAX_DECODED_FRAMES + 1}
            ],
            "checkpoints": []
        });
        std::fs::write(
            temp.path(),
            serde_json::to_vec_pretty(&oversized).expect("serialize oversized json"),
        )
        .expect("write oversized v3 file");

        let result = load_autorun_file(temp.path());
        assert!(
            result.is_err(),
            "loading a file exceeding MAX_DECODED_FRAMES should fail"
        );
        assert!(
            result.unwrap_err().contains("exceeds maximum"),
            "error message should mention exceeds maximum"
        );
    }

    #[test]
    fn test_load_rejects_v3_rle_frame_with_zero_repeat() {
        let temp = NamedTempFile::new().expect("create temp file");
        let zero_repeat = json!({
            "version": 3,
            "frames": [{"player1": 1, "player2": 0, "repeat": 0}],
            "checkpoints": []
        });
        std::fs::write(
            temp.path(),
            serde_json::to_vec_pretty(&zero_repeat).expect("serialize"),
        )
        .expect("write");

        let result = load_autorun_file(temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("repeat=0"));
    }

    #[test]
    fn test_convert_autorun_file_fails_when_source_file_missing() {
        let path = Path::new("/tmp/neser_missing_convert_input.autorun");
        let result = convert_autorun_file(path);
        assert!(
            result.is_err(),
            "conversion should fail when file is missing"
        );
    }

    #[test]
    fn test_convert_autorun_file_rewrites_v2_to_v3_rle() {
        let temp = NamedTempFile::new().expect("create temp file");
        let legacy_v2 = json!({
            "version": 2,
            "frames": [
                {"player1": 0, "player2": 0},
                {"player1": 0, "player2": 0},
                {"player1": 1, "player2": 0}
            ],
            "checkpoints": []
        });
        std::fs::write(
            temp.path(),
            serde_json::to_vec_pretty(&legacy_v2).expect("serialize v2 json"),
        )
        .expect("write v2 autorun");

        convert_autorun_file(temp.path()).expect("convert file");

        let parsed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(temp.path()).expect("read converted file"))
                .expect("parse converted json");
        assert_eq!(parsed["version"], json!(AUTORUN_VERSION));
        assert_eq!(parsed["frames"].as_array().map(Vec::len), Some(2));
        assert_eq!(parsed["frames"][0]["repeat"], json!(2));
    }
}
