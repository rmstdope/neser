//! Automates ddribin's hdrv-snes-test display-mode ROM
//! (`roms/snes/automated_tests/snes_test_roms/ddribin-hdrv-snes-test/hdrvtest.sfc`,
//! built once from the CC0 source subtree -- see that folder's README)
//! covering its joypad-driven test patterns and scan/height mode
//! switches (issue #2881).
//!
//! The ROM shows a splash screen until roughly frame 300 (input other
//! than Start is ignored during it -- taps must be scheduled after
//! ~frame 320), then 100% colorbars; A cycles test patterns, X toggles
//! interlace and Y toggles 239-line overscan.
//!
//! Baseline results (#2878/#2880 workflow; input scripts replayed in
//! Mesen2 via Lua `emu.setInput` per the #2879 recipe):
//!
//! - The default 256x224 colorbars (frame 371) and the graybars
//!   pattern (two A taps at frames 340/355, sampled at frame 520)
//!   match Mesen2 pixel-exactly: approved goldens.
//! - The interlace combo (X tap at frame 340, sampled at frame 650) and
//!   the overscan combo (Y tap at frame 340, same frame) were parked on
//!   a capture-geometry mismatch: NESER rendered 256x448 and 256x239
//!   against Mesen2's 512x448 and 256x224. #3001 aligned both
//!   dimensions and #3034 moved interlace column-doubling to write
//!   time, which changed NESER's output and left the recorded CRCs
//!   stale -- unnoticed because the tests were ignored. #3092 supplied
//!   the fresh input-scripted Mesen2 captures those changes called for:
//!   both now match Mesen2 exactly at the same geometry, so they are
//!   ordinary committed goldens rather than divergence records.

use super::rom_runner::{InputEvent, RunConfig, RunOracle, RunResult, run_rom_with_oracle};
use crate::snes::input::SnesButton;
use std::fs;
use std::path::Path;

const HDRV_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/ddribin-hdrv-snes-test";

#[cfg(test)]
mod tests {
    use super::*;

    fn run_hdrv_screen_crc(
        label: &str,
        script: &[InputEvent],
        frames: u32,
        expected_crc: u32,
    ) -> RunResult {
        let path = Path::new(HDRV_ROOT).join("hdrvtest.sfc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let name = if label.is_empty() {
            "hdrvtest.sfc".to_string()
        } else {
            format!("hdrvtest-{label}.sfc")
        };
        let result = run_rom_with_oracle(
            &rom,
            &name,
            "ddribin_hdrv_tests",
            RunConfig::new(700_000_000, 0).with_input_script(script),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{name}: rendered screen at frame {frames} no longer matches the \
             approved golden CRC (got 0x{:08X}); if this is an intentional \
             rendering change, re-approve the golden per README-SNES.md",
            result.screen_crc32
        );
        result
    }

    /// Tap `button` for one frame at `frame`.
    const fn tap(button: SnesButton, frame: u32) -> [InputEvent; 2] {
        [
            InputEvent::button(frame, button, true),
            InputEvent::button(frame + 1, button, false),
        ]
    }

    #[test]
    fn colorbars_default() {
        run_hdrv_screen_crc("", &[], 371, 0xF745_9692);
    }

    #[test]
    fn graybars() {
        let mut script = Vec::new();
        script.extend(tap(SnesButton::A, 340));
        script.extend(tap(SnesButton::A, 355));
        run_hdrv_screen_crc("graybars", &script, 520, 0x0AC8_BD62);
    }

    /// Mesen2-approved golden (#3092): since #3001 aligned the capture
    /// dimensions and #3034 moved interlace column-doubling to write
    /// time, NESER renders this combo at Mesen2's own 512x448 interlace
    /// geometry, and a fresh input-scripted headless capture at frame
    /// 650 hashes to the same value -- so the raw framebuffers are
    /// directly comparable at last.
    #[test]
    fn interlace_toggle() {
        run_hdrv_screen_crc("interlace", &tap(SnesButton::X, 340), 650, 0x902F_E4EE);
    }

    /// Mesen2-approved golden (#3092): since #3001 NESER captures the
    /// standard 224-line window for this combo, matching Mesen2's frame
    /// exactly.
    ///
    /// This CRC is byte-identical to `colorbars_default`'s golden, and by
    /// itself proves nothing about overscan (#3096). With overscan on,
    /// `screen_snapshot_rgb` crops the 239-line frame to 224 rows starting
    /// at internal row `OVERSCAN_CROP_TOP` (7); with it off, from row 0.
    /// hdrvtest's colorbars are *vertical* bars with no horizontal
    /// structure, so a 7-row vertical shift is invisible and both crops
    /// hash the same -- the equal CRC is exactly what you would see
    /// whether the Y tap registered or was swallowed. A matching Mesen2
    /// capture doesn't help either: Mesen2's overscan and non-overscan
    /// captures are identical for the same reason.
    ///
    /// The `overscan_239_enabled` assertion below is the actual oracle for
    /// this combo: it reads the PPU's SETINI overscan bit directly at the
    /// sample frame, so a regression that stops the Y tap from reaching
    /// SETINI (or stops SETINI from being honored) fails the test even
    /// though the cropped CRC can't see it. Confirmed by instrumented
    /// trace that the tap does register (SETINI bit 2 goes true at frame
    /// 341, one frame after the tap, and holds through frame 650).
    #[test]
    fn overscan_toggle() {
        let result = run_hdrv_screen_crc("overscan", &tap(SnesButton::Y, 340), 650, 0xF745_9692);
        assert!(
            result.overscan_239_enabled,
            "overscan_toggle: overscan_239_enabled() was false at frame 650 -- \
             the Y tap failed to enable SETINI overscan, or SETINI handling regressed"
        );
    }
}
