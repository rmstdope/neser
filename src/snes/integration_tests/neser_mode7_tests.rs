//! Automates the NESER-authored Mode 7 test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/neser-mode7-tests/`),
//! written for issue #2881 against undisbeliever's bass framework
//! (sources live in the same directory).
//!
//! All ROMs render one static 1024x1024 px Mode 7 scene (white border
//! ring, coloured 128px corner blocks, magenta centre cross,
//! cyan/dark-grey checkerboard; tile 0 is a solid orange fill marker
//! and the backdrop is dark blue) with a matrix written once during
//! init. They cover the identity baseline, M7SEL out-of-screen wrap /
//! colour-0 / tile-0-fill at an 8x zoom-out, a 30-degree rotation
//! about the map centre, both M7SEL screen flips, and Mode 7 mosaic
//! (MOSAIC size 16 on BG1).
//!
//! Baseline results (#2878 settle-probe workflow): every scene settles
//! by frame 8 and is sampled at frame 68; all 8 captures match Mesen2
//! headless captures at the identical frame pixel-exactly
//! (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`), so all 8 carry approved
//! goldens.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const MODE7_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-mode7-tests";

/// All scenes settle by frame 8; sampled with a 60-frame margin.
const SAMPLE_FRAME: u32 = 68;

#[cfg(test)]
mod tests {
    use super::*;

    fn run_mode7_screen_crc(file: &str, expected_crc: u32) {
        let path = Path::new(MODE7_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "neser_mode7_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {SAMPLE_FRAME} no longer matches \
             the Mesen2-approved golden CRC (got 0x{:08X}); if this is an \
             intentional rendering change, re-approve the golden per \
             README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! neser_mode7_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_mode7_screen_crc($file, $crc);
            }
        };
    }

    neser_mode7_test!(m7_identity, "m7-identity.sfc", 0x7EDC_DD3D);
    neser_mode7_test!(m7_scale_wrap, "m7-scale-wrap.sfc", 0xB431_6EA2);
    neser_mode7_test!(m7_scale_color0, "m7-scale-color0.sfc", 0xE5AB_774A);
    neser_mode7_test!(m7_scale_tile0, "m7-scale-tile0.sfc", 0x5D47_8D7E);
    neser_mode7_test!(m7_rot30, "m7-rot30.sfc", 0x9A58_AB93);
    neser_mode7_test!(m7_flip_h, "m7-flip-h.sfc", 0x3C92_8CE7);
    neser_mode7_test!(m7_flip_v, "m7-flip-v.sfc", 0x7DB8_DCC0);
    neser_mode7_test!(m7_mosaic, "m7-mosaic.sfc", 0x27C9_C012);
}
