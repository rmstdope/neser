//! Automates the vendored undisbeliever PPU OBJ / sprite-limit test ROM
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-obj/`):
//! `object-dropout-test.sfc` (v3) renders one static scene exercising the
//! OBJ range-over limit (36 sprites on a scanline, official $213E bit 6
//! naming), the time-over limit (>34 tile slivers per line, plus a
//! V/H-flipped variant; $213E bit 7) and the X=256 bug (issue #2879).
//!
//! The ROM was settle-probed per the #2878 baseline workflow: static,
//! settles at frame 6 (stable for the remaining 1794 probed frames),
//! sampled at settle + 60 (frame 66). After the #2999 OBJ eval/fetch
//! pipeline landed (34-sliver time-over limit with reverse-order fetch,
//! X=256 range/time participation), the frame-66 capture is a
//! byte-for-byte match for a fresh Mesen2 headless capture (0/57344
//! differing pixels at shift (0,0); Mesen2 flags per README-SNES.md,
//! including --snes.disableFrameSkipping=true), so the CRC below is a
//! Mesen2-approved golden.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const UNDISBELIEVER_PPU_OBJ_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-obj";

#[cfg(test)]
mod tests {
    use super::*;

    /// Like the other visual PPU suites, a mismatch here means the approved
    /// golden screen changed, not that the ROM itself reported a failure.
    fn run_ppu_obj_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(UNDISBELIEVER_PPU_OBJ_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "undisbeliever_ppu_obj_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {frames} no longer matches the \
             Mesen2-approved golden CRC (got 0x{:08X}); if this is an \
             intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    /// Frame 66 is pixel-identical to Mesen2 since the #2999 OBJ pipeline
    /// (see module docs for the capture/diff evidence).
    #[test]
    fn object_dropout() {
        run_ppu_obj_screen_crc("object-dropout-test.sfc", 66, 0xE9AF_DDE6);
    }
}
