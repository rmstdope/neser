//! SDL-free headless playback for autorun recordings.
//!
//! This module provides [`run_headless_playback`] which replays a recording and verifies
//! screen CRCs at every checkpoint. It has no SDL dependencies and can be called from
//! integration tests or other non-SDL frontends (e.g. web).

use crate::nes::console::{Nes, SaveState};
use crate::platform::autorun::AutorunFile;

/// Summary of a headless playback run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct HeadlessPlaybackResult {
    /// Number of checkpoint CRC mismatches detected.
    pub crc_mismatches: usize,
    /// Total number of checkpoints verified.
    pub total_checkpoints_verified: usize,
}

impl HeadlessPlaybackResult {
    /// Returns `true` if all checkpoint CRCs matched.
    #[allow(dead_code)]
    pub fn is_ok(&self) -> bool {
        self.crc_mismatches == 0
    }
}

/// Run a full headless playback of `file` on `nes`.
///
/// If `start_checkpoint` is `Some(n)`, the emulator state is restored from checkpoint `n`
/// and playback begins from that checkpoint's `frame_index`. Otherwise playback starts
/// from frame 0.
///
/// At every checkpoint frame, the screen CRC is computed and compared against the stored
/// value. The result contains the mismatch count and the total number of checkpoints
/// verified.
///
/// The `nes` must already have the correct cartridge loaded and be reset before calling.
#[allow(dead_code)]
pub fn run_headless_playback(
    nes: &mut Nes,
    file: &AutorunFile,
    start_checkpoint: Option<usize>,
) -> Result<HeadlessPlaybackResult, String> {
    let (start_frame, first_checkpoint_idx) = if let Some(cp_idx) = start_checkpoint {
        let cp = file
            .checkpoints
            .get(cp_idx)
            .ok_or_else(|| format!("Checkpoint index {cp_idx} out of range"))?;
        let state = SaveState::from_bytes(&cp.state_bytes)
            .map_err(|e| format!("Failed to deserialize checkpoint {cp_idx} state: {e}"))?;
        nes.load_state(&state)
            .map_err(|e| format!("Failed to load checkpoint {cp_idx} state: {e}"))?;
        // State is captured AFTER frame cp.frame_index; start playing from the next frame
        // and verify from the checkpoint after the start checkpoint.
        (cp.frame_index as usize + 1, cp_idx + 1)
    } else {
        (0, 0)
    };

    let mut cp_idx = first_checkpoint_idx;
    let mut crc_mismatches = 0usize;
    let mut total_verified = 0usize;

    for (frame_idx, frame) in (start_frame..).zip(file.frames.iter().skip(start_frame)) {
        // Apply controller input
        nes.set_joypad_button_states(1, frame.player1);
        nes.set_joypad_button_states(2, frame.player2);

        // Emulate one full frame
        run_one_frame(nes);

        // Check if this frame is a checkpoint
        while cp_idx < file.checkpoints.len()
            && file.checkpoints[cp_idx].frame_index as usize == frame_idx
        {
            let cp = &file.checkpoints[cp_idx];
            let actual_crc = nes.ppu().borrow().screen_buffer().crc32();
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
#[allow(dead_code)]
pub fn recalculate_checkpoint_crcs(
    nes: &mut Nes,
    file: &mut AutorunFile,
    start_checkpoint: Option<usize>,
) -> Result<usize, String> {
    recalculate_checkpoint_crcs_with_progress(nes, file, start_checkpoint, |_, _| {})
}

/// Run a full headless playback of `file`, rewrite checkpoint screen CRCs,
/// and report progress via callback as `(done, total)`.
pub fn recalculate_checkpoint_crcs_with_progress<F>(
    nes: &mut Nes,
    file: &mut AutorunFile,
    start_checkpoint: Option<usize>,
    mut on_progress: F,
) -> Result<usize, String>
where
    F: FnMut(usize, usize),
{
    let (start_frame, first_checkpoint_idx) = if let Some(cp_idx) = start_checkpoint {
        let cp = file
            .checkpoints
            .get(cp_idx)
            .ok_or_else(|| format!("Checkpoint index {cp_idx} out of range"))?;
        let state = SaveState::from_bytes(&cp.state_bytes)
            .map_err(|e| format!("Failed to deserialize checkpoint {cp_idx} state: {e}"))?;
        nes.load_state(&state)
            .map_err(|e| format!("Failed to load checkpoint {cp_idx} state: {e}"))?;
        (cp.frame_index as usize + 1, cp_idx + 1)
    } else {
        (0, 0)
    };

    let mut cp_idx = first_checkpoint_idx;
    let mut updated = 0usize;
    let total_to_update = file.checkpoints.len().saturating_sub(first_checkpoint_idx);

    for (frame_idx, frame) in (start_frame..).zip(file.frames.iter().skip(start_frame)) {
        nes.set_joypad_button_states(1, frame.player1);
        nes.set_joypad_button_states(2, frame.player2);

        run_one_frame(nes);

        while cp_idx < file.checkpoints.len()
            && file.checkpoints[cp_idx].frame_index as usize == frame_idx
        {
            let actual_crc = nes.ppu().borrow().screen_buffer().crc32();
            file.checkpoints[cp_idx].screen_crc = actual_crc;
            updated += 1;
            on_progress(updated, total_to_update);
            cp_idx += 1;
        }
    }

    Ok(updated)
}

/// Emulate the NES until the PPU signals a completed frame, then clear the flag.
#[allow(dead_code)]
/// Run a single NES frame without audio, draining samples.
///
/// Used by headless playback and the native frontend's headless autorun loop.
pub fn run_one_frame(nes: &mut Nes) {
    while !nes.is_ready_to_render() && !nes.cpu_ref().is_halted() {
        nes.run_cpu_tick();
        // Drain audio samples to avoid unbounded accumulation
        while nes.sample_ready() {
            nes.get_sample();
        }
    }
    nes.clear_ready_to_render();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::{Config, Nes, RamInitMode};
    use crate::platform::app_context::AppContext;
    use crate::platform::autorun::{AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFrame};
    use crate::platform::config::FrontendConfig;

    /// Minimal NROM-128 ROM that just loops forever with no audio/PPU effects.
    fn minimal_nrom_rom() -> Vec<u8> {
        let mut rom = Vec::with_capacity(16 + 16 * 1024);
        rom.extend_from_slice(b"NES\x1A");
        rom.push(1); // 16KB PRG
        rom.push(0); // 0 CHR (CHR RAM)
        rom.push(0x00); // flags6
        rom.push(0x00); // flags7
        rom.extend_from_slice(&[0u8; 8]); // padding
        let mut prg = vec![0xEAu8; 16 * 1024]; // NOPs
        // Reset vector at $FFFC pointing to $C000
        prg[0x3FFC] = 0x00;
        prg[0x3FFD] = 0xC0;
        // Infinite loop at $C000: JMP $C000
        prg[0x0000] = 0x4C;
        prg[0x0001] = 0x00;
        prg[0x0002] = 0xC0;
        rom.extend_from_slice(&prg);
        rom
    }

    fn make_nes() -> Nes {
        let config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        Nes::new(AppContext::new_with_config(config))
    }

    fn make_nes_with_cart(rom: &[u8]) -> Nes {
        let mut nes = make_nes();
        let cart = crate::nes::cartridge::Cartridge::load_from_file(rom, "test.nes", None)
            .expect("load cart");
        nes.insert_cartridge(cart);
        nes.reset(false);
        nes
    }

    fn run_nes_frames(nes: &mut Nes, n: u32) {
        for _ in 0..n {
            run_one_frame(nes);
        }
    }

    #[test]
    fn test_headless_playback_empty_recording_succeeds_with_no_checkpoints() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![],
            checkpoints: vec![],
        };

        let result = run_headless_playback(&mut nes, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_headless_playback_matching_crc_reports_no_mismatches() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        // Run 1 frame and capture real screen CRC to build a correct recording
        run_nes_frames(&mut nes, 1);
        let state_after_1 = nes.save_state();
        let crc_after_1 = nes.ppu().borrow().screen_buffer().crc32();

        let state_bytes = state_after_1.to_bytes().expect("serialize state");

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

        // Reset and replay
        nes.reset(false);
        let result = run_headless_playback(&mut nes, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_headless_playback_wrong_crc_reports_mismatch() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        let state_bytes = nes.save_state().to_bytes().expect("serialize state");

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 0,
                player2: 0,
            }],
            checkpoints: vec![AutorunCheckpoint {
                frame_index: 0,
                screen_crc: 0xDEADBEEF, // Deliberately wrong
                state_bytes,
            }],
        };

        let result = run_headless_playback(&mut nes, &file, None).expect("playback ok");
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 1);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_headless_playback_from_checkpoint_restores_state() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        // Run to frame 299 to get a state
        run_nes_frames(&mut nes, 300);
        let state_at_300 = nes.save_state();
        let state_bytes_300 = state_at_300.to_bytes().expect("serialize");

        // Run one more frame from that state to know the expected CRC
        run_nes_frames(&mut nes, 1);
        let crc_at_301 = nes.ppu().borrow().screen_buffer().crc32();

        let file = AutorunFile {
            version: AUTORUN_VERSION,
            // 600 frames total
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
                    screen_crc: 0x0000, // Not checked during this test
                    state_bytes: state_bytes_300.clone(),
                },
                AutorunCheckpoint {
                    frame_index: 300,
                    screen_crc: crc_at_301,
                    state_bytes: vec![], // Not used during this test
                },
            ],
        };

        // Start from checkpoint 0 (frame 299) - only checkpoint index 1 (frame 300) gets verified
        let mut nes2 = make_nes_with_cart(&rom);
        let result = run_headless_playback(&mut nes2, &file, Some(0)).expect("playback ok");
        // Checkpoint at frame 299 (index 0) is the start checkpoint and not re-verified;
        // checkpoint at frame 300 (index 1) should be verified once.
        assert_eq!(result.total_checkpoints_verified, 1);
        assert_eq!(result.crc_mismatches, 0);
    }

    #[test]
    fn test_recalculate_checkpoint_crcs_updates_mismatching_checkpoint_crc() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        run_nes_frames(&mut nes, 1);
        let state_after_1 = nes.save_state();
        let state_bytes = state_after_1.to_bytes().expect("serialize state");

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

        nes.reset(false);
        let updated =
            recalculate_checkpoint_crcs(&mut nes, &mut file, None).expect("recalculation succeeds");

        assert_eq!(updated, 1);
        assert_ne!(file.checkpoints[0].screen_crc, 0xDEADBEEF);
    }

    #[test]
    fn test_recalculate_checkpoint_crcs_reports_progress_for_each_checkpoint() {
        let rom = minimal_nrom_rom();
        let mut nes = make_nes_with_cart(&rom);

        run_nes_frames(&mut nes, 1);
        let state_after_1 = nes.save_state();
        let state_bytes = state_after_1.to_bytes().expect("serialize state");

        let mut file = AutorunFile {
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
            ],
            checkpoints: vec![
                AutorunCheckpoint {
                    frame_index: 0,
                    screen_crc: 0,
                    state_bytes: state_bytes.clone(),
                },
                AutorunCheckpoint {
                    frame_index: 1,
                    screen_crc: 0,
                    state_bytes,
                },
            ],
        };

        nes.reset(false);
        let mut progress = Vec::new();
        let updated =
            recalculate_checkpoint_crcs_with_progress(&mut nes, &mut file, None, |done, total| {
                progress.push((done, total))
            })
            .expect("recalculation succeeds");

        assert_eq!(updated, 2);
        assert_eq!(progress, vec![(1, 2), (2, 2)]);
    }
}
