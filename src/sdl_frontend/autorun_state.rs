use crate::autorun::{
    AUTORUN_VERSION, AutorunFile, AutorunFrame, autorun_path_for_rom, crc32, load_autorun_file,
    save_autorun_file,
};
use crate::console::AutorunMode;
use std::path::PathBuf;

/// Manages autorun recording and playback state.
pub struct AutorunState {
    autorun: AutorunFile,
    autorun_path: PathBuf,
    frame_index: usize,
    extending_playback: bool,
    mode: AutorunMode,
}

impl AutorunState {
    /// Create a new AutorunState for the given mode and ROM path.
    pub fn new(
        mode: AutorunMode,
        rom_path: &str,
        overwrite: bool,
        extend: bool,
    ) -> Result<Self, String> {
        let autorun_path = autorun_path_for_rom(&PathBuf::from(rom_path));

        let (autorun, extending_playback) = match mode {
            AutorunMode::None => {
                return Err("AutorunState requires Record or Playback mode".to_string());
            }
            AutorunMode::Record => {
                if extend && autorun_path.exists() {
                    // Extend mode: load existing recording for playback, then record new frames
                    let existing = load_autorun_file(&autorun_path)?;
                    (existing, true)
                } else {
                    // New recording
                    if autorun_path.exists() {
                        if overwrite {
                            std::fs::remove_file(&autorun_path).map_err(|e| {
                                format!(
                                    "Failed to remove existing recording {}: {e}",
                                    autorun_path.display()
                                )
                            })?;
                        } else {
                            return Err(format!(
                                "Recording already exists: {} (use --overwrite-recording to replace)",
                                autorun_path.display()
                            ));
                        }
                    }
                    (
                        AutorunFile {
                            version: AUTORUN_VERSION,
                            frames: Vec::new(),
                            checksum: 0,
                        },
                        false,
                    )
                }
            }
            AutorunMode::Playback => {
                let autorun = load_autorun_file(&autorun_path)?;
                (autorun, false)
            }
        };

        Ok(Self {
            autorun,
            autorun_path,
            frame_index: 0,
            extending_playback,
            mode,
        })
    }

    /// Get the total number of frames in the recording.
    pub fn total_frames(&self) -> usize {
        self.autorun.frames.len()
    }

    /// Get the current frame index.
    #[allow(dead_code)]
    pub fn current_frame_index(&self) -> usize {
        self.frame_index
    }

    /// Check if we're in extend mode and still playing back.
    pub fn is_extending_playback(&self) -> bool {
        self.extending_playback && self.frame_index < self.autorun.frames.len()
    }

    /// Get the next frame to play back. Returns None if playback is complete.
    pub fn next_playback_frame(&mut self) -> Option<AutorunFrame> {
        if self.frame_index < self.autorun.frames.len() {
            let frame = self.autorun.frames[self.frame_index].clone();
            self.frame_index += 1;
            Some(frame)
        } else {
            None
        }
    }

    /// Record a new frame. Used in record mode or after playback in extend mode.
    pub fn record_frame(&mut self, player1: u8, player2: u8) {
        self.autorun.frames.push(AutorunFrame { player1, player2 });
        self.frame_index += 1;
    }

    /// Save the recording with the given CRC checksum.
    pub fn save_with_checksum(&mut self, checksum: u32) -> Result<(), String> {
        self.autorun.checksum = checksum;
        save_autorun_file(&self.autorun_path, &self.autorun)
    }

    /// Calculate CRC of the screen buffer.
    pub fn calculate_screen_crc(screen_data: &[u8]) -> u32 {
        crc32(screen_data)
    }

    /// Verify the CRC checksum matches the stored value.
    pub fn verify_checksum(&self, screen_data: &[u8]) -> Result<(), String> {
        let calculated = Self::calculate_screen_crc(screen_data);
        if calculated == self.autorun.checksum {
            Ok(())
        } else {
            Err(format!(
                "CRC mismatch: expected 0x{:08X}, got 0x{:08X}",
                self.autorun.checksum, calculated
            ))
        }
    }

    /// Check if playback is complete.
    #[allow(dead_code)]
    pub fn is_playback_complete(&self) -> bool {
        self.mode == AutorunMode::Playback && self.frame_index >= self.autorun.frames.len()
    }

    /// Get the autorun mode.
    pub fn mode(&self) -> AutorunMode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_total_frames_empty() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");

        let state = AutorunState::new(AutorunMode::Record, rom_path_str, false, false)
            .expect("create autorun state");

        assert_eq!(state.total_frames(), 0);
    }

    #[test]
    fn test_total_frames_with_data() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 1,
                    player2: 2,
                },
                AutorunFrame {
                    player1: 3,
                    player2: 4,
                },
                AutorunFrame {
                    player1: 5,
                    player2: 6,
                },
            ],
            checksum: 0,
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let state = AutorunState::new(AutorunMode::Playback, rom_path_str, false, false)
            .expect("create autorun state");

        assert_eq!(state.total_frames(), 3);
    }

    #[test]
    fn test_is_extending_playback_in_extend_mode() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 1,
                    player2: 2,
                },
                AutorunFrame {
                    player1: 3,
                    player2: 4,
                },
            ],
            checksum: 0,
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let mut state = AutorunState::new(AutorunMode::Record, rom_path_str, false, true)
            .expect("create autorun state");

        // Should be true while playing back existing frames
        assert!(state.is_extending_playback());

        // Consume first frame
        state.next_playback_frame();
        assert!(state.is_extending_playback());

        // Consume second frame
        state.next_playback_frame();
        assert!(!state.is_extending_playback()); // Past existing frames
    }

    #[test]
    fn test_is_extending_playback_in_normal_record_mode() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");

        let state = AutorunState::new(AutorunMode::Record, rom_path_str, false, false)
            .expect("create autorun state");

        assert!(!state.is_extending_playback());
    }

    #[test]
    fn test_is_extending_playback_in_playback_mode() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 1,
                player2: 2,
            }],
            checksum: 0,
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let state = AutorunState::new(AutorunMode::Playback, rom_path_str, false, false)
            .expect("create autorun state");

        assert!(!state.is_extending_playback());
    }
}
