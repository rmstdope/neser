//! SDL-free headless playback for GB autorun recordings.
//!
//! Mirrors `nes::autorun::headless_playback` for the Game Boy (DMG).
//! Provides [`run_headless_playback`] to replay a recording and verify
//! screen CRCs at every checkpoint, plus CRC recalculation helpers.

use crate::gb::bus::DmgBus;
use crate::gb::console::Gb;
use crate::gb::console::save_state::GbSaveState;
use crate::platform::autorun::AutorunFile;

/// Summary of a headless playback run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessPlaybackResult {
    /// Number of checkpoint CRC mismatches detected.
    pub crc_mismatches: usize,
    /// Total number of checkpoints verified.
    pub total_checkpoints_verified: usize,
}

impl HeadlessPlaybackResult {
    /// Returns `true` if all checkpoint CRCs matched.
    pub fn is_ok(&self) -> bool {
        self.crc_mismatches == 0
    }
}

/// Restore emulator state from checkpoint `cp_idx` and return `(start_frame, first_checkpoint_idx)`.
///
/// Returns `(0, 0)` when `start_checkpoint` is `None`.
fn apply_start_checkpoint(
    gb: &mut Gb<DmgBus>,
    file: &AutorunFile,
    start_checkpoint: Option<usize>,
) -> Result<(usize, usize), String> {
    let Some(cp_idx) = start_checkpoint else {
        return Ok((0, 0));
    };
    let cp = file
        .checkpoints
        .get(cp_idx)
        .ok_or_else(|| format!("Checkpoint index {cp_idx} out of range"))?;
    let state = GbSaveState::from_bytes(&cp.state_bytes)
        .map_err(|e| format!("Failed to deserialize checkpoint {cp_idx} state: {e}"))?;
    gb.load_state(&state)
        .map_err(|e| format!("Failed to load checkpoint {cp_idx} state: {e}"))?;
    Ok((cp.frame_index as usize + 1, cp_idx + 1))
}

/// Run a full headless playback of `file` on `gb`.
///
/// If `start_checkpoint` is `Some(n)`, the emulator state is restored from
/// checkpoint `n` and playback begins from that checkpoint's `frame_index`.
/// Otherwise playback starts from frame 0.
///
/// At every checkpoint frame, the screen CRC is compared against the stored
/// value. The result contains the mismatch count and the total number of
/// checkpoints verified.
///
/// The `gb` must already have the correct cartridge loaded and be reset.
pub fn run_headless_playback(
    gb: &mut Gb<DmgBus>,
    file: &AutorunFile,
    start_checkpoint: Option<usize>,
) -> Result<HeadlessPlaybackResult, String> {
    let (start_frame, first_checkpoint_idx) = apply_start_checkpoint(gb, file, start_checkpoint)?;

    let mut cp_idx = first_checkpoint_idx;
    let mut crc_mismatches = 0usize;
    let mut total_verified = 0usize;

    for (frame_idx, frame) in (start_frame..).zip(file.frames.iter().skip(start_frame)) {
        // Apply controller input (GB has one player; use player1 bits)
        gb.cpu.bus.joypad.set_states(frame.player1);

        // Emulate one full frame
        run_one_frame(gb);

        // Check if this frame is a checkpoint
        while cp_idx < file.checkpoints.len()
            && file.checkpoints[cp_idx].frame_index as usize == frame_idx
        {
            let cp = &file.checkpoints[cp_idx];
            let actual_crc = gb.screen_crc32();
            if actual_crc != cp.screen_crc {
                crc_mismatches += 1;
            }
            total_verified += 1;
            cp_idx += 1;
        }
    }

    Ok(HeadlessPlaybackResult {
        crc_mismatches,
        total_checkpoints_verified: total_verified,
    })
}

/// Run a full headless playback of `file` and rewrite checkpoint screen CRCs.
///
/// Returns the number of checkpoints updated.
pub fn recalculate_checkpoint_crcs(
    gb: &mut Gb<DmgBus>,
    file: &mut AutorunFile,
    start_checkpoint: Option<usize>,
) -> Result<usize, String> {
    recalculate_checkpoint_crcs_with_progress(gb, file, start_checkpoint, |_, _| {})
}

/// Run a full headless playback of `file`, rewrite checkpoint screen CRCs,
/// and report progress via callback as `(done, total)`.
pub fn recalculate_checkpoint_crcs_with_progress<F>(
    gb: &mut Gb<DmgBus>,
    file: &mut AutorunFile,
    start_checkpoint: Option<usize>,
    mut on_progress: F,
) -> Result<usize, String>
where
    F: FnMut(usize, usize),
{
    let (start_frame, first_checkpoint_idx) = apply_start_checkpoint(gb, file, start_checkpoint)?;

    let mut cp_idx = first_checkpoint_idx;
    let mut updated = 0usize;
    let total_to_update = file.checkpoints.len().saturating_sub(first_checkpoint_idx);

    for (frame_idx, frame) in (start_frame..).zip(file.frames.iter().skip(start_frame)) {
        gb.cpu.bus.joypad.set_states(frame.player1);

        run_one_frame(gb);

        while cp_idx < file.checkpoints.len()
            && file.checkpoints[cp_idx].frame_index as usize == frame_idx
        {
            let actual_crc = gb.screen_crc32();
            file.checkpoints[cp_idx].screen_crc = actual_crc;
            updated += 1;
            on_progress(updated, total_to_update);
            cp_idx += 1;
        }
    }

    Ok(updated)
}

/// Step the GB until the PPU raises the frame-ready flag, then clear it.
///
/// Drains audio samples to avoid unbounded accumulation.
pub fn run_one_frame(gb: &mut Gb<DmgBus>) {
    while !gb.is_frame_ready() {
        gb.step();
        while gb.cpu.bus.sample_ready() {
            gb.cpu.bus.take_sample();
        }
    }
    gb.clear_frame_ready();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::DmgBus;
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::console::Gb;
    use crate::gb::model::DmgModel;
    use crate::platform::autorun::{AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFrame};

    /// Build a minimal valid ROM-only (MBC0) cartridge that loops forever.
    fn minimal_gb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        // Put an infinite loop (JR -2 = 0x18, 0xFE) at the cartridge entry point $0100.
        rom[0x0100] = 0x18; // JR offset
        rom[0x0101] = 0xFE; // -2 (jumps back to $0100)
        for (index, byte) in rom[0x0104..0x0134].iter_mut().enumerate() {
            *byte = ((index as u8).wrapping_mul(17)) ^ 0x5A;
        }
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn make_gb() -> Gb<DmgBus> {
        let rom = minimal_gb_rom();
        let cart = load_cartridge(&rom).expect("valid ROM");
        Gb::new(DmgBus::new(cart, DmgModel::DmgB))
    }

    /// Run the boot ROM to completion, advancing from $0000 to $0100.
    fn skip_boot_rom(gb: &mut Gb<DmgBus>) {
        // The DMG boot ROM plays a ~58-frame logo scroll + sound animation
        // (70224 cycles/frame × 60 frames ≈ 4.2M cycles), so we need a generous limit.
        const BOOT_ROM_STEP_LIMIT: usize = 10_000_000;

        // The boot ROM ends by jumping to $0100. Run until PC reaches $0100.
        for _ in 0..BOOT_ROM_STEP_LIMIT {
            gb.step();
            if gb.cpu.regs.pc == 0x0100 {
                return;
            }
        }

        panic!(
            "skip_boot_rom did not reach $0100 within {} steps; final PC=${:04X}",
            BOOT_ROM_STEP_LIMIT, gb.cpu.regs.pc
        );
    }

    fn run_gb_frames(gb: &mut Gb<DmgBus>, n: u32) {
        for _ in 0..n {
            run_one_frame(gb);
        }
    }

    #[test]
    fn test_headless_playback_empty_recording_succeeds() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![],
        };

        let result = run_headless_playback(&mut gb, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_headless_playback_matching_crc_reports_no_mismatches() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        // Run 1 frame and capture screen CRC
        run_gb_frames(&mut gb, 1);
        let crc_after_1 = gb.screen_crc32();
        let state = gb.save_state();
        let state_bytes = state.to_bytes().expect("serialize state");

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: crc_after_1,
                state_bytes,
            }],
        };

        // Reset and replay from same starting state
        let mut gb2 = make_gb();
        skip_boot_rom(&mut gb2);
        let result = run_headless_playback(&mut gb2, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_headless_playback_wrong_crc_reports_mismatch() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        let state_bytes = gb.save_state().to_bytes().expect("serialize state");

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0xDEADBEEF,
                state_bytes,
            }],
        };

        let result = run_headless_playback(&mut gb, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 1);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_headless_playback_from_checkpoint_restores_state() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        // Run to frame 5 to get a state
        run_gb_frames(&mut gb, 5);
        let state_at_5 = gb.save_state();
        let state_bytes_5 = state_at_5.to_bytes().expect("serialize");

        // Run one more frame to capture the expected CRC
        run_gb_frames(&mut gb, 1);
        let crc_at_6 = gb.screen_crc32();

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![
                AutorunFrame {
                    player1: 0,
                    player2: 0,
                };
                10
            ],
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 4,
                    screen_crc: 0x0000,
                    state_bytes: state_bytes_5.clone(),
                },
                AutorunCheckpoint {
                    frame_index: 5,
                    screen_crc: crc_at_6,
                    state_bytes: vec![],
                },
            ],
        };

        // Start from checkpoint 0 (frame 4)
        let mut gb2 = make_gb();
        let result = run_headless_playback(&mut gb2, &file, Some(0)).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 0);
    }

    #[test]
    fn test_recalculate_checkpoint_crcs_updates_wrong_crc() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        run_gb_frames(&mut gb, 1);
        let state_bytes = gb.save_state().to_bytes().expect("serialize");

        let mut file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0xDEADBEEF,
                state_bytes,
            }],
        };

        let mut gb2 = make_gb();
        skip_boot_rom(&mut gb2);
        let updated =
            recalculate_checkpoint_crcs(&mut gb2, &mut file, None).expect("recalculate ok");

        assert_eq!(updated, 1);
        assert_ne!(file.checkpoints[0].screen_crc, 0xDEADBEEF);

        // Verify playback with the new CRC passes
        let mut gb3 = make_gb();
        skip_boot_rom(&mut gb3);
        let result = run_headless_playback(&mut gb3, &file, None).expect("playback ok");
        assert!(result.is_ok());
    }

    #[test]
    fn test_save_state_roundtrip_is_deterministic() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);
        run_gb_frames(&mut gb, 3);

        // Save state
        let state = gb.save_state();
        let bytes = state.to_bytes().expect("serialize");

        // Load state into a fresh console
        let mut gb2 = make_gb();
        let restored = GbSaveState::from_bytes(&bytes).expect("deserialize");
        gb2.load_state(&restored).expect("load state");

        // Run both from same state — screen CRC should match
        run_gb_frames(&mut gb, 1);
        run_gb_frames(&mut gb2, 1);
        assert_eq!(gb.screen_crc32(), gb2.screen_crc32());
    }

    #[test]
    fn test_extend_mode_produces_longer_valid_recording() {
        let mut gb = make_gb();
        skip_boot_rom(&mut gb);

        // Record initial 5 frames with a checkpoint at frame 4
        let mut frames = Vec::new();
        for _ in 0..5 {
            frames.push(AutorunFrame {
                player1: 0,
                player2: 0,
            });
            run_one_frame(&mut gb);
        }
        let state_at_end = gb.save_state();
        let state_bytes = state_at_end.to_bytes().expect("serialize");
        let crc_at_end = gb.screen_crc32();

        let mut file = AutorunFile {
            version: AUTORUN_VERSION,
            frames,
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 4,
                screen_crc: crc_at_end,
                state_bytes,
            }],
        };

        // Extend from checkpoint 0: add more frames
        // Restore state from checkpoint
        let cp = &file.checkpoints[0];
        let restore_state = GbSaveState::from_bytes(&cp.state_bytes).expect("deserialize");
        let mut gb_ext = make_gb();
        gb_ext.load_state(&restore_state).expect("load");

        // Record 5 more frames
        for _ in 0..5 {
            file.frames.push(AutorunFrame {
                player1: 0,
                player2: 0,
            });
            run_one_frame(&mut gb_ext);
        }
        let extend_state = gb_ext.save_state();
        let extend_bytes = extend_state.to_bytes().expect("serialize");
        let extend_crc = gb_ext.screen_crc32();
        file.checkpoints.push(AutorunCheckpoint {
            frame_index: 9,
            screen_crc: extend_crc,
            state_bytes: extend_bytes,
        });

        // Verify the extended recording plays back correctly
        let mut gb_verify = make_gb();
        skip_boot_rom(&mut gb_verify);
        let result = run_headless_playback(&mut gb_verify, &file, None).expect("playback ok");
        assert!(result.is_ok());
        assert_eq!(result.total_checkpoints_verified, 2);
    }
}
