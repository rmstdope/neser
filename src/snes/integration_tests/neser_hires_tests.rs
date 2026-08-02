//! Automates the NESER-authored mid-frame hires-transition ROMs
//! (`roms/snes/automated_tests/snes_test_roms/neser-hires-tests/`),
//! written for issue #3034 against undisbeliever's bass framework
//! (sources live in the same directory). No vendored third-party ROM
//! switches hires part-way down a frame -- every hires ROM in the
//! collection (MosaicMode5, the six Interlace demos, the four HiColor
//! demos, opt-m6) sets BGMODE/SETINI once during init, and ddribin's
//! hdrvtest toggles interlace only between frames -- so these were
//! authored for NESER.
//!
//! Both ROMs render the same scene (BG1 vertical stripes on the main
//! screen, BG2 horizontal bands on the sub screen) and use one HDMA
//! channel to turn a hires mode on at display line 100: one writes
//! BGMODE ($2105) 1 -> 5 per scanline, the other SETINI ($2133) bit 3
//! for pseudo-hires. The frame therefore begins native and ends hires,
//! which is exactly the transition #3016 deferred to #3034.
//!
//! Baseline results (#2878/#2880 workflow; Mesen2 headless captures at
//! the identical frame): both match Mesen2 pixel-exactly at the native
//! 512x448 -- 0 of 229,376 px differ, with no normalization of any
//! kind. Each vector also asserts the structure the golden encodes, so
//! a future regression says *what* broke rather than only that a CRC
//! moved: output rows 0-199 (the 100 lines drawn before the switch)
//! must be column-doubled, rows 200-447 must carry true half-pixel
//! pairs, and every row pair must be identical. That is one uniform
//! 512-column frame whose pre-switch rows were re-laid-out
//! retroactively -- not a frame with 256-column rows on top and
//! 512-column rows below.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const HIRES_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-hires-tests";

/// The scene is static and settles within a few frames; sampled with a
/// wide margin, matching the other NESER-authored suites.
const SAMPLE_FRAME: u32 = 68;

/// Display line at which both ROMs' HDMA tables switch to a hires mode
/// (`HIRES_SWITCH_LINE` in `src/_hires-scene.inc`).
const SWITCH_LINE: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a ROM to [`SAMPLE_FRAME`], assert its golden CRC, and assert the
    /// mid-frame-transition structure that CRC stands for.
    fn run_hires_vector(file: &str, expected_crc: u32) {
        let path = Path::new(HIRES_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "neser_hires_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        );
        // The structural checks run first on purpose. A golden CRC only ever says
        // "these bytes changed"; asking what the frame actually looks like first
        // means a regression reports which row broke and how.
        assert_eq!(
            result.screen_dimensions,
            (512, 448),
            "{file}: a frame that turns hires part-way down is a hires frame whole"
        );
        let rgb = result
            .screen_rgb
            .as_deref()
            .expect("the ScreenCrc oracle reached its target frame");
        assert_converted_above_and_hires_below(file, rgb);

        assert_eq!(
            result.screen_crc32, expected_crc,
            "{file}: rendered screen at frame {SAMPLE_FRAME} no longer matches \
             the approved golden CRC (got 0x{:08X}); if this is an intentional \
             rendering change, re-approve the golden per README-SNES.md",
            result.screen_crc32
        );
    }

    /// The rows drawn before the switch must be column-doubled (they were
    /// converted in place), the rows after it must carry real half-pixel pairs,
    /// and the whole frame must be row-doubled.
    fn assert_converted_above_and_hires_below(file: &str, rgb: &[u8]) {
        const WIDTH: usize = 512;
        const HEIGHT: usize = 448;
        assert_eq!(rgb.len(), WIDTH * HEIGHT * 3, "{file}: 512x448 output");

        let px = |x: usize, y: usize| {
            let i = (y * WIDTH + x) * 3;
            [rgb[i], rgb[i + 1], rgb[i + 2]]
        };
        let column_doubled = |y: usize| (0..WIDTH).step_by(2).all(|x| px(x, y) == px(x + 1, y));

        for y in 0..(SWITCH_LINE * 2) {
            assert!(
                column_doubled(y),
                "{file}: output row {y} was drawn before the switch and must be \
                 column-doubled -- if it is not, the pre-switch rows were left in \
                 the narrow layout"
            );
        }
        assert!(
            ((SWITCH_LINE * 2)..HEIGHT).any(|y| !column_doubled(y)),
            "{file}: no row below the switch carries half-pixel pairs, so the \
             column-doubling check above proves nothing"
        );
        for y in (0..HEIGHT).step_by(2) {
            assert!(
                (0..WIDTH).all(|x| px(x, y) == px(x, y + 1)),
                "{file}: output rows {y}/{} must be duplicates -- a progressive \
                 hires frame fills both rows of every display line",
                y + 1
            );
        }
    }

    #[test]
    fn hires_hdma_bgmode() {
        run_hires_vector("hires-hdma-bgmode.sfc", 0x4A88_7677);
    }

    #[test]
    fn hires_hdma_setini() {
        run_hires_vector("hires-hdma-setini.sfc", 0x0CDF_029C);
    }
}
