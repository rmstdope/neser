//! Automates 12 of the 29 vendored undisbeliever/snes-test-roms hardware
//! ROMs (`roms/snes/automated_tests/undisbeliever_snes_test_roms/`) that
//! visually match Mesen2.
//!
//! Unlike blargg/gilyon ROMs, these do not print a PASS/FAIL text screen --
//! reading the upstream source (github.com/undisbeliever/snes-test-roms)
//! shows they are hardware-glitch demonstrations that the source comments
//! themselves describe as needing "a few console resets" to manifest, or
//! interactive demos driven by joypad input. There is no canonical correct
//! screen even on real hardware. Each golden here is a **stability
//! snapshot**: the ROM's default (no-input) rendering, cross-checked against
//! a Mesen2 capture of the identical ROM file at the same frame (using
//! `--Video.VideoFilter=None --Video.AspectRatio=NoStretching` to get a
//! comparable capture, and allowing for a harmless constant 1-scanline row
//! offset between the two emulators' screenshot conventions) -- not proof
//! of hardware accuracy.
//!
//! The other 17 ROMs are deliberately left un-automated: cross-checking
//! against Mesen2 exposed real NESER divergences, tracked as follow-up bugs
//! rather than papered over with a golden that bakes in known-wrong
//! behavior (see README-SNES.md for the full breakdown):
//! - `hdmaen_latch_test(_2).sfc`, `inidisp_brightness_delay.sfc`,
//!   `hdma-2100-glitch-2ch-{0a,81}.sfc`, `hdma-21ff-2100-glitch.sfc` -- #2943
//! - `inidisp_forgot_to_force_blank.sfc` -- #2944
//! - all 10 `scpu-a-dma-bug-*.sfc` -- #2945

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const UNDISBELIEVER_ROOT: &str = "roms/snes/automated_tests/undisbeliever_snes_test_roms";

#[cfg(test)]
mod tests {
    use super::*;

    /// All 11 ROMs here settle into their steady-state rendering well
    /// before frame 600 (matching the default budget used throughout
    /// blargg_apu_tests.rs / gilyon_*_tests.rs) and hold it indefinitely.
    ///
    /// Deliberately does not use `rom_runner::assert_rom_screen_crc`: its
    /// panic message says "expected screen-CRC PASS", which would be
    /// misleading here -- these ROMs have no PASS/FAIL concept, so a
    /// mismatch means the stability snapshot changed, not that a "test"
    /// failed.
    fn run_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(UNDISBELIEVER_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {frames} no longer matches the \
             Mesen2-cross-checked stability-snapshot CRC (got 0x{:08X}); if this \
             is an intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! undisbeliever_rom_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_screen_crc($file, 600, $crc);
            }
        };
    }

    undisbeliever_rom_test!(
        hdma_2100_glitch_matches_mesen2,
        "hdma-2100-glitch.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        hdma_21ff_2100_0f_glitch_matches_mesen2,
        "hdma-21ff-2100-0f-glitch.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        hdma_21ff_glitch_matches_mesen2,
        "hdma-21ff-glitch.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_d7_glitch_test_matches_mesen2,
        "inidisp_d7_glitch_test.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f_matches_mesen2,
        "inidisp_hammer_0f.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f00_matches_mesen2,
        "inidisp_hammer_0f00.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f0f_matches_mesen2,
        "inidisp_hammer_0f0f.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f8f_matches_mesen2,
        "inidisp_hammer_0f8f.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f8f_fast_matches_mesen2,
        "inidisp_hammer_0f8f_fast.sfc",
        0x4844_ECF2
    );

    undisbeliever_rom_test!(
        inidisp_hammer_0f_long_matches_mesen2,
        "inidisp_hammer_0f_long.sfc",
        0x4844_ECF2
    );

    // The only exact byte-for-byte match against Mesen2 (both stay in
    // forced-blank almost the entire time, alternating INIDISP $8F/$0F).
    undisbeliever_rom_test!(
        inidisp_hammer_8f0f_matches_mesen2,
        "inidisp_hammer_8f0f.sfc",
        0x6E8D_8520
    );

    // Fixed by the per-scanline INIDISP latch (was previously left un-automated
    // under #2944): the top of the frame is force-blanked (black), then the
    // display is enabled partway down, matching Mesen2 exactly.
    undisbeliever_rom_test!(
        inidisp_enable_display_mid_frame_matches_mesen2,
        "inidisp_enable_display_mid_frame.sfc",
        0x3B0F_939D
    );
}
