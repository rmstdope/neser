//! Automates the NESER-authored PPU OBJ feature test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/neser-obj-tests/`), written
//! for issue #2879 against undisbeliever's bass framework (sources live in
//! the same directory). Static one-screen scenes covering the OBJ features
//! not reachable by the vendored upstream ROMs: all eight OBSEL size pairs
//! (small + large side by side), OBJ-vs-OBJ priority (OAM index order,
//! unaffected by the OAM priority bits), OBJ palette selection, the OAM
//! high-table X bit 8, OBJ-vs-BG priority interaction in mode 1, OAMADDH
//! first-sprite priority rotation, and OBJ vertical wrap-around.
//!
//! Every golden was approved via the #2878/#2879 baseline workflow: all 14
//! ROMs settle at frame 6 (probed to 900), are sampled at settle + 60
//! (frame 66), and were pixel-diffed against Mesen2 headless captures at
//! the same frame -- the 13 goldens below match exactly at shift (0,0).
//! `obj-y-wrap.sfc` matches for its control and unflipped wrapped sprites
//! but NESER renders the V-flipped wrapped sprite with wrong rows (issue
//! #3003), so that test is `#[ignore]`d with NESER's current CRC recorded.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const NESER_OBJ_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-obj-tests";

/// All scenes settle at frame 6 and are sampled at settle + 60.
const SAMPLE_FRAME: u32 = 66;

#[cfg(test)]
mod tests {
    use super::*;

    /// Like the other visual PPU suites, a mismatch here means the approved
    /// golden screen changed, not that the ROM itself reported a failure.
    fn run_neser_obj_screen_crc(file: &str, expected_crc: u32) {
        let path = Path::new(NESER_OBJ_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "neser_obj_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {SAMPLE_FRAME} no longer \
             matches the Mesen2-approved golden CRC (got 0x{:08X}); if this \
             is an intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! neser_obj_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_neser_obj_screen_crc($file, $crc);
            }
        };
    }

    // One small + one large sprite per OBSEL size select, including the
    // undocumented rectangular pairs 6 (16x32/32x64) and 7 (16x32/32x32).
    neser_obj_test!(size_grid_0, "obj-size-grid-0.sfc", 0x7E11_1AF2);
    neser_obj_test!(size_grid_1, "obj-size-grid-1.sfc", 0xC1AD_0E2B);
    neser_obj_test!(size_grid_2, "obj-size-grid-2.sfc", 0xF0D1_88F2);
    neser_obj_test!(size_grid_3, "obj-size-grid-3.sfc", 0xE045_FA33);
    neser_obj_test!(size_grid_4, "obj-size-grid-4.sfc", 0xD139_7CEA);
    neser_obj_test!(size_grid_5, "obj-size-grid-5.sfc", 0x43DE_31F4);
    neser_obj_test!(size_grid_6, "obj-size-grid-6.sfc", 0x140F_CBCB);
    neser_obj_test!(size_grid_7, "obj-size-grid-7.sfc", 0x2D73_A4CB);

    // Same glyphs through all eight OBJ palettes (CGRAM 128 + 16*p).
    neser_obj_test!(palettes, "obj-palettes.sfc", 0xA6D2_0070);

    // Overlapping sprites layer by OAM index (lower index in front), even
    // when the back sprite carries higher OAM priority bits.
    neser_obj_test!(obj_vs_obj_priority, "obj-priority.sfc", 0x53B2_4C7B);

    // OAM high-table X bit 8: right-edge clip, negative X (left clip) and
    // X = 256 (fully off-screen).
    neser_obj_test!(oam_x_bit8, "oam-x8.sfc", 0xC625_82AF);

    // Mode 1 layering: OBJ3 > BG1 pri-1 > OBJ2 > BG1 pri-0 > OBJ1/OBJ0.
    neser_obj_test!(obj_vs_bg_priority, "obj-bg-priority.sfc", 0x0314_5C50);

    // OAMADDH bit 7 priority rotation: evaluation starts at the sprite
    // selected by OAMADD, flipping which of two overlapping sprites wins.
    neser_obj_test!(
        first_sprite_rotation,
        "first-sprite-rotation.sfc",
        0x0D78_01A1
    );

    /// Control and unflipped wrapped sprites match Mesen2; the V-flipped
    /// wrapped sprite renders wrong rows in NESER, so this CRC is NESER's
    /// own output from when #3003 was filed, NOT a Mesen2-approved golden.
    /// Re-probe and re-approve once #3003 is fixed.
    #[test]
    #[ignore = "V-flipped OBJ renders wrong rows when wrapping past Y=255; pending #3003"]
    fn obj_y_wrap() {
        run_neser_obj_screen_crc("obj-y-wrap.sfc", 0xBDA5_8124);
    }
}
