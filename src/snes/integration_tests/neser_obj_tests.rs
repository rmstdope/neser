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
//! (frame 66). Thirteen were pixel-diffed against Mesen2 headless captures
//! at the same frame and match exactly at shift (0,0).
//!
//! `obj-y-wrap.sfc` is the exception (#3003). Mesen2 disagrees with ares,
//! ares-performance, higan and Snes9x on V-flip across an 8-bit Y wrap, so
//! its golden rests on that majority plus the SNESdev wiki rather than on a
//! Mesen2 cross-check, and the residual 40-pixel Mesen2 diff is confined
//! entirely to the flipped band with zero pixels outside it. Because no
//! reference capture can arbitrate it, the suite also carries a
//! **golden-independent structural assertion** that derives the expected
//! wrapped rows from the ROM's own output -- see
//! `obj_y_wrap_vflipped_sprite_mirrors_the_unflipped_wrapped_sprite`.

use super::rom_runner::{RunConfig, RunOracle, RunResult, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const NESER_OBJ_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/neser-obj-tests";

/// All scenes settle at frame 6 and are sampled at settle + 60.
const SAMPLE_FRAME: u32 = 66;

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a suite ROM to [`SAMPLE_FRAME`] and return the full result, including the
    /// sampled frame's pixels.
    fn run_neser_obj(file: &str, expected_crc: u32) -> RunResult {
        let path = Path::new(NESER_OBJ_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        run_rom_with_oracle(
            &rom,
            file,
            "neser_obj_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: SAMPLE_FRAME,
                expected_crc,
            },
        )
    }

    /// Like the other visual PPU suites, a mismatch here means the approved
    /// golden screen changed, not that the ROM itself reported a failure.
    fn run_neser_obj_screen_crc(file: &str, expected_crc: u32) {
        let result = run_neser_obj(file, expected_crc);
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

    /// `obj-y-wrap.sfc` renders three 16x32 sprites (OBSEL size select 6): a fully
    /// visible control at (64,64), an unflipped one at (128,240) whose bottom half wraps
    /// to screen lines 0-15, and a V-flipped one at (192,240) that wraps the same way.
    const OBJ_Y_WRAP_CRC: u32 = 0x8EA2_EF1E;

    /// One 16-px-wide sprite row of the capture as a lit/unlit mask.
    ///
    /// The two wrapped sprites are drawn through different OBJ palettes, so their colours
    /// differ; but the ROM's `hex8` glyph tiles use only colour indices 1-2 and index 0 is
    /// the black backdrop, so "not black" is exactly "opaque OBJ pixel" for both and the
    /// masks are directly comparable.
    fn lit_row(rgb: &[u8], x: usize, y: usize) -> [bool; 16] {
        std::array::from_fn(|dx| {
            let i = ((y * 256) + x + dx) * 3;
            rgb[i..i + 3] != [0, 0, 0]
        })
    }

    /// Golden-independent check that the wrapped V-flipped sprite mirrors the wrapped
    /// unflipped one (#3003).
    ///
    /// This vector has no reference capture to diff against: Mesen2 disagrees with ares,
    /// ares-performance, higan and Snes9x here (see the comment on the V-flip mirror in
    /// `sprites.rs`), so its CRC golden rests on that four-implementation majority rather
    /// than on a cross-emulator pixel diff. This test is the substitute oracle, and it
    /// depends on no emulator at all -- it derives the expectation from the ROM's own
    /// output.
    ///
    /// Both wrapped sprites are the same 16x32 sprite at the same Y, differing only in the
    /// V-flip bit, and the visible window is exactly the sprite's lower 16x16 square half.
    /// Screen row `L` of the unflipped sprite shows source line `16 + L`; the flipped one
    /// shows `(16 + L) ^ 15 = 16 + (15 - L)`. So `flipped(L)` must equal `unflipped(15 - L)`
    /// -- which IS the "rectangular OBJs flip as two stacked squares" claim, stated without
    /// reference to absolute screen rows or to the OAM-Y-to-framebuffer-row convention.
    #[test]
    fn obj_y_wrap_vflipped_sprite_mirrors_the_unflipped_wrapped_sprite() {
        let result = run_neser_obj("obj-y-wrap.sfc", OBJ_Y_WRAP_CRC);
        let rgb = result
            .screen_rgb
            .expect("a ScreenCrc run that reached its frame captures the pixels");
        assert_eq!(rgb.len(), 256 * 224 * 3);

        // Non-triviality: the unflipped wrapped band must actually be drawn, or the
        // mirror assertion below would hold vacuously over two blank bands.
        assert!(
            (0..16).any(|row| lit_row(&rgb, 128, row).iter().any(|&lit| lit)),
            "the unflipped wrapped sprite must render on lines 0-15"
        );
        // And the flip must not be a no-op: every hex glyph's top row is blank, so the
        // unflipped band's row 0 is empty while the flipped band's is not.
        assert_ne!(
            lit_row(&rgb, 192, 0),
            lit_row(&rgb, 128, 0),
            "the V-flip must change what lands on line 0"
        );

        for row in 0..16usize {
            assert_eq!(
                lit_row(&rgb, 192, row),
                lit_row(&rgb, 128, 15 - row),
                "V-flipped wrapped row {row} must mirror unflipped wrapped row {}",
                15 - row
            );
        }
    }

    /// Re-approved in #3003. NOT a Mesen2 cross-check: Mesen2 selects tile rows 15/14 in
    /// the flipped band where ares, ares-performance, higan and Snes9x all select 3/2, and
    /// NESER follows those four plus the SNESdev wiki. The flipped band's self-labelling
    /// glyphs read `30/31` above `20/21`, confirmed empirically against a real ares
    /// screenshot of this ROM: ares renders the same glyph shapes, and the mirror relation
    /// the test above asserts holds on ares' OWN output (its unflipped and V-flipped
    /// wrapped bands have equal lit-pixel counts, 196 vs 196) while failing on Mesen2's
    /// (98 vs 90). The remaining 40-pixel diff against a fresh Mesen2 capture is entirely
    /// inside that band (x 192-207, lines 0-15) with zero pixels outside it, and the
    /// structural test pins the semantics independently of this CRC.
    neser_obj_test!(obj_y_wrap, "obj-y-wrap.sfc", OBJ_Y_WRAP_CRC);
}
