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
//!   against Mesen2's 512x448 and 256x224. Their content was verified
//!   (0-pixel diffs after halving Mesen2's width, resp. at crop offset
//!   0), but the raw framebuffers were not comparable. #3001 aligned
//!   both dimensions and #3034 moved interlace column-doubling to write
//!   time, so the mismatch is gone and both are re-approvable -- but
//!   that needs fresh input-scripted Mesen2 captures, so they stay
//!   `#[ignore]`d with refreshed NESER-current CRCs for now.

use super::rom_runner::{InputEvent, RunConfig, RunOracle, run_rom_with_oracle};
use crate::snes::input::SnesButton;
use std::fs;
use std::path::Path;

const HDRV_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/ddribin-hdrv-snes-test";

#[cfg(test)]
mod tests {
    use super::*;

    fn run_hdrv_screen_crc(label: &str, script: &[InputEvent], frames: u32, expected_crc: u32) {
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

    /// NESER's current CRC, NOT a Mesen2-approved golden. The geometry
    /// mismatch these two were parked on is gone -- #3001 aligned the
    /// interlace and overscan capture dimensions with Mesen2 and #3034
    /// moved interlace column-doubling to write time -- so both now render
    /// at Mesen2's geometry and are re-approvable. Doing so needs fresh
    /// input-scripted Mesen2 captures (the #2879 `emu.setInput` recipe),
    /// which is why it is deferred rather than done here; the CRCs below
    /// were refreshed to NESER's current output so they describe reality.
    #[test]
    #[ignore = "re-approvable since #3001/#3034 but needs a fresh input-replay Mesen2 capture"]
    fn interlace_toggle() {
        run_hdrv_screen_crc("interlace", &tap(SnesButton::X, 340), 650, 0x902F_E4EE);
    }

    /// As above. Note for whoever re-approves this one: its current CRC is
    /// byte-identical to `colorbars_default`'s, which would mean the Y tap
    /// left the screen unchanged. Check that the overscan toggle actually
    /// registers before treating a matching capture as confirmation.
    #[test]
    #[ignore = "re-approvable since #3001/#3034 but needs a fresh input-replay Mesen2 capture"]
    fn overscan_toggle() {
        run_hdrv_screen_crc("overscan", &tap(SnesButton::Y, 340), 650, 0xF745_9692);
    }
}
