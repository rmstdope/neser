//! Automates the six vendored undisbeliever Mode 7 test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-mode7/`,
//! built from the source mirror -- see that folder's README) covering
//! the interleaved Mode 7 VRAM layout written through every VMAIN
//! increment/remapping path plus the Mode 7 tilemap column/row update
//! demos (issue #2881).
//!
//! Baseline results (#2878/#2880 workflow): the four static
//! `vmain-mode7-image-*` demos settle by frame 20 and are sampled at
//! settle + 60; all four match Mesen2 headless captures pixel-exactly
//! at identical frames, and by demo design all four render the same
//! screen (shared golden CRC), pinning VMAIN low-byte-only writes,
//! tilemap-then-tiles uploads and 8/10-bit address remapping to one
//! reference image. The two animated `vmain-mode7-tilemap-*` demos
//! never settle; their goldens are pinned at frames 120, 360 and 600,
//! each frame verified pixel-identical to a frame-skip-free Mesen2
//! capture (a phase regression cannot slip past three spread samples).

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const UNDISBELIEVER_PPU_MODE7_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-mode7";

#[cfg(test)]
mod tests {
    use super::*;

    fn run_mode7_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(UNDISBELIEVER_PPU_MODE7_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "undisbeliever_ppu_mode7_tests",
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

    macro_rules! undisbeliever_ppu_mode7_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_mode7_screen_crc($file, $frames, $crc);
            }
        };
    }

    // The four static demos draw the same image through different VMAIN
    // paths; the shared CRC is by design.
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_image_no_remapping,
        "vmain-mode7-image-no-remapping.sfc",
        80,
        0x01C0_A94F
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_image_tilemap,
        "vmain-mode7-image-tilemap.sfc",
        78,
        0x01C0_A94F
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_image_with_8bit_remapping,
        "vmain-mode7-image-with-8bit-remapping.sfc",
        77,
        0x01C0_A94F
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_image_with_10bit_remapping,
        "vmain-mode7-image-with-10bit-remapping.sfc",
        77,
        0x01C0_A94F
    );

    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_columns_f120,
        "vmain-mode7-tilemap-columns.sfc",
        120,
        0x3E99_C9A3
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_columns_f360,
        "vmain-mode7-tilemap-columns.sfc",
        360,
        0x548F_9743
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_columns_f600,
        "vmain-mode7-tilemap-columns.sfc",
        600,
        0xCED9_9433
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_rows_f120,
        "vmain-mode7-tilemap-rows.sfc",
        120,
        0x2B58_4F59
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_rows_f360,
        "vmain-mode7-tilemap-rows.sfc",
        360,
        0xA41A_4CC7
    );
    undisbeliever_ppu_mode7_test!(
        vmain_mode7_tilemap_rows_f600,
        "vmain-mode7-tilemap-rows.sfc",
        600,
        0x0ED7_C208
    );
}
