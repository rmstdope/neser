use crate::platform::autorun::AutorunMode;
use crate::platform::autorun::{
    AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFileV2, AutorunFileV3OnDisk,
    AutorunFileV3Ser, AutorunFrame, CHECKPOINT_INTERVAL_FRAMES, decode_rle_frames,
    encode_rle_frames,
};

/// In-memory autorun state for the WASM frontend.
///
/// Unlike the native frontend's `AutorunState` this variant never
/// touches the file system – recordings are created in memory and returned as raw
/// bytes that the JavaScript layer can trigger the browser to download.
pub struct WasmAutorunState {
    autorun: AutorunFile,
    frame_index: usize,
    extending_playback: bool,
    mode: AutorunMode,
    playback_checkpoint_idx: usize,
    crc_mismatches: usize,
    total_checkpoints_verified: usize,
    current_frame_prerecorded: bool,
}

impl WasmAutorunState {
    /// Create a fresh recording state with no frames.
    pub fn new_recording() -> Self {
        WasmAutorunState {
            autorun: AutorunFile {
                version: AUTORUN_VERSION,
                frames: Vec::new(),
                checkpoints: Vec::new(),
            },
            frame_index: 0,
            extending_playback: false,
            mode: AutorunMode::Record,
            playback_checkpoint_idx: 0,
            crc_mismatches: 0,
            total_checkpoints_verified: 0,
            current_frame_prerecorded: false,
        }
    }

    /// Create a playback state from serialized [`AutorunFile`] bytes.
    ///
    /// Returns `(state, pending_restore)` where `pending_restore` is `Some(state_bytes)` when
    /// the caller must restore the NES state before starting emulation (checkpoint seek or
    /// extend mode).
    ///
    /// # Parameters
    /// * `bytes` – JSON-serialized [`AutorunFile`].
    /// * `checkpoint_idx` – If `Some(idx)`, start playback from that checkpoint (0-based).
    /// * `extend` – If `true`, play back and then continue recording (extend mode).
    pub fn new_playback(
        bytes: &[u8],
        checkpoint_idx: Option<u32>,
        extend: bool,
    ) -> Result<(Self, Option<Vec<u8>>), String> {
        let autorun = parse_autorun_bytes(bytes)?;

        if extend {
            return Ok(Self::new_extend(autorun));
        }

        // Pure playback
        let (frame_index, playback_checkpoint_idx, pending) = if let Some(cp_idx) = checkpoint_idx {
            let cp_idx = cp_idx as usize;
            let cp = autorun.checkpoints.get(cp_idx).ok_or_else(|| {
                format!(
                    "Checkpoint index {cp_idx} out of range (recording has {} checkpoints)",
                    autorun.checkpoints.len()
                )
            })?;
            let pending = cp.state_bytes.clone();
            (cp.frame_index as usize + 1, cp_idx + 1, Some(pending))
        } else {
            (0, 0, None)
        };

        Ok((
            WasmAutorunState {
                autorun,
                frame_index,
                extending_playback: false,
                mode: AutorunMode::Playback,
                playback_checkpoint_idx,
                crc_mismatches: 0,
                total_checkpoints_verified: 0,
                current_frame_prerecorded: false,
            },
            pending,
        ))
    }

    /// Build extend state: restore from a checkpoint and optionally replay.
    ///
    /// - n ≥ 2 checkpoints: restore from second-to-last, replay toward last.
    /// - n = 1 checkpoint:  restore from the single checkpoint, record immediately.
    /// - n = 0 checkpoints: start from frame 0 with nothing to restore.
    fn new_extend(autorun: AutorunFile) -> (Self, Option<Vec<u8>>) {
        let n = autorun.checkpoints.len();
        let (frame_index, playback_checkpoint_idx, pending, extending_playback) = if n >= 2 {
            let second_to_last = &autorun.checkpoints[n - 2];
            let pending = second_to_last.state_bytes.clone();
            (
                second_to_last.frame_index as usize + 1,
                n - 1,
                Some(pending),
                true,
            )
        } else if n == 1 {
            let cp = &autorun.checkpoints[0];
            let pending = cp.state_bytes.clone();
            (cp.frame_index as usize + 1, 1, Some(pending), false)
        } else {
            (0, 0, None, false)
        };

        (
            WasmAutorunState {
                autorun,
                frame_index,
                extending_playback,
                mode: AutorunMode::Record,
                playback_checkpoint_idx,
                crc_mismatches: 0,
                total_checkpoints_verified: 0,
                current_frame_prerecorded: false,
            },
            pending,
        )
    }

    /// Signal the start of a new frame cycle.  Must be called before [`next_playback_frame`].
    pub fn begin_frame(&mut self) {
        self.current_frame_prerecorded = false;
    }

    /// Returns true if the current frame's inputs came from a pre-recorded source.
    pub fn current_frame_was_prerecorded(&self) -> bool {
        self.current_frame_prerecorded
    }

    /// Get the next pre-recorded frame.  Returns `None` when playback is exhausted.
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

    /// Record one frame.  Returns `true` when a checkpoint should be captured.
    pub fn record_frame(&mut self, player1: u8, player2: u8) -> bool {
        self.autorun.frames.push(AutorunFrame { player1, player2 });
        self.frame_index += 1;
        (self.frame_index as u32).is_multiple_of(CHECKPOINT_INTERVAL_FRAMES)
    }

    /// Persist a checkpoint.  Call immediately after `record_frame` returns `true`.
    pub fn record_checkpoint(&mut self, screen_crc: u32, state_bytes: Vec<u8>) {
        self.autorun.checkpoints.push(AutorunCheckpoint {
            frame_index: (self.frame_index - 1) as u32,
            screen_crc,
            state_bytes,
        });
    }

    /// Append a final checkpoint and return the recording as serialized JSON bytes.
    ///
    /// The output uses v3 RLE-encoded frames, matching the native save format.
    pub fn finalize_recording(&mut self, screen_crc: u32, state_bytes: Vec<u8>) -> Vec<u8> {
        self.autorun.checkpoints.push(AutorunCheckpoint {
            frame_index: self.frame_index.saturating_sub(1) as u32,
            screen_crc,
            state_bytes,
        });
        let v3 = AutorunFileV3Ser {
            version: AUTORUN_VERSION,
            frames: encode_rle_frames(&self.autorun.frames),
            checkpoints: &self.autorun.checkpoints,
        };
        serde_json::to_vec_pretty(&v3).unwrap_or_default()
    }

    /// Check whether the just-completed frame has a playback checkpoint, verify CRC.
    ///
    /// Returns `Some(true)` on CRC match, `Some(false)` on mismatch, `None` if no checkpoint.
    pub fn check_playback_checkpoint(&mut self, screen_crc: u32) -> Option<bool> {
        let current = (self.frame_index - 1) as u32;
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

    /// True while in extend mode and still replaying the existing recording.
    pub fn is_extending_playback(&self) -> bool {
        self.extending_playback && self.frame_index < self.autorun.frames.len()
    }

    /// True when the state is in recording mode (original or extend).
    pub fn is_recording(&self) -> bool {
        self.mode == AutorunMode::Record
    }

    /// True when the state is in playback mode (not recording).
    pub fn is_playback(&self) -> bool {
        self.mode == AutorunMode::Playback
    }

    /// Returns `true` when pure playback (non-extend mode) has consumed all recorded frames.
    ///
    /// This is the signal for the frontend to stop emulation at the end of a run.
    /// Returns `false` for recording mode and extend mode.
    pub fn is_playback_finished(&self) -> bool {
        self.mode == AutorunMode::Playback && self.frame_index >= self.autorun.frames.len()
    }

    /// Number of CRC mismatches detected during playback.
    pub fn crc_mismatches(&self) -> usize {
        self.crc_mismatches
    }

    /// Total number of checkpoints verified during playback.
    pub fn total_checkpoints_verified(&self) -> usize {
        self.total_checkpoints_verified
    }

    /// Current frame index.
    pub fn current_frame_index(&self) -> usize {
        self.frame_index
    }

    /// Total number of frames in the recording.
    pub fn total_frames(&self) -> usize {
        self.autorun.frames.len()
    }
}

fn parse_autorun_bytes(bytes: &[u8]) -> Result<AutorunFile, String> {
    let json_value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Failed to parse autorun file: {e}"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_autorun_bytes(num_frames: usize, num_checkpoints: usize) -> Vec<u8> {
        let checkpoints: Vec<AutorunCheckpoint> = (0..num_checkpoints)
            .map(|i| {
                let frame = ((i + 1) * num_frames)
                    .checked_div(num_checkpoints)
                    .unwrap_or(0)
                    .saturating_sub(1) as u32;
                AutorunCheckpoint {
                    frame_index: frame,
                    screen_crc: (i as u32 + 1) * 0x1111,
                    state_bytes: format!("state{i}").into_bytes(),
                }
            })
            .collect();
        let frames: Vec<AutorunFrame> = (0..num_frames)
            .map(|i| AutorunFrame {
                player1: (i % 256) as u8,
                player2: 0,
            })
            .collect();
        let v3 = AutorunFileV3Ser {
            version: AUTORUN_VERSION,
            frames: encode_rle_frames(&frames),
            checkpoints: &checkpoints,
        };
        serde_json::to_vec(&v3).expect("serialize")
    }

    /// Serialize an in-memory AutorunFile to v3 RLE JSON bytes (for tests that
    /// construct an AutorunFile manually).
    fn autorun_to_v3_bytes(file: &AutorunFile) -> Vec<u8> {
        let v3 = AutorunFileV3Ser {
            version: AUTORUN_VERSION,
            frames: encode_rle_frames(&file.frames),
            checkpoints: &file.checkpoints,
        };
        serde_json::to_vec(&v3).expect("serialize")
    }

    // ── new_recording ────────────────────────────────────────────────────────

    #[test]
    fn new_recording_starts_with_no_frames() {
        let state = WasmAutorunState::new_recording();
        assert_eq!(state.total_frames(), 0);
    }

    #[test]
    fn new_recording_is_in_record_mode() {
        let state = WasmAutorunState::new_recording();
        assert!(state.is_recording());
        assert!(!state.is_playback());
    }

    #[test]
    fn new_recording_frame_index_is_zero() {
        let state = WasmAutorunState::new_recording();
        assert_eq!(state.current_frame_index(), 0);
    }

    // ── record_frame ────────────────────────────────────────────────────────

    #[test]
    fn record_frame_returns_true_at_checkpoint_interval() {
        let mut state = WasmAutorunState::new_recording();
        for _ in 0..299 {
            assert!(!state.record_frame(0, 0));
        }
        assert!(
            state.record_frame(0, 0),
            "frame 300 should trigger checkpoint"
        );
    }

    #[test]
    fn record_frame_increments_frame_index() {
        let mut state = WasmAutorunState::new_recording();
        state.record_frame(1, 2);
        assert_eq!(state.current_frame_index(), 1);
    }

    // ── record_checkpoint + finalize_recording ───────────────────────────────

    #[test]
    fn finalize_recording_returns_valid_bytes() {
        let mut state = WasmAutorunState::new_recording();
        state.record_frame(1, 0);
        let bytes = state.finalize_recording(0xDEAD, b"end".to_vec());
        let file: AutorunFile = serde_json::from_slice(&bytes).expect("bytes must be valid JSON");
        assert_eq!(file.version, AUTORUN_VERSION);
        assert_eq!(file.frames.len(), 1);
        assert_eq!(file.checkpoints.last().unwrap().screen_crc, 0xDEAD);
    }

    #[test]
    fn record_checkpoint_stores_crc_and_state() {
        let mut state = WasmAutorunState::new_recording();
        for _ in 0..300 {
            state.record_frame(0, 0);
        }
        state.record_checkpoint(0xCAFE, b"s".to_vec());
        assert_eq!(state.autorun.checkpoints.len(), 1);
        assert_eq!(state.autorun.checkpoints[0].screen_crc, 0xCAFE);
    }

    // ── new_playback – basic ────────────────────────────────────────────────

    #[test]
    fn new_playback_from_start_has_no_pending_restore() {
        let bytes = make_autorun_bytes(300, 1);
        let (state, pending) =
            WasmAutorunState::new_playback(&bytes, None, false).expect("create playback");
        assert!(pending.is_none());
        assert_eq!(state.current_frame_index(), 0);
    }

    #[test]
    fn new_playback_is_in_playback_mode() {
        let bytes = make_autorun_bytes(300, 1);
        let (state, _) =
            WasmAutorunState::new_playback(&bytes, None, false).expect("create playback");
        assert!(state.is_playback());
        assert!(!state.is_recording());
    }

    #[test]
    fn new_playback_from_checkpoint_returns_pending_restore() {
        let bytes = make_autorun_bytes(600, 2);
        let (state, pending) = WasmAutorunState::new_playback(&bytes, Some(0), false).expect("ok");
        assert!(pending.is_some());
        // Frame index should be checkpoint[0].frame_index + 1
        let file: AutorunFile = serde_json::from_slice(&bytes).unwrap();
        let expected_idx = file.checkpoints[0].frame_index as usize + 1;
        assert_eq!(state.current_frame_index(), expected_idx);
    }

    #[test]
    fn new_playback_invalid_bytes_returns_error() {
        let result = WasmAutorunState::new_playback(b"not json", None, false);
        assert!(result.is_err());
    }

    #[test]
    fn new_playback_checkpoint_out_of_range_returns_error() {
        let bytes = make_autorun_bytes(300, 1);
        let result = WasmAutorunState::new_playback(&bytes, Some(99), false);
        assert!(result.is_err());
    }

    // ── next_playback_frame ─────────────────────────────────────────────────

    #[test]
    fn next_playback_frame_returns_recorded_input() {
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 42,
                player2: 7,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0,
                state_bytes: vec![],
            }],
        };
        let bytes = autorun_to_v3_bytes(&file);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        state.begin_frame();
        let frame = state.next_playback_frame();
        assert!(frame.is_some());
        let f = frame.unwrap();
        assert_eq!(f.player1, 42);
        assert_eq!(f.player2, 7);
    }

    #[test]
    fn next_playback_frame_returns_none_when_exhausted() {
        let bytes = make_autorun_bytes(1, 1);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        state.begin_frame();
        state.next_playback_frame(); // consume
        state.begin_frame();
        assert!(state.next_playback_frame().is_none());
    }

    // ── extend mode ─────────────────────────────────────────────────────────

    #[test]
    fn extend_mode_is_extending_playback_initially() {
        let bytes = make_autorun_bytes(600, 2);
        let (state, _) = WasmAutorunState::new_playback(&bytes, None, true).unwrap();
        assert!(state.is_extending_playback());
        assert!(state.is_recording()); // extend = record mode
    }

    #[test]
    fn extend_mode_with_no_checkpoints_starts_fresh() {
        let bytes = make_autorun_bytes(0, 0);
        let (state, pending) = WasmAutorunState::new_playback(&bytes, None, true).unwrap();
        assert!(pending.is_none());
        assert!(!state.is_extending_playback());
        assert_eq!(state.current_frame_index(), 0);
    }

    // ── CRC check ───────────────────────────────────────────────────────────

    #[test]
    fn check_playback_checkpoint_detects_mismatch() {
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0x1234,
                state_bytes: vec![],
            }],
        };
        let bytes = autorun_to_v3_bytes(&file);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        state.begin_frame();
        state.next_playback_frame();
        let result = state.check_playback_checkpoint(0xBAD0);
        assert_eq!(result, Some(false));
        assert_eq!(state.crc_mismatches(), 1);
    }

    #[test]
    fn check_playback_checkpoint_matching_crc_no_mismatch() {
        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0x5678,
                state_bytes: vec![],
            }],
        };
        let bytes = autorun_to_v3_bytes(&file);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        state.begin_frame();
        state.next_playback_frame();
        let result = state.check_playback_checkpoint(0x5678);
        assert_eq!(result, Some(true));
        assert_eq!(state.crc_mismatches(), 0);
        assert_eq!(state.total_checkpoints_verified(), 1);
    }

    // ── is_playback_finished ─────────────────────────────────────────────────

    #[test]
    fn playback_not_finished_before_frames_exhausted() {
        let bytes = make_autorun_bytes(3, 0);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        assert!(!state.is_playback_finished());
        state.begin_frame();
        state.next_playback_frame();
        assert!(!state.is_playback_finished());
    }

    #[test]
    fn playback_finished_when_all_frames_consumed() {
        let bytes = make_autorun_bytes(2, 0);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, false).unwrap();
        state.begin_frame();
        state.next_playback_frame();
        state.begin_frame();
        state.next_playback_frame();
        assert!(state.is_playback_finished());
    }

    #[test]
    fn playback_finished_is_false_for_recording_mode() {
        let mut state = WasmAutorunState::new_recording();
        // Drive frame_index well past 0 by recording frames
        state.begin_frame();
        state.record_frame(0, 0);
        state.begin_frame();
        state.record_frame(0, 0);
        assert!(!state.is_playback_finished());
    }

    #[test]
    fn playback_finished_is_false_for_extend_mode() {
        // Extend mode uses Record mode internally; playback_finished must stay false
        let bytes = make_autorun_bytes(2, 1);
        let (mut state, _) = WasmAutorunState::new_playback(&bytes, None, true).unwrap();
        // Drain the pre-recorded frames
        state.begin_frame();
        state.next_playback_frame();
        state.begin_frame();
        state.next_playback_frame();
        assert!(!state.is_playback_finished());
    }

    // ── version compatibility ────────────────────────────────────────────────

    #[test]
    fn parse_autorun_bytes_accepts_v2_non_rle_format() {
        // A v2 autorun file has per-frame entries without RLE compression.
        // parse_autorun_bytes should accept v2 and normalize to v3 in memory.
        let v2_json = serde_json::json!({
            "version": 2,
            "frames": [
                {"player1": 4, "player2": 0},
                {"player1": 4, "player2": 0},
                {"player1": 7, "player2": 1}
            ],
            "checkpoints": []
        });
        let bytes = serde_json::to_vec(&v2_json).unwrap();
        let file = parse_autorun_bytes(&bytes).expect("should accept v2 format");
        assert_eq!(file.version, AUTORUN_VERSION);
        assert_eq!(file.frames.len(), 3);
        assert_eq!(file.frames[0].player1, 4);
        assert_eq!(file.frames[2].player1, 7);
        assert_eq!(file.frames[2].player2, 1);
    }

    #[test]
    fn parse_autorun_bytes_accepts_v3_rle_format() {
        // A v3 autorun file on disk uses RLE-encoded frames (with `repeat` field).
        // parse_autorun_bytes should decode RLE and expand to per-frame entries.
        let v3_json = serde_json::json!({
            "version": 3,
            "frames": [
                {"player1": 0, "player2": 0, "repeat": 3},
                {"player1": 1, "player2": 0, "repeat": 1}
            ],
            "checkpoints": []
        });
        let bytes = serde_json::to_vec(&v3_json).unwrap();
        let file = parse_autorun_bytes(&bytes).expect("should accept v3 RLE format");
        assert_eq!(file.version, AUTORUN_VERSION);
        assert_eq!(file.frames.len(), 4);
        assert_eq!(file.frames[0].player1, 0);
        assert_eq!(file.frames[3].player1, 1);
    }

    #[test]
    fn finalize_recording_produces_v3_rle_json() {
        // finalize_recording should produce v3 JSON with RLE-encoded frames,
        // matching the native save format.
        let mut state = WasmAutorunState::new_recording();
        // Record 3 identical frames + 1 different frame
        state.record_frame(0, 0);
        state.record_frame(0, 0);
        state.record_frame(0, 0);
        state.record_frame(1, 0);

        let bytes = state.finalize_recording(0xDEAD, b"end".to_vec());
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // The frames array should use RLE encoding (2 entries, not 4)
        let frames = json["frames"].as_array().unwrap();
        assert_eq!(
            frames.len(),
            2,
            "should have 2 RLE entries, not 4 per-frame entries"
        );
        assert_eq!(frames[0]["repeat"], 3);
        assert_eq!(frames[0]["player1"], 0);
        assert_eq!(frames[1]["repeat"], 1);
        assert_eq!(frames[1]["player1"], 1);
    }
}
