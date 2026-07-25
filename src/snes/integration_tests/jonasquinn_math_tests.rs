//! Automates the jonasquinn-mirrored colour-math proof ROM
//! (`roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/color_halve_proof/`)
//! for issue #2880.
//!
//! `demo.smc` proves that half colour math halves AFTER the add: it
//! busy-waits on $4212 HBlank and rewrites COLDATA per scanline so the
//! top and bottom halves of the screen render B=7 vs B=8 -- a
//! halve-before-add implementation would show a different split. The
//! screen is static once the per-scanline write loop is running.
//!
//! `test_math.sfc` from the sibling `test_math/` directory is
//! deliberately NOT automated here: it is a CPU multiply/divide
//! ($4202-$4217) latency test that writes its results to SRAM and ends
//! on a solid-colour screen, so a screen CRC would only prove the ROM
//! ran to completion; it remains a documented candidate asset (see the
//! manifest notes).
//!
//! The golden CRC follows the #2878/#2879 baseline workflow: the screen
//! settles at frame 2 (per-frame CRC probe to 700), is sampled at frame
//! 66, and the capture matches a Mesen2 headless capture at the same
//! frame pixel-exactly; see README-SNES.md.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const COLOR_HALVE_PROOF_ROM: &str =
    "roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/color_halve_proof/demo.smc";

/// Settles at frame 2; sampled with the standard settle margin.
const SAMPLE_FRAME: u32 = 66;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_halve_proof() {
        let rom = fs::read(Path::new(COLOR_HALVE_PROOF_ROM))
            .unwrap_or_else(|err| panic!("failed to read ROM {COLOR_HALVE_PROOF_ROM}: {err}"));
        let expected_crc = 0x450F_573E;
        let result = run_rom_with_oracle(
            &rom,
            "color_halve_proof-demo.smc",
            "jonasquinn_math_tests",
            RunConfig::new(2_000_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "color_halve_proof demo.smc: rendered screen at frame \
             {SAMPLE_FRAME} no longer matches the Mesen2-approved golden \
             CRC (got 0x{:08X}); if this is an intentional rendering \
             change, re-approve the golden per README-SNES.md",
            result.screen_crc32
        );
    }
}
