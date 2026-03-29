use super::types::{AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFormat, AutorunFrame};
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

/// Body of a binary autorun file (postcard-encoded, after the magic header).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AutorunFileBinaryBody {
    frames: Vec<AutorunRleFrame>,
    checkpoints: Vec<AutorunCheckpointBinary>,
}

/// A checkpoint in the binary autorun format.
/// `state_bytes` contains a postcard-encoded `SaveState` (or is empty when no state was saved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AutorunCheckpointBinary {
    frame_index: u32,
    screen_crc: u32,
    state_bytes: Vec<u8>,
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

/// Magic header bytes that identify a binary autorun file.
pub const BINARY_MAGIC: &[u8; 6] = b"NESRA3";

pub fn save_autorun_file(
    path: &Path,
    file: &AutorunFile,
    format: AutorunFormat,
) -> Result<(), String> {
    if file.version != AUTORUN_VERSION {
        return Err(format!("Unsupported autorun version: {}", file.version));
    }

    match format {
        AutorunFormat::Json => save_autorun_file_json(path, file),
        AutorunFormat::Binary => save_autorun_file_binary(path, file),
    }
}

fn save_autorun_file_json(path: &Path, file: &AutorunFile) -> Result<(), String> {
    let encoded_file = AutorunFileV3Ser {
        version: AUTORUN_VERSION,
        frames: encode_rle_frames(&file.frames),
        checkpoints: &file.checkpoints,
    };

    let data = serde_json::to_vec_pretty(&encoded_file)
        .map_err(|e| format!("Failed to serialize autorun file: {e}"))?;
    write_autorun_bytes(path, data)
}

fn encode_checkpoint_to_binary(cp: &AutorunCheckpoint) -> Result<AutorunCheckpointBinary, String> {
    use crate::console::SaveState;

    let state_bytes = if cp.state_bytes.is_empty() {
        Vec::new()
    } else {
        let state = SaveState::from_bytes(&cp.state_bytes)
            .map_err(|e| format!("Failed to deserialize checkpoint state: {e}"))?;
        state
            .to_binary_bytes()
            .map_err(|e| format!("Failed to serialize checkpoint state as binary: {e}"))?
    };

    Ok(AutorunCheckpointBinary {
        frame_index: cp.frame_index,
        screen_crc: cp.screen_crc,
        state_bytes,
    })
}

fn save_autorun_file_binary(path: &Path, file: &AutorunFile) -> Result<(), String> {
    let checkpoints = file
        .checkpoints
        .iter()
        .map(encode_checkpoint_to_binary)
        .collect::<Result<Vec<_>, _>>()?;

    let body = AutorunFileBinaryBody {
        frames: encode_rle_frames(&file.frames),
        checkpoints,
    };

    let payload = postcard::to_allocvec(&body)
        .map_err(|e| format!("Failed to serialize binary autorun: {e}"))?;

    let mut data = Vec::with_capacity(BINARY_MAGIC.len() + payload.len());
    data.extend_from_slice(BINARY_MAGIC);
    data.extend_from_slice(&payload);

    write_autorun_bytes(path, data)
}

fn write_autorun_bytes(path: &Path, data: Vec<u8>) -> Result<(), String> {
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

    if data.starts_with(BINARY_MAGIC) {
        return load_autorun_file_binary(&data);
    }

    load_autorun_file_json(&data)
}

fn decode_checkpoint_from_binary(cp: AutorunCheckpointBinary) -> Result<AutorunCheckpoint, String> {
    use crate::console::SaveState;

    let state_bytes = if cp.state_bytes.is_empty() {
        Vec::new()
    } else {
        let state = SaveState::from_binary_bytes(&cp.state_bytes)
            .map_err(|e| format!("Failed to deserialize checkpoint state from binary: {e}"))?;
        state
            .to_bytes()
            .map_err(|e| format!("Failed to serialize checkpoint state as JSON: {e}"))?
    };

    Ok(AutorunCheckpoint {
        frame_index: cp.frame_index,
        screen_crc: cp.screen_crc,
        state_bytes,
    })
}

fn load_autorun_file_binary(data: &[u8]) -> Result<AutorunFile, String> {
    let payload = &data[BINARY_MAGIC.len()..];
    let body: AutorunFileBinaryBody = postcard::from_bytes(payload)
        .map_err(|e| format!("Failed to deserialize binary autorun file: {e}"))?;

    let frames = decode_rle_frames(&body.frames)?;
    let checkpoints = body
        .checkpoints
        .into_iter()
        .map(decode_checkpoint_from_binary)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AutorunFile {
        version: AUTORUN_VERSION,
        frames,
        checkpoints,
    })
}

fn load_autorun_file_json(data: &[u8]) -> Result<AutorunFile, String> {
    // Parse the JSON once into a generic Value so we can inspect the version field and then
    // re-use the same in-memory representation for the version-specific deserialization —
    // avoiding a second scan of the raw bytes.
    let json_value: serde_json::Value = serde_json::from_slice(data)
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

pub fn convert_autorun_file(path: &Path, target_format: AutorunFormat) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Autorun file not found: {}", path.display()));
    }

    let autorun_file = load_autorun_file(path)?;
    save_autorun_file(path, &autorun_file, target_format)
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

    /// Build a sample AutorunFile with many repeated frames (good for size comparison)
    /// and empty state_bytes (valid for binary roundtrip without a real NES).
    fn sample_large_file() -> AutorunFile {
        AutorunFile {
            version: AUTORUN_VERSION,
            frames: (0..3000)
                .map(|i| AutorunFrame {
                    player1: if i < 2990 { 0 } else { 1 },
                    player2: 0,
                })
                .collect(),
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 299,
                screen_crc: 0xABCDEF01,
                state_bytes: vec![],
            }],
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

        save_autorun_file(temp.path(), &file, AutorunFormat::Json).expect("save autorun file");
        let loaded = load_autorun_file(temp.path()).expect("load autorun file");

        assert_eq!(loaded, file);
    }

    #[test]
    fn test_save_binary_and_load_roundtrip() {
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
                AutorunFrame {
                    player1: 0b0000_0010,
                    player2: 0b0010_0000,
                },
            ],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 2,
                screen_crc: 0xDEADBEEF,
                state_bytes: vec![],
            }],
        };

        save_autorun_file(temp.path(), &file, AutorunFormat::Binary)
            .expect("save binary autorun file");
        let loaded = load_autorun_file(temp.path()).expect("load binary autorun file");

        assert_eq!(loaded, file);
    }

    #[test]
    fn test_binary_file_starts_with_magic_header() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        save_autorun_file(temp.path(), &file, AutorunFormat::Binary).expect("save binary");

        let raw = std::fs::read(temp.path()).expect("read binary file");
        assert!(
            raw.starts_with(BINARY_MAGIC),
            "binary file should start with NESRA3 magic, got {:?}",
            &raw[..BINARY_MAGIC.len().min(raw.len())]
        );
    }

    #[test]
    fn test_load_auto_detects_binary_format() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        save_autorun_file(temp.path(), &file, AutorunFormat::Binary).expect("save binary");
        let loaded = load_autorun_file(temp.path()).expect("auto-detect binary load");

        assert_eq!(loaded.version, AUTORUN_VERSION);
        assert_eq!(loaded.frames.len(), file.frames.len());
    }

    #[test]
    fn test_load_auto_detects_json_format() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        save_autorun_file(temp.path(), &file, AutorunFormat::Json).expect("save json");
        let loaded = load_autorun_file(temp.path()).expect("auto-detect json load");

        assert_eq!(loaded.version, AUTORUN_VERSION);
        assert_eq!(loaded.frames.len(), file.frames.len());
    }

    #[test]
    fn test_binary_file_is_smaller_than_json() {
        let temp_bin = NamedTempFile::new().expect("create temp file");
        let temp_json = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        save_autorun_file(temp_bin.path(), &file, AutorunFormat::Binary).expect("save binary");
        save_autorun_file(temp_json.path(), &file, AutorunFormat::Json).expect("save json");

        let binary_size = std::fs::metadata(temp_bin.path())
            .expect("get binary size")
            .len();
        let json_size = std::fs::metadata(temp_json.path())
            .expect("get json size")
            .len();

        let ratio = binary_size as f64 / json_size as f64;
        assert!(
            ratio <= 0.25,
            "binary ({binary_size} bytes) should be <=25% of JSON ({json_size} bytes), got {:.1}%",
            ratio * 100.0
        );
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

        save_autorun_file(temp.path(), &file, AutorunFormat::Json).expect("save autorun file");
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
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let missing_path = temp_dir.path().join("missing.autorun");
        let result = convert_autorun_file(&missing_path, AutorunFormat::Binary);
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

        convert_autorun_file(temp.path(), AutorunFormat::Json).expect("convert file");

        let parsed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(temp.path()).expect("read converted file"))
                .expect("parse converted json");
        assert_eq!(parsed["version"], json!(AUTORUN_VERSION));
        assert_eq!(parsed["frames"].as_array().map(Vec::len), Some(2));
        assert_eq!(parsed["frames"][0]["repeat"], json!(2));
    }

    #[test]
    fn test_convert_json_to_binary_format() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        // Start with a JSON file
        save_autorun_file(temp.path(), &file, AutorunFormat::Json).expect("save json");
        assert!(
            !std::fs::read(temp.path())
                .unwrap()
                .starts_with(BINARY_MAGIC),
            "should start as JSON"
        );

        // Convert to binary
        convert_autorun_file(temp.path(), AutorunFormat::Binary).expect("convert to binary");

        let raw = std::fs::read(temp.path()).expect("read converted file");
        assert!(
            raw.starts_with(BINARY_MAGIC),
            "converted file should start with binary magic"
        );
    }

    #[test]
    fn test_convert_binary_to_json_format() {
        let temp = NamedTempFile::new().expect("create temp file");
        let file = sample_large_file();

        // Start with a binary file
        save_autorun_file(temp.path(), &file, AutorunFormat::Binary).expect("save binary");
        assert!(
            std::fs::read(temp.path())
                .unwrap()
                .starts_with(BINARY_MAGIC),
            "should start as binary"
        );

        // Convert to JSON
        convert_autorun_file(temp.path(), AutorunFormat::Json).expect("convert to json");

        let raw = std::fs::read(temp.path()).expect("read converted file");
        assert!(
            !raw.starts_with(BINARY_MAGIC),
            "converted file should no longer have binary magic"
        );
        assert_eq!(raw[0], b'{', "converted file should start with JSON object");
    }
}
