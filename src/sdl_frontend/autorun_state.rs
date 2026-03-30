use crate::autorun::{
    AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFormat, AutorunFrame,
    CHECKPOINT_INTERVAL_FRAMES, autorun_path_for_rom, backup_autorun_file, load_autorun_file,
    save_autorun_file,
};
use crate::console::AutorunMode;
use std::path::PathBuf;

/// Resolves a checkpoint index (possibly negative) to a concrete `usize` index.
///
/// Negative indices count from the second-to-last checkpoint:
/// - `-1` → index `total - 2` (second-to-last)
/// - `-2` → index `total - 3` (third-to-last)
///
/// Non-negative indices are returned as-is.
fn resolve_checkpoint_index(raw: i64, total: usize) -> Result<usize, String> {
    if raw >= 0 {
        Ok(raw as usize)
    } else {
        // -1 → total-2, -2 → total-3, …
        let resolved = (total as i64 - 1) + raw;
        if resolved < 0 {
            Err(format!(
                "Checkpoint index {} out of range (recording has {} checkpoints)",
                raw, total
            ))
        } else {
            Ok(resolved as usize)
        }
    }
}

/// Information needed to restore NES state when starting from a checkpoint.
///
/// Returned by [`AutorunState::new`] when the caller must restore emulator state before
/// beginning playback or extend playback.
pub struct PendingRestore {
    /// Serialized [`crate::console::SaveState`] bytes.
    pub state_bytes: Vec<u8>,
}

/// Manages autorun recording and playback state.
pub struct AutorunState {
    autorun: AutorunFile,
    autorun_path: PathBuf,
    /// Index of the *next* frame to be emulated. In playback, this advances through the
    /// stored frames. In record, it counts newly recorded frames.
    frame_index: usize,
    /// True while in extend-record mode and still replaying the existing recording.
    extending_playback: bool,
    mode: AutorunMode,
    /// Index of the next checkpoint to verify during playback.
    playback_checkpoint_idx: usize,
    /// Accumulated CRC mismatch count during playback.
    crc_mismatches: usize,
    /// Total number of checkpoints verified so far.
    total_checkpoints_verified: usize,
    /// Set to `true` when `next_playback_frame` returned a pre-recorded frame this frame.
    /// Cleared by `begin_frame` at the start of each frame cycle.
    current_frame_prerecorded: bool,
    /// Serialization format to use when saving autorun files.
    format: AutorunFormat,
}

impl AutorunState {
    /// Create a new AutorunState for the given mode and ROM path.
    ///
    /// Returns `(state, Some(PendingRestore))` when the caller must restore NES state before
    /// beginning emulation (extend mode with checkpoints, or `from_checkpoint` playback).
    ///
    /// # Parameters
    /// * `mode` - Operating mode (`Record` or `Playback`).
    /// * `rom_path` - Path to the loaded ROM; the `.autorun` file lives alongside it.
    /// * `overwrite` - In `Record` mode: back up and overwrite any existing recording.
    /// * `extend` - In `Record` mode: play back the existing recording from the second-to-last
    ///   checkpoint, then continue recording.
    /// * `from_checkpoint` - In `Playback` mode: start from this checkpoint index instead of
    ///   frame 0.
    /// * `format` - Serialization format for saving autorun files.
    pub fn new(
        mode: AutorunMode,
        rom_path: &str,
        overwrite: bool,
        extend: bool,
        from_checkpoint: Option<i64>,
        format: AutorunFormat,
    ) -> Result<(Self, Option<PendingRestore>), String> {
        let autorun_path = autorun_path_for_rom(&PathBuf::from(rom_path));

        match mode {
            AutorunMode::None => Err("AutorunState requires Record or Playback mode".to_string()),

            AutorunMode::Record => {
                if extend && autorun_path.exists() {
                    Self::new_extend(autorun_path, format)
                } else {
                    // New recording — back up or clear any existing file
                    if autorun_path.exists() {
                        if overwrite {
                            backup_autorun_file(&autorun_path)?;
                        } else {
                            return Err(format!(
                                "Recording already exists: {} (use --create-recording to replace)",
                                autorun_path.display()
                            ));
                        }
                    }
                    let state = Self {
                        autorun: AutorunFile {
                            version: AUTORUN_VERSION,
                            frames: Vec::new(),
                            checkpoints: Vec::new(),
                        },
                        autorun_path,
                        frame_index: 0,
                        extending_playback: false,
                        mode,
                        playback_checkpoint_idx: 0,
                        crc_mismatches: 0,
                        total_checkpoints_verified: 0,
                        current_frame_prerecorded: false,
                        format,
                    };
                    Ok((state, None))
                }
            }

            AutorunMode::Playback => {
                let autorun = load_autorun_file(&autorun_path)?;
                let (frame_index, playback_checkpoint_idx, pending) = if let Some(cp_idx_raw) =
                    from_checkpoint
                {
                    let cp_idx = resolve_checkpoint_index(cp_idx_raw, autorun.checkpoints.len())?;
                    let cp = autorun.checkpoints.get(cp_idx).ok_or_else(|| {
                        format!(
                            "Checkpoint index {cp_idx} out of range (recording has {})",
                            autorun.checkpoints.len()
                        )
                    })?;
                    let pending = PendingRestore {
                        state_bytes: cp.state_bytes.clone(),
                    };
                    (cp.frame_index as usize + 1, cp_idx + 1, Some(pending))
                } else {
                    (0, 0, None)
                };

                let state = Self {
                    autorun,
                    autorun_path,
                    frame_index,
                    extending_playback: false,
                    mode,
                    playback_checkpoint_idx,
                    crc_mismatches: 0,
                    total_checkpoints_verified: 0,
                    current_frame_prerecorded: false,
                    format,
                };
                Ok((state, pending))
            }
        }
    }

    /// Construct extend-mode state: load an existing recording and prepare to continue it.
    ///
    /// * **n ≥ 2 checkpoints**: restores from the second-to-last checkpoint and replays frames
    ///   up to the last checkpoint, so the transition into recording is verifiable.
    /// * **n = 1 checkpoint**: restores directly from the single checkpoint and starts recording
    ///   immediately — no extend playback is needed.
    /// * **n = 0 checkpoints**: starts from scratch (no state to restore).
    fn new_extend(
        autorun_path: PathBuf,
        format: AutorunFormat,
    ) -> Result<(Self, Option<PendingRestore>), String> {
        let existing = load_autorun_file(&autorun_path)?;

        let n = existing.checkpoints.len();
        let (frame_index, playback_checkpoint_idx, pending) = if n >= 2 {
            let second_to_last = &existing.checkpoints[n - 2];
            let pending = PendingRestore {
                state_bytes: second_to_last.state_bytes.clone(),
            };
            // Start playing from the frame after the second-to-last checkpoint.
            // The last checkpoint (n-1) will be verified during this playback.
            (
                second_to_last.frame_index as usize + 1,
                n - 1,
                Some(pending),
            )
        } else if n == 1 {
            // Single checkpoint: restore from it and start recording immediately after.
            // No extend playback is needed — state restoration alone is sufficient.
            let cp = &existing.checkpoints[0];
            let pending = PendingRestore {
                state_bytes: cp.state_bytes.clone(),
            };
            (cp.frame_index as usize + 1, 1, Some(pending))
        } else {
            // No checkpoints: start from scratch.
            (0, 0, None)
        };

        let state = Self {
            autorun: existing,
            autorun_path,
            frame_index,
            extending_playback: true,
            mode: AutorunMode::Record,
            playback_checkpoint_idx,
            crc_mismatches: 0,
            total_checkpoints_verified: 0,
            current_frame_prerecorded: false,
            format,
        };
        Ok((state, pending))
    }

    /// Get the total number of frames in the recording.
    pub fn total_frames(&self) -> usize {
        self.autorun.frames.len()
    }

    /// Get the current frame index.
    pub fn current_frame_index(&self) -> usize {
        self.frame_index
    }

    /// Check if we're in extend mode and still playing back.
    pub fn is_extending_playback(&self) -> bool {
        self.extending_playback && self.frame_index < self.autorun.frames.len()
    }

    /// Signal the start of a new frame cycle.
    ///
    /// Must be called at the very beginning of each frame before any call to
    /// [`next_playback_frame`]. Clears the pre-recorded flag so that
    /// [`current_frame_was_prerecorded`] correctly reflects only the current frame.
    pub fn begin_frame(&mut self) {
        self.current_frame_prerecorded = false;
    }

    /// Returns `true` if the current frame's inputs came from a pre-recorded source
    /// (i.e., [`next_playback_frame`] returned `Some` this frame cycle).
    pub fn current_frame_was_prerecorded(&self) -> bool {
        self.current_frame_prerecorded
    }

    /// Get the next frame to play back. Returns None if playback is complete.
    pub fn next_playback_frame(&mut self) -> Option<AutorunFrame> {
        if self.frame_index < self.autorun.frames.len() {
            let frame = self.autorun.frames[self.frame_index].clone();
            self.frame_index += 1;
            self.current_frame_prerecorded = true;
            Some(frame)
        } else {
            None
        }
    }

    /// Record a new frame during record mode (or extend mode after playback).
    ///
    /// Returns `true` if a checkpoint should be captured immediately after this call
    /// (i.e., the frame completes a checkpoint interval).
    pub fn record_frame(&mut self, player1: u8, player2: u8) -> bool {
        self.autorun.frames.push(AutorunFrame { player1, player2 });
        self.frame_index += 1;
        // Checkpoint every CHECKPOINT_INTERVAL_FRAMES frames
        (self.frame_index as u32).is_multiple_of(CHECKPOINT_INTERVAL_FRAMES)
    }

    /// Store a checkpoint for the frame just recorded.
    ///
    /// Call this immediately after [`record_frame`] returns `true`.
    pub fn record_checkpoint(&mut self, screen_crc: u32, state_bytes: Vec<u8>) {
        self.autorun.checkpoints.push(AutorunCheckpoint {
            frame_index: (self.frame_index - 1) as u32,
            screen_crc,
            state_bytes,
        });
    }

    /// Check whether the current frame has a playback checkpoint and verify its CRC.
    ///
    /// Must be called after each frame is rendered during playback.
    /// Returns `Some(true)` if the CRC matched, `Some(false)` if it mismatched,
    /// or `None` if no checkpoint falls on this frame.
    pub fn check_playback_checkpoint(&mut self, screen_crc: u32) -> Option<bool> {
        let current = (self.frame_index - 1) as u32; // frame just completed
        let mut matched = None;
        while self.playback_checkpoint_idx < self.autorun.checkpoints.len()
            && self.autorun.checkpoints[self.playback_checkpoint_idx].frame_index == current
        {
            let expected = self.autorun.checkpoints[self.playback_checkpoint_idx].screen_crc;
            let ok = screen_crc == expected;
            if !ok {
                self.crc_mismatches += 1;
            }
            self.total_checkpoints_verified += 1;
            self.playback_checkpoint_idx += 1;
            matched = Some(ok);
        }
        matched
    }

    /// Save the recording. Appends a final checkpoint with the given CRC and state.
    pub fn save_with_final_checkpoint(
        &mut self,
        screen_crc: u32,
        state_bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.autorun.checkpoints.push(AutorunCheckpoint {
            frame_index: self.frame_index.saturating_sub(1) as u32,
            screen_crc,
            state_bytes,
        });
        save_autorun_file(&self.autorun_path, &self.autorun, self.format)
    }

    /// Number of CRC mismatches detected during playback so far.
    pub fn crc_mismatches(&self) -> usize {
        self.crc_mismatches
    }

    /// Total number of checkpoints verified so far during playback.
    pub fn total_checkpoints_verified(&self) -> usize {
        self.total_checkpoints_verified
    }

    /// Total number of checkpoints in the loaded autorun file.
    pub fn total_checkpoints(&self) -> usize {
        self.autorun.checkpoints.len()
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

    fn make_file_with_checkpoints(num_frames: u32, checkpoint_crcs: &[(u32, u32)]) -> AutorunFile {
        AutorunFile {
            version: AUTORUN_VERSION,
            frames: (0..num_frames)
                .map(|_| AutorunFrame {
                    player1: 0,
                    player2: 0,
                })
                .collect(),
            checkpoints: checkpoint_crcs
                .iter()
                .map(|(frame_idx, crc)| AutorunCheckpoint {
                    frame_index: *frame_idx,
                    screen_crc: *crc,
                    state_bytes: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn test_total_frames_empty() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");

        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        assert_eq!(state.total_frames(), 0);
    }

    #[test]
    fn test_total_frames_with_data() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = make_file_with_checkpoints(3, &[(2, 0xABCD)]);
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        assert_eq!(state.total_frames(), 3);
    }

    #[test]
    fn test_record_frame_returns_true_at_checkpoint_interval() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        // Record 299 frames – none should trigger a checkpoint
        for _ in 0..299 {
            let checkpoint_due = state.record_frame(0, 0);
            assert!(!checkpoint_due);
        }
        // Frame 300 (index 299) triggers a checkpoint
        let checkpoint_due = state.record_frame(0, 0);
        assert!(checkpoint_due, "frame 300 should trigger a checkpoint");
    }

    #[test]
    fn test_record_checkpoint_stores_crc_and_state() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        for _ in 0..300 {
            state.record_frame(0, 0);
        }
        state.record_checkpoint(0xCAFEBABE, vec![1, 2, 3]);

        assert_eq!(state.autorun.checkpoints.len(), 1);
        assert_eq!(state.autorun.checkpoints[0].frame_index, 299);
        assert_eq!(state.autorun.checkpoints[0].screen_crc, 0xCAFEBABE);
        assert_eq!(state.autorun.checkpoints[0].state_bytes, vec![1, 2, 3]);
    }

    #[test]
    fn test_check_playback_checkpoint_matching_crc() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let file = make_file_with_checkpoints(300, &[(299, 0x1234)]);
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        // Consume all 300 frames
        for _ in 0..300 {
            state.next_playback_frame();
        }

        let result = state.check_playback_checkpoint(0x1234);
        assert_eq!(result, Some(true));
        assert_eq!(state.crc_mismatches(), 0);
        assert_eq!(state.total_checkpoints_verified(), 1);
    }

    #[test]
    fn test_check_playback_checkpoint_mismatching_crc() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let file = make_file_with_checkpoints(300, &[(299, 0x1234)]);
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        for _ in 0..300 {
            state.next_playback_frame();
        }

        let result = state.check_playback_checkpoint(0xBADBAD00);
        assert_eq!(result, Some(false));
        assert_eq!(state.crc_mismatches(), 1);
    }

    #[test]
    fn test_overwrite_record_creates_backup() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let existing_file = make_file_with_checkpoints(1, &[(0, 0xABCD)]);
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &existing_file, AutorunFormat::Json)
            .expect("save existing");
        assert!(autorun_path.exists());

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let _ = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            true,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state with overwrite");

        let bak_path = autorun_path.with_extension("autorun.bak");
        assert!(
            bak_path.exists(),
            "backup file should be created on overwrite"
        );
        let _ = std::fs::remove_file(&bak_path);
    }

    #[test]
    fn test_is_extending_playback_starts_true_in_extend_mode() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        // Need >=2 checkpoints to get full extend behavior; use >=2
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
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 0,
                    screen_crc: 0,
                    state_bytes: vec![],
                },
                AutorunCheckpoint {
                    frame_index: 1,
                    screen_crc: 0,
                    state_bytes: vec![],
                },
            ],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        assert!(state.is_extending_playback());
    }

    #[test]
    fn test_extend_mode_starts_from_second_to_last_checkpoint() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        // 4 checkpoints at frames 299, 599, 899, 1199
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0
                };
                1200
            ],
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 299,
                    screen_crc: 0,
                    state_bytes: b"state0".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 599,
                    screen_crc: 0,
                    state_bytes: b"state1".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 899,
                    screen_crc: 0,
                    state_bytes: b"state2".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 1199,
                    screen_crc: 0,
                    state_bytes: b"state3".to_vec(),
                },
            ],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (state, pending) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        // Should start from second-to-last checkpoint (frame 899) + 1 = 900
        assert_eq!(state.current_frame_index(), 900);
        assert!(
            pending.is_some(),
            "should have pending restore for second-to-last checkpoint"
        );
        assert_eq!(
            pending.unwrap().state_bytes,
            b"state2".to_vec(),
            "should restore state from second-to-last checkpoint"
        );
    }

    #[test]
    fn test_playback_from_checkpoint_sets_frame_index() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0
                };
                600
            ],
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 299,
                    screen_crc: 0x1111,
                    state_bytes: b"s0".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 599,
                    screen_crc: 0x2222,
                    state_bytes: b"s1".to_vec(),
                },
            ],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (state, pending) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            Some(0),
            AutorunFormat::Json,
        )
        .expect("create state");

        // frame_index should be 299 + 1 = 300
        assert_eq!(state.current_frame_index(), 300);
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().state_bytes, b"s0".to_vec());
    }

    #[test]
    fn test_is_extending_playback_in_normal_record_mode() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path to string");

        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
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
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0,
                state_bytes: vec![],
            }],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save file");

        let rom_path_str = rom_file.path().to_str().expect("rom path to string");
        let (state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create autorun state");

        assert!(!state.is_extending_playback());
    }

    fn make_file_with_4_checkpoints() -> (tempfile::NamedTempFile, AutorunFile) {
        // 4 checkpoints at frame indices 0, 1, 2, 3
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0
                };
                4
            ],
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 0,
                    screen_crc: 0,
                    state_bytes: b"s0".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 1,
                    screen_crc: 0,
                    state_bytes: b"s1".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 2,
                    screen_crc: 0,
                    state_bytes: b"s2".to_vec(),
                },
                AutorunCheckpoint {
                    frame_index: 3,
                    screen_crc: 0,
                    state_bytes: b"s3".to_vec(),
                },
            ],
        };
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");
        (rom_file, autorun_file)
    }

    #[test]
    fn test_playback_from_negative_checkpoint_minus_1_is_second_to_last() {
        // -1 = second-to-last checkpoint (index 2 of 4)
        let (rom_file, _) = make_file_with_4_checkpoints();
        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (state, pending) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            Some(-1),
            AutorunFormat::Json,
        )
        .expect("create state");

        // Second-to-last is index 2, frame_index=2, so playback starts at frame 3
        assert_eq!(state.current_frame_index(), 3);
        assert_eq!(pending.unwrap().state_bytes, b"s2".to_vec());
    }

    #[test]
    fn test_playback_from_negative_checkpoint_minus_2_is_third_from_end() {
        // -2 = third from end (index 1 of 4)
        let (rom_file, _) = make_file_with_4_checkpoints();
        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (state, pending) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            Some(-2),
            AutorunFormat::Json,
        )
        .expect("create state");

        assert_eq!(state.current_frame_index(), 2);
        assert_eq!(pending.unwrap().state_bytes, b"s1".to_vec());
    }

    #[test]
    fn test_playback_from_negative_checkpoint_out_of_range_is_error() {
        // -4 with 4 checkpoints would resolve to index -1 which is out of range
        let (rom_file, _) = make_file_with_4_checkpoints();
        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let result = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            Some(-4),
            AutorunFormat::Json,
        );
        assert!(result.is_err(), "negative index beyond range should fail");
    }

    fn make_recording_with_single_checkpoint() -> (tempfile::NamedTempFile, AutorunFile) {
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0
                };
                300
            ],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 299,
                screen_crc: 0xABCD,
                state_bytes: b"single_checkpoint_state".to_vec(),
            }],
        };
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");
        (rom_file, autorun_file)
    }

    fn make_recording_with_no_checkpoints() -> tempfile::NamedTempFile {
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![],
        };
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");
        rom_file
    }

    // --- Bug 2: n=1 extend restores from the single checkpoint ---

    #[test]
    fn test_extend_with_single_checkpoint_restores_from_it() {
        let (rom_file, _) = make_recording_with_single_checkpoint();
        let rom_path_str = rom_file.path().to_str().expect("rom path");

        let (_, pending) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create extend state");

        assert!(
            pending.is_some(),
            "extending with 1 checkpoint must restore from that checkpoint"
        );
        assert_eq!(
            pending.unwrap().state_bytes,
            b"single_checkpoint_state".to_vec()
        );
    }

    #[test]
    fn test_extend_with_single_checkpoint_starts_at_frame_after_checkpoint() {
        let (rom_file, _) = make_recording_with_single_checkpoint();
        let rom_path_str = rom_file.path().to_str().expect("rom path");

        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create extend state");

        // CP[0].frame_index = 299, so recording starts at 300
        assert_eq!(
            state.current_frame_index(),
            300,
            "extend with 1 checkpoint should start at CP[0].frame_index + 1"
        );
    }

    #[test]
    fn test_extend_with_single_checkpoint_has_no_extend_playback() {
        let (rom_file, _) = make_recording_with_single_checkpoint();
        let rom_path_str = rom_file.path().to_str().expect("rom path");

        let (state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create extend state");

        assert!(
            !state.is_extending_playback(),
            "extend with 1 checkpoint should not replay frames — restoration alone is sufficient"
        );
    }

    #[test]
    fn test_extend_with_no_checkpoints_starts_from_scratch() {
        let rom_file = make_recording_with_no_checkpoints();
        let rom_path_str = rom_file.path().to_str().expect("rom path");

        let (state, pending) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            true,
            None,
            AutorunFormat::Json,
        )
        .expect("create extend state");

        assert!(
            pending.is_none(),
            "no checkpoints means no state to restore"
        );
        assert_eq!(state.current_frame_index(), 0, "should start from frame 0");
    }

    // --- Bug 1: pre-recorded frame flag prevents duplicate at extend transition ---

    #[test]
    fn test_begin_frame_clears_prerecorded_flag() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Record,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        state.begin_frame();
        assert!(!state.current_frame_was_prerecorded());
    }

    #[test]
    fn test_prerecorded_flag_true_after_next_playback_frame_returns_some() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 7,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0,
                state_bytes: vec![],
            }],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        state.begin_frame();
        let frame = state.next_playback_frame();
        assert!(frame.is_some(), "should return the pre-recorded frame");
        assert!(
            state.current_frame_was_prerecorded(),
            "flag must be true after next_playback_frame returned Some"
        );
    }

    #[test]
    fn test_prerecorded_flag_false_after_next_playback_frame_returns_none() {
        let rom_file = NamedTempFile::new().expect("create temp file");
        // Empty recording — next_playback_frame always returns None
        let autorun_file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![],
        };
        let autorun_path = autorun_path_for_rom(rom_file.path());
        save_autorun_file(&autorun_path, &autorun_file, AutorunFormat::Json).expect("save");

        let rom_path_str = rom_file.path().to_str().expect("rom path");
        let (mut state, _) = AutorunState::new(
            AutorunMode::Playback,
            rom_path_str,
            false,
            false,
            None,
            AutorunFormat::Json,
        )
        .expect("create state");

        state.begin_frame();
        let frame = state.next_playback_frame();
        assert!(frame.is_none());
        assert!(
            !state.current_frame_was_prerecorded(),
            "flag must remain false when next_playback_frame returns None"
        );
    }
}
