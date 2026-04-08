use serde::{Deserialize, Serialize};

pub const AUTORUN_VERSION: u32 = 3;

/// Number of frames between checkpoints (~5 seconds at NTSC 60fps).
pub const CHECKPOINT_INTERVAL_FRAMES: u32 = 300;

/// Controller input for a single frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFrame {
    pub player1: u8,
    pub player2: u8,
}

/// A snapshot of full emulator state and screen CRC at a specific frame.
///
/// Checkpoints are saved every [`CHECKPOINT_INTERVAL_FRAMES`] frames and at the end of a
/// recording. During playback, the screen CRC at each checkpoint is verified against the
/// stored value to detect emulation divergence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunCheckpoint {
    /// Index of the frame at which this checkpoint was taken (0-based).
    pub frame_index: u32,
    /// CRC-32 of the screen buffer at this frame.
    pub screen_crc: u32,
    /// Full serialized emulator state ([`crate::nes::console::SaveState`] as JSON bytes).
    pub state_bytes: Vec<u8>,
}

/// A complete autorun recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFile {
    pub version: u32,
    /// Input for every recorded frame, in order.
    pub frames: Vec<AutorunFrame>,
    /// Checkpoints taken every [`CHECKPOINT_INTERVAL_FRAMES`] frames and at the end.
    pub checkpoints: Vec<AutorunCheckpoint>,
}

/// The serialization format for autorun files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutorunFormat {
    #[default]
    Binary,
    Json,
}

impl std::fmt::Display for AutorunFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binary => write!(f, "binary"),
            Self::Json => write!(f, "json"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autorun_format_default_is_binary() {
        assert_eq!(AutorunFormat::default(), AutorunFormat::Binary);
    }

    #[test]
    fn test_autorun_format_variants_are_distinct() {
        assert_ne!(AutorunFormat::Binary, AutorunFormat::Json);
    }

    #[test]
    fn test_autorun_version_is_3() {
        assert_eq!(AUTORUN_VERSION, 3);
    }

    #[test]
    fn test_checkpoint_interval_is_300() {
        assert_eq!(CHECKPOINT_INTERVAL_FRAMES, 300);
    }

    #[test]
    fn test_autorun_file_has_checkpoints_field() {
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![],
        };
        assert!(file.checkpoints.is_empty());
    }

    #[test]
    fn test_autorun_checkpoint_fields() {
        let cp = AutorunCheckpoint {
            frame_index: 300,
            screen_crc: 0xDEADBEEF,
            state_bytes: vec![1, 2, 3],
        };
        assert_eq!(cp.frame_index, 300);
        assert_eq!(cp.screen_crc, 0xDEADBEEF);
        assert_eq!(cp.state_bytes, vec![1, 2, 3]);
    }

    #[test]
    fn test_autorun_file_uses_checkpoints_instead_of_checksum() {
        // AutorunFile must NOT have a `checksum` field (it was replaced by checkpoints)
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0xABCD,
                state_bytes: vec![],
            }],
        };
        // The last checkpoint holds the final screen CRC
        assert_eq!(file.checkpoints.last().unwrap().screen_crc, 0xABCD);
    }
}
