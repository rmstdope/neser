//! Automates 1 of the 2 vendored 93143 hblank-dma-vram ROMs
//! (`roms/snes/automated_tests/snes_test_roms/93143-hblank-dma-vram/`), mirrored
//! from a NESdev forum post by user "93143"
//! (<https://forums.nesdev.com/viewtopic.php?p=248408#p248408>) into the
//! `rmstdope/snes-test-roms` fork of higan's bundled test ROMs.
//!
//! Neither ROM prints a PASS/FAIL text screen -- like the undisbeliever suite,
//! they are purely visual, so automation here uses a screen-CRC golden
//! cross-checked against a Mesen2 capture of the identical ROM file (same
//! methodology as `undisbeliever_tests.rs`: `--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching`, allowing for the harmless constant
//! ±1-scanline row offset between the two emulators' screenshot conventions).
//!
//! **Automated (1 of 2): `hvdma.sfc`.** Uses all 8 HDMA channels to, on two
//! scanlines during the frame, force-blank the display, write a VRAM address,
//! burst 20 bytes of tile data into VRAM, and unblank -- so the tile pattern
//! BG1 draws changes partway down the screen. The golden below was captured
//! after fixing #2952 (NESER's HDMA transfer mode 5 -- "2 registers, written
//! twice each": 4 bytes/line -- was reusing the general-purpose DMA
//! controller's cyclic-transfer mode simplification, which collapsed it to
//! mode 1's 2-byte pattern; this silently dropped 2 of every 4 table bytes
//! HDMA read per line and desynced the per-channel table pointer, so all 5 of
//! this ROM's VMDATA channels mistook a leftover data byte for the table
//! terminator right after their first line and never performed the ROM's
//! mid-frame VRAM update). After the fix, NESER's frame-600 capture matches a
//! Mesen2 capture of the same ROM file to within 0.66% (best ±1-row
//! alignment) -- and the bundled real-hardware reference photo
//! (`93143-hblank-dma-vram/expected-output.jpg`) confirms this two-region
//! tile pattern is expected, reliable real-hardware behavior (not a
//! bus-residual coin-flip), so this golden is a genuine hardware-accuracy
//! claim, not just a shared-limitation match.
//!
//! **Deliberately un-automated (1 of 2): `hvdma_max.sfc`.** Triggers a DMA
//! from inside an H-IRQ (cycle-counted jump table) to measure how long a VRAM
//! burst can run before force-blank release becomes visibly late. NESER
//! renders a solid black screen where Mesen2 renders the documented solid
//! green (100% pixel diff, stable across frames 600-610) -- a real,
//! unrelated rendering gap (the #2952 fix above does not change this ROM's
//! output at all), tracked as #2953.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const HBLANK_DMA_VRAM_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/93143-hblank-dma-vram";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a vendored 93143 hblank-dma-vram ROM and asserts that the screen
    /// rendered at `frames` matches the visually-approved, Mesen2-cross-checked
    /// golden CRC32.
    ///
    /// To approve a new golden, run with NESER_CAPTURE_SCREEN=1, cross-check the
    /// capture under target/snes_test_captures/ against a Mesen2 capture of the
    /// same ROM/frame (pixel-diff, not eyeballing -- see README-SNES.md), then
    /// record the (frame, CRC) here.
    fn run_screen_crc(file: &str, frames: u32, expected_crc: u32, config: RunConfig) {
        let path = Path::new(HBLANK_DMA_VRAM_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "hblank_dma_vram_tests",
            config,
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: rendered screen at frame {frames} no longer matches the \
             Mesen2-cross-checked golden CRC (got 0x{:08X}); if this is an \
             intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! hblank_dma_vram_rom_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            hblank_dma_vram_rom_test!($name, $file, $frames, $crc, RunConfig::new(400_000_000, 0));
        };
        ($name:ident, $file:expr, $frames:expr, $crc:expr, $config:expr) => {
            #[test]
            fn $name() {
                run_screen_crc($file, $frames, $crc, $config);
            }
        };
    }

    // Confirmed hardware-accurate: the source describes this two-region tile
    // pattern as reliable, expected real-hardware behavior, and NESER now
    // matches both Mesen2 and the bundled real-hardware reference photo. See
    // #2952.
    hblank_dma_vram_rom_test!(
        hvdma_matches_mesen2_and_hardware,
        "hvdma.sfc",
        600,
        0x4EE7_C1E5
    );
}
