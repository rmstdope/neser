//! Automates the vendored PeterLemon (krom) advanced PPU mode ROMs
//! (`roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-Mode7/`,
//! `SNES-PPU-Mosaic/`, `SNES-PPU-Interlace/` and
//! `SNES-PPU-HDMA-HiColor64PerTileRowPseudoHiRes/`) covering Mode 7
//! (static rotozoom plus input-scripted rotation/zoom, HDMA
//! perspective, animated HDMA crawl), mosaic in modes 3 and 5, true
//! hires + interlace, and pseudo-hires (issue #2881).
//!
//! Baseline results (#2878/#2880 workflow; Mesen2 headless captures at
//! identical frames, input scripts replayed in Mesen2 via Lua
//! `emu.setInput` per the #2879 recipe):
//!
//! - `RotZoom.sfc` matches Mesen2 pixel-exactly: the static frame-67
//!   screen and both scripted variants (R held frames 120-159 rotates
//!   one angle step per polled frame; A held frames 120-179 zooms in)
//!   carry approved goldens, pinning the full-frame affine path.
//! - `MosaicMode3.sfc` matches pixel-exactly both untouched (mosaic
//!   size 0) and with R held frames 120-149 (a larger dialed size);
//!   approved goldens.
//! - `StarWars.sfc` (animated HDMA perspective crawl) matches
//!   pixel-exactly at frame 120 (approved golden) but drifts later
//!   (964 px at frame 360, 222 px at frame 600, no phase offset
//!   explains it): the f360/f600 vectors are `#[ignore]`d with NESER's
//!   current CRCs pending #3021.
//! - `Perspective.sfc` diverges in exactly the rightmost pixel column
//!   (53 px, all at x = 255): `#[ignore]`d pending #3020.
//! - The four pseudo-hires `HiColor*.sfc` ROMs match Mesen2
//!   pixel-exactly since the #3016 hires rework (sub-on-even
//!   interleave, per-dot hires color math): approved goldens after
//!   normalizing Mesen2's row-doubled height. The main demo and the
//!   mandrill TEST variant draw the same image, hence their shared CRC.
//! - `MosaicMode5.sfc` and all six `Interlace*.sfc` ROMs still diverge
//!   (line-doubled interlace fields instead of distinct odd/even
//!   fields, #3017; the #3016 hires column rendering itself is fixed):
//!   `#[ignore]`d with NESER's current CRCs, refreshed after #3016.
//!   MosaicMode5's CRC no longer equals InterlaceMoogle's: mode 5
//!   forces the mosaic collapse of half-pixel pairs even at size 0's
//!   block size 1, which InterlaceMoogle (no mosaic) doesn't share.

use super::rom_runner::{InputEvent, RunConfig, RunOracle, run_rom_with_oracle};
use crate::snes::input::SnesButton;
use std::fs;
use std::path::Path;

const PETERLEMON_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/PeterLemon";

#[cfg(test)]
mod tests {
    use super::*;

    fn run_advanced_screen_crc(
        file: &str,
        label: &str,
        script: &[InputEvent],
        frames: u32,
        expected_crc: u32,
    ) {
        let path = Path::new(PETERLEMON_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let name = if label.is_empty() {
            file.to_string()
        } else {
            let stem = Path::new(file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file);
            format!("{stem}-{label}.sfc")
        };
        let result = run_rom_with_oracle(
            &rom,
            &name,
            "peterlemon_ppu_advanced_tests",
            RunConfig::new(400_000_000, 0).with_input_script(script),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert_eq!(
            result.screen_crc32, expected_crc,
            "{name}: rendered screen at frame {frames} no longer matches the \
             approved golden CRC (got 0x{:08X}); if this is an intentional \
             rendering change, re-approve the golden per README-SNES.md",
            result.screen_crc32
        );
    }

    /// Hold `button` from `from` to `to` (exclusive).
    const fn hold(button: SnesButton, from: u32, to: u32) -> [InputEvent; 2] {
        [
            InputEvent {
                frame: from,
                button,
                pressed: true,
            },
            InputEvent {
                frame: to,
                button,
                pressed: false,
            },
        ]
    }

    macro_rules! peterlemon_advanced_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_advanced_screen_crc($file, "", &[], $frames, $crc);
            }
        };
    }

    // ---- Mode 7 ----

    peterlemon_advanced_test!(
        rotzoom,
        "SNES-PPU-Mode7/RotZoom/RotZoom.sfc",
        67,
        0x1067_5E9E
    );

    #[test]
    fn rotzoom_rotate() {
        run_advanced_screen_crc(
            "SNES-PPU-Mode7/RotZoom/RotZoom.sfc",
            "rotate",
            &hold(SnesButton::R, 120, 160),
            300,
            0x9297_9197,
        );
    }

    #[test]
    fn rotzoom_zoom() {
        run_advanced_screen_crc(
            "SNES-PPU-Mode7/RotZoom/RotZoom.sfc",
            "zoom",
            &hold(SnesButton::A, 120, 180),
            300,
            0x2EF4_9D89,
        );
    }

    peterlemon_advanced_test!(
        starwars_f120,
        "SNES-PPU-Mode7/StarWars/StarWars.sfc",
        120,
        0x534A_F4DD
    );

    /// NESER's current CRC, NOT a Mesen2-approved golden (crawl drift).
    #[test]
    #[ignore = "Mode 7 HDMA crawl drifts from Mesen2 after frame ~120; pending #3021"]
    fn starwars_f360() {
        run_advanced_screen_crc(
            "SNES-PPU-Mode7/StarWars/StarWars.sfc",
            "",
            &[],
            360,
            0x5A6E_5802,
        );
    }

    /// NESER's current CRC, NOT a Mesen2-approved golden (crawl drift).
    #[test]
    #[ignore = "Mode 7 HDMA crawl drifts from Mesen2 after frame ~120; pending #3021"]
    fn starwars_f600() {
        run_advanced_screen_crc(
            "SNES-PPU-Mode7/StarWars/StarWars.sfc",
            "",
            &[],
            600,
            0xF4B1_137A,
        );
    }

    /// NESER's current CRC, NOT a Mesen2-approved golden (rightmost
    /// column, 53 px at x = 255).
    #[test]
    #[ignore = "Mode 7 rightmost column diverges from Mesen2 in the HDMA-perspective scene; pending #3020"]
    fn perspective() {
        run_advanced_screen_crc(
            "SNES-PPU-Mode7/Perspective/Perspective.sfc",
            "",
            &[],
            67,
            0x91F5_F669,
        );
    }

    // ---- Mosaic ----

    peterlemon_advanced_test!(
        mosaic_mode3,
        "SNES-PPU-Mosaic/Mode3/MosaicMode3.sfc",
        80,
        0x4C8F_FCF7
    );

    #[test]
    fn mosaic_mode3_sized() {
        run_advanced_screen_crc(
            "SNES-PPU-Mosaic/Mode3/MosaicMode3.sfc",
            "sized",
            &hold(SnesButton::R, 120, 150),
            300,
            0x1F4F_403B,
        );
    }

    /// NESER's current CRC, NOT a Mesen2-approved golden.
    #[test]
    #[ignore = "interlace renders line-doubled fields instead of distinct odd/even fields; pending #3017"]
    fn mosaic_mode5() {
        run_advanced_screen_crc(
            "SNES-PPU-Mosaic/Mode5/MosaicMode5.sfc",
            "",
            &[],
            81,
            0x1105_5DF5,
        );
    }

    /// NESER's current CRC, NOT a Mesen2-approved golden.
    #[test]
    #[ignore = "interlace renders line-doubled fields instead of distinct odd/even fields; pending #3017"]
    fn mosaic_mode5_sized() {
        run_advanced_screen_crc(
            "SNES-PPU-Mosaic/Mode5/MosaicMode5.sfc",
            "sized",
            &hold(SnesButton::R, 120, 150),
            300,
            0x96A0_9C1C,
        );
    }

    // ---- Hires + interlace (all NESER-current CRCs, not goldens) ----

    macro_rules! peterlemon_interlace_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            /// NESER's current CRC, NOT a Mesen2-approved golden.
            #[test]
            #[ignore = "interlace renders line-doubled fields instead of distinct odd/even fields; pending #3017"]
            fn $name() {
                run_advanced_screen_crc($file, "", &[], $frames, $crc);
            }
        };
    }

    peterlemon_interlace_test!(
        interlace_font,
        "SNES-PPU-Interlace/InterlaceFont/InterlaceFont.sfc",
        79,
        0xB608_0965
    );
    peterlemon_interlace_test!(
        interlace_moogle,
        "SNES-PPU-Interlace/InterlaceMoogle/InterlaceMoogle.sfc",
        79,
        0xBDF9_FB2F
    );
    peterlemon_interlace_test!(
        interlace_myst_hdma,
        "SNES-PPU-Interlace/InterlaceMystHDMA/InterlaceMystHDMA.sfc",
        81,
        0xE0CC_4D17
    );
    peterlemon_interlace_test!(
        interlace_rpg,
        "SNES-PPU-Interlace/InterlaceRPG/InterlaceRPG.sfc",
        80,
        0xE09D_0A46
    );
    peterlemon_interlace_test!(
        interlace_scroll,
        "SNES-PPU-Interlace/InterlaceScroll/InterlaceScroll.sfc",
        80,
        0x566A_C2D7
    );
    peterlemon_interlace_test!(
        interlace_simpsons_hdma,
        "SNES-PPU-Interlace/InterlaceSimpsonsHDMA/InterlaceSimpsonsHDMA.sfc",
        80,
        0xE159_F195
    );

    // ---- Pseudo-hires (Mesen2-approved goldens since #3016) ----

    macro_rules! peterlemon_pseudohires_test {
        ($name:ident, $file:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_advanced_screen_crc($file, "", &[], 67, $crc);
            }
        };
    }

    // The main demo and the mandrill TEST variant draw the same image;
    // the shared CRC is by design.
    peterlemon_pseudohires_test!(
        hicolor_pseudohires,
        "SNES-PPU-HDMA-HiColor64PerTileRowPseudoHiRes/HiColor64PerTileRowPseudoHiRes.sfc",
        0xE0E1_0821
    );
    peterlemon_pseudohires_test!(
        hicolor_pseudohires_rgb_chart,
        "SNES-PPU-HDMA-HiColor64PerTileRowPseudoHiRes/TEST/RGB_24bits_palette_color_test_chart64PerTileRowHiRes.sfc",
        0xD0D0_E9FE
    );
    peterlemon_pseudohires_test!(
        hicolor_pseudohires_lenna,
        "SNES-PPU-HDMA-HiColor64PerTileRowPseudoHiRes/TEST/lenna64PerTileRowHiRes.sfc",
        0x2BC6_C82D
    );
    peterlemon_pseudohires_test!(
        hicolor_pseudohires_mandrill,
        "SNES-PPU-HDMA-HiColor64PerTileRowPseudoHiRes/TEST/mandrill64PerTileRowHiRes.sfc",
        0xE0E1_0821
    );
}
