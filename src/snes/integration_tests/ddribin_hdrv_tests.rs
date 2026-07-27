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
//! - The interlace combo (X tap at frame 340, sampled at frame 650)
//!   renders 256x448 in NESER vs Mesen2's 512x448; halving Mesen2's
//!   width leaves a 0-pixel diff, so the content is verified but the
//!   raw framebuffers are not comparable: `#[ignore]`d with NESER's
//!   current CRC pending the #3001 capture-geometry convention.
//! - The overscan combo (Y tap at frame 340, sampled at frame 650)
//!   renders 256x239 in NESER vs Mesen2's 256x224; NESER rows 0-223
//!   equal Mesen2's frame exactly (0-pixel diff at crop offset 0), so
//!   the content is verified: `#[ignore]`d likewise pending #3001.

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
            InputEvent {
                frame,
                button,
                pressed: true,
            },
            InputEvent {
                frame: frame + 1,
                button,
                pressed: false,
            },
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

    /// NESER's current CRC, NOT a Mesen2-approved golden: the rendered
    /// content matches Mesen2 exactly after halving Mesen2's 512-wide
    /// interlace capture, but the raw framebuffer geometries differ.
    #[test]
    #[ignore = "interlace capture geometry differs from Mesen2 (content verified via width-halving); pending #3001"]
    fn interlace_toggle() {
        run_hdrv_screen_crc("interlace", &tap(SnesButton::X, 340), 650, 0xD5DD_F545);
    }

    /// NESER's current CRC, NOT a Mesen2-approved golden: NESER rows
    /// 0-223 equal Mesen2's 224-line frame exactly, but NESER renders
    /// all 239 overscan lines.
    #[test]
    #[ignore = "239-line overscan capture geometry differs from Mesen2 (content verified rows 0-223); pending #3001"]
    fn overscan_toggle() {
        run_hdrv_screen_crc("overscan", &tap(SnesButton::Y, 340), 650, 0x8DE1_20EC);
    }
}
