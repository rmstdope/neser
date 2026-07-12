//! Automates 8 of the 18 vendored undisbeliever PPU BG / VMAIN test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg/`, built
//! from the source mirror -- see that folder's README) covering VRAM
//! increment modes at 1/2/4/8bpp, byte- vs word-increment uploads, plain
//! (non-DMA) VRAM data-port writes, 1bpp tile decode and a text tilemap
//! (issue #2878).
//!
//! Every golden was approved via the #2878 baseline workflow: probed to its
//! settle frame (screen CRC unchanged for >= 600 consecutive frames), sampled
//! at settle + 60, and pixel-diffed against a Mesen2 headless capture of the
//! identical ROM at the same frame (`--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching`) -- all 8 match Mesen2 exactly. The
//! shared CRC for `vmain-4bpp-no-remapping{,-word}.sfc` is by design: the
//! demos draw the same screen via byte- and word-increment writes.
//!
//! The other 10 vendored ROMs are deliberately left un-automated because
//! cross-checking exposed real NESER divergences, tracked as follow-up bugs
//! rather than papered over with known-wrong goldens:
//! - `vmain-{1bpp,2bpp,2bpp-split,4bpp,4bpp-word,8bpp}-with-remapping.sfc` --
//!   #2989 (VMAIN $2115 bits 2-3 address remapping is not implemented; the
//!   no-remapping twins below prove everything else in the upload path).
//! - `vmain-{horizontal,vertical,vertical-2-rows}-scrolling.sfc` and
//!   `textbuffer-hello-world.sfc` -- #2990 (animation frame-phase drifts from
//!   Mesen2 while static rendering matches).

use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const UNDISBELIEVER_PPU_BG_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg";

#[cfg(test)]
mod tests {
    use super::*;

    /// The demos settle within ~100 frames (init plus a tile-buffer upload
    /// spread over several frames, then a static screen forever); each
    /// sampled frame is the probed settle frame plus a 60-frame margin. The
    /// 400M tick budget matches the other screen-CRC suites.
    fn run_ppu_bg_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        assert_rom_screen_crc(
            UNDISBELIEVER_PPU_BG_ROOT,
            file,
            "undisbeliever_ppu_bg_tests",
            frames,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    macro_rules! undisbeliever_ppu_bg_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            #[test]
            fn $name() {
                run_ppu_bg_screen_crc($file, $frames, $crc);
            }
        };
    }

    undisbeliever_ppu_bg_test!(
        vmain_1bpp_no_remapping,
        "vmain-1bpp-no-remapping.sfc",
        107,
        0x755E_7FBD
    );

    undisbeliever_ppu_bg_test!(
        vmain_2bpp_no_remapping,
        "vmain-2bpp-no-remapping.sfc",
        115,
        0x029C_AE27
    );

    undisbeliever_ppu_bg_test!(
        vmain_4bpp_no_remapping,
        "vmain-4bpp-no-remapping.sfc",
        129,
        0xAC71_052A
    );

    undisbeliever_ppu_bg_test!(
        vmain_4bpp_no_remapping_word,
        "vmain-4bpp-no-remapping-word.sfc",
        129,
        0xAC71_052A
    );

    undisbeliever_ppu_bg_test!(
        vmain_8bpp_no_remapping,
        "vmain-8bpp-no-remapping.sfc",
        156,
        0xB903_D8CA
    );

    undisbeliever_ppu_bg_test!(
        vmain_1bpp_tiles_0,
        "vmain-1bpp-tiles-0.sfc",
        65,
        0x5CA4_211F
    );

    undisbeliever_ppu_bg_test!(
        vmain_1bpp_tiles_1,
        "vmain-1bpp-tiles-1.sfc",
        65,
        0x13CE_1D30
    );

    undisbeliever_ppu_bg_test!(
        vram_writes_without_dma,
        "vram-writes-without-dma.sfc",
        66,
        0x1D70_AE67
    );
}
