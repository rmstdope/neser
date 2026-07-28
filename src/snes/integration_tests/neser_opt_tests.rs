//! Automates the NESER-authored offset-per-tile (BG modes 2/4/6) test
//! ROMs (`roms/snes/automated_tests/snes_test_roms/neser-opt-tests/`),
//! written for issue #2881 against undisbeliever's bass framework
//! (sources live in the same directory). No redistributable
//! third-party ROM covering offset-per-tile could be found, so these
//! were authored for NESER.
//!
//! All ROMs render a static solid-colour-tile scene whose BG3 tilemap
//! is the offset map (row 0 = horizontal entries, row 1 = vertical
//! entries). They cover: growing vertical offsets with per-entry BG1
//! apply-flag gating (plus the OPT-exempt leftmost column and the
//! entry-j-to-column-j+1 mapping), horizontal offsets with flag
//! gating, the ignored low 3 bits of horizontal entries plus the
//! BG1HOFS fine scroll being retained, the BG1/BG2 apply-flag
//! selection over a two-layer scene, mode 4's single offset row with
//! per-entry bit-15 H/V selection (with conspicuous flag-less filler
//! in the never-read second row), and mode 6.
//!
//! Baseline results (#2878 settle-probe workflow): every scene settles
//! by frame 8 and is sampled at frame 68. All six ROMs match Mesen2
//! headless captures pixel-exactly at the identical frame (approved
//! goldens). `opt-m6.sfc` joined them with the #3016 hires rework
//! (doubled fetch with 16-wide char pairing, hires-domain OPT, CGRAM-0
//! sub backdrop): 0 of 114,688 px differ after normalizing Mesen2's
//! row-doubled height.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const OPT_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-opt-tests";

/// All scenes settle by frame 8; sampled with a 60-frame margin.
const SAMPLE_FRAME: u32 = 68;

#[cfg(test)]
mod tests {
    use super::*;

    fn run_opt_screen_crc(file: &str, expected_crc: u32) {
        let path = Path::new(OPT_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "neser_opt_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {SAMPLE_FRAME} no longer matches \
             the approved golden CRC (got 0x{:08X}); if this is an intentional \
             rendering change, re-approve the golden per README-SNES.md",
            result.screen_crc32
        );
    }

    macro_rules! neser_opt_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_opt_screen_crc($file, $crc);
            }
        };
    }

    neser_opt_test!(opt_m2_bg1_v, "opt-m2-bg1-v.sfc", 0xA0E6_CD76);
    neser_opt_test!(opt_m2_bg1_h, "opt-m2-bg1-h.sfc", 0x6918_9AEC);
    neser_opt_test!(opt_m2_fine_hofs, "opt-m2-fine-hofs.sfc", 0x9983_4588);
    neser_opt_test!(opt_m2_bg2_select, "opt-m2-bg2-select.sfc", 0x8935_F6DC);
    neser_opt_test!(opt_m4, "opt-m4.sfc", 0x24C0_14F0);

    neser_opt_test!(opt_m6, "opt-m6.sfc", 0x7F6E_FE0F);
}
