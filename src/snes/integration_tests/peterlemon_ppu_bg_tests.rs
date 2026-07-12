//! Automates 11 of the 12 vendored PeterLemon (krom) basic PPU BG demos
//! (`roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-{BGMAP,GreenSpace,Rings}/`)
//! covering BG tile decoding at 2/4/8bpp, BG1-BG4 layer selection, all four
//! tilemap screen sizes, tilemap H/V flip attributes, backdrop/CGRAM setup and
//! palette loads (issue #2878).
//!
//! Every golden below was approved via the #2878 baseline workflow: probed to
//! its settle frame (screen CRC unchanged for >= 600 consecutive frames),
//! sampled at settle + 60, and pixel-diffed against a Mesen2 headless capture
//! of the identical ROM at the same frame (`--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching`) -- all 11 match Mesen2 exactly, and the
//! 8 demos that ship an upstream reference screenshot also match that PNG
//! pixel-for-pixel.
//!
//! Shared CRCs are genuine: the BG2/BG3/BG4 2bpp demos render identical
//! screens (each verified against its own upstream reference), and the
//! 32x64/64x32/64x64 8bpp demos show the same 256x224 viewport of the same
//! art regardless of tilemap size.
//!
//! `8x8BGMap8BPP32x32.sfc` is deliberately left un-automated: it scrolls its
//! map diagonally forever and NESER's animation phase drifts from Mesen2
//! (content matches at a (30,30) px wrap shift; Mesen2 advances the scroll
//! every second frame while NESER advances every frame) -- tracked as #2990.

use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const PETERLEMON_PPU_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/PeterLemon";

#[cfg(test)]
mod tests {
    use super::*;

    /// The demos settle within ~20 frames (forced-blank init, one-shot DMA
    /// upload, then a static screen forever); each sampled frame is the probed
    /// settle frame plus a 60-frame margin. The 400M tick budget matches the
    /// other screen-CRC suites.
    fn run_ppu_bg_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        assert_rom_screen_crc(
            PETERLEMON_PPU_ROOT,
            file,
            "peterlemon_ppu_bg_tests",
            frames,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    macro_rules! peterlemon_ppu_bg_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_ppu_bg_screen_crc($file, $frames, $crc);
            }
        };
    }

    peterlemon_ppu_bg_test!(
        bg1_map_2bpp_32x32_8pal,
        "SNES-PPU-BGMAP/8x8/2BPP/8x8BG1Map2BPP32x328PAL/8x8BG1Map2BPP32x328PAL.sfc",
        79,
        0xE565_98AD
    );

    peterlemon_ppu_bg_test!(
        bg2_map_2bpp_32x32_8pal,
        "SNES-PPU-BGMAP/8x8/2BPP/8x8BG2Map2BPP32x328PAL/8x8BG2Map2BPP32x328PAL.sfc",
        79,
        0x0F5A_62FD
    );

    peterlemon_ppu_bg_test!(
        bg3_map_2bpp_32x32_8pal,
        "SNES-PPU-BGMAP/8x8/2BPP/8x8BG3Map2BPP32x328PAL/8x8BG3Map2BPP32x328PAL.sfc",
        79,
        0x0F5A_62FD
    );

    peterlemon_ppu_bg_test!(
        bg4_map_2bpp_32x32_8pal,
        "SNES-PPU-BGMAP/8x8/2BPP/8x8BG4Map2BPP32x328PAL/8x8BG4Map2BPP32x328PAL.sfc",
        79,
        0x0F5A_62FD
    );

    peterlemon_ppu_bg_test!(
        bg_map_4bpp_32x32_8pal,
        "SNES-PPU-BGMAP/8x8/4BPP/8x8BGMap4BPP32x328PAL/8x8BGMap4BPP32x328PAL.sfc",
        80,
        0x018F_0F81
    );

    peterlemon_ppu_bg_test!(
        bg_map_8bpp_32x64,
        "SNES-PPU-BGMAP/8x8/8BPP/32x64/8x8BGMap8BPP32x64.sfc",
        80,
        0x1C2F_0025
    );

    peterlemon_ppu_bg_test!(
        bg_map_8bpp_64x32,
        "SNES-PPU-BGMAP/8x8/8BPP/64x32/8x8BGMap8BPP64x32.sfc",
        80,
        0x1C2F_0025
    );

    peterlemon_ppu_bg_test!(
        bg_map_8bpp_64x64,
        "SNES-PPU-BGMAP/8x8/8BPP/64x64/8x8BGMap8BPP64x64.sfc",
        80,
        0x1C2F_0025
    );

    peterlemon_ppu_bg_test!(
        bg_map_tile_flip,
        "SNES-PPU-BGMAP/8x8/8BPP/TileFlip/8x8BGMapTileFlip.sfc",
        79,
        0xCE83_E2E6
    );

    peterlemon_ppu_bg_test!(
        green_space_backdrop,
        "SNES-PPU-GreenSpace/GreenSpace.sfc",
        65,
        0x9E6A_0E5A
    );

    peterlemon_ppu_bg_test!(rings_palette, "SNES-PPU-Rings/Rings.sfc", 80, 0x25A4_54DB);
}
