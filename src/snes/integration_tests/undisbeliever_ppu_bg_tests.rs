//! Automates 12 of the 18 vendored undisbeliever PPU BG / VMAIN test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg/`, built
//! from the source mirror -- see that folder's README) covering VRAM
//! increment modes at 1/2/4/8bpp, byte- vs word-increment uploads, plain
//! (non-DMA) VRAM data-port writes, 1bpp tile decode, a text tilemap, and
//! the animated scrolling/textbuffer demos (issues #2878, #2990).
//!
//! Every static golden was approved via the #2878 baseline workflow: probed
//! to its settle frame (screen CRC unchanged for >= 600 consecutive frames),
//! sampled at settle + 60, and pixel-diffed against a Mesen2 headless capture
//! of the identical ROM at the same frame (`--Video.VideoFilter=None
//! --Video.AspectRatio=NoStretching --snes.disableFrameSkipping=true`; the
//! frame-skip switch is essential for animated content, see #2990) -- all
//! match Mesen2 exactly. The shared CRC for
//! `vmain-4bpp-no-remapping{,-word}.sfc` is by design: the demos draw the
//! same screen via byte- and word-increment writes.
//!
//! The four animated demos (three vmain-*-scrolling + textbuffer-hello-world)
//! never settle, so their goldens were derived directly from frame-skip-free
//! Mesen2 captures -- the scrolling demos at frames 120, 360 and 600 each,
//! textbuffer at frame 120 -- and additionally verified pixel-identical to
//! Mesen2 across frames 118-122 and 598-601 (#2990).
//!
//! The other 6 vendored ROMs
//! (`vmain-{1bpp,2bpp,2bpp-split,4bpp,4bpp-word,8bpp}-with-remapping.sfc`)
//! are deliberately left un-automated because cross-checking exposed a real
//! NESER divergence, tracked as #2989 (VMAIN $2115 bits 2-3 address remapping
//! is not implemented; the no-remapping twins below prove everything else in
//! the upload path) rather than papered over with known-wrong goldens.

use super::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const UNDISBELIEVER_PPU_BG_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg";

#[cfg(test)]
mod tests {
    use super::*;

    /// The demos settle within ~100 frames (init plus a tile-buffer upload
    /// spread over several frames, then a static screen forever); each
    /// sampled frame is the probed settle frame plus a 60-frame margin. The
    /// 400M tick budget matches the other screen-CRC suites.
    ///
    /// Deliberately does not use `rom_runner::assert_rom_screen_crc`: its
    /// panic message says "expected screen-CRC PASS", which would be
    /// misleading here -- these demos draw no PASS/FAIL text, so a mismatch
    /// means the approved golden screen changed, not that a "test" failed.
    fn run_ppu_bg_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(UNDISBELIEVER_PPU_BG_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "undisbeliever_ppu_bg_tests",
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

    // The four animated demos below never settle, so they are sampled at
    // fixed frames instead of settle + 60. Every golden is derived directly
    // from a frame-skip-free Mesen2 capture (issue #2990): decode the Mesen2
    // PNG at the same frame to the `screen_snapshot` RGB layout and CRC32 it.
    // The scrolling demos are pinned at three spread-out frames each
    // (120/360/600) so a cadence or accumulating-phase regression cannot slip
    // past a single lucky sample. Frame numbers count every vblank entry,
    // including vblanks that elapse while a long DMA is in flight (see
    // `Ppu::take_completed_frames`).

    undisbeliever_ppu_bg_test!(
        vmain_horizontal_scrolling_f120,
        "vmain-horizontal-scrolling.sfc",
        120,
        0xA658_FD8B
    );

    undisbeliever_ppu_bg_test!(
        vmain_horizontal_scrolling_f360,
        "vmain-horizontal-scrolling.sfc",
        360,
        0x4B94_FFD2
    );

    undisbeliever_ppu_bg_test!(
        vmain_horizontal_scrolling_f600,
        "vmain-horizontal-scrolling.sfc",
        600,
        0xF4B9_4F56
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_f120,
        "vmain-vertical-scrolling.sfc",
        120,
        0x0622_B19F
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_f360,
        "vmain-vertical-scrolling.sfc",
        360,
        0x2B1C_23B9
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_f600,
        "vmain-vertical-scrolling.sfc",
        600,
        0x9325_3E90
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_2_rows_f120,
        "vmain-vertical-scrolling-2-rows.sfc",
        120,
        0x9E07_7E8B
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_2_rows_f360,
        "vmain-vertical-scrolling-2-rows.sfc",
        360,
        0xC268_D0EB
    );

    undisbeliever_ppu_bg_test!(
        vmain_vertical_scrolling_2_rows_f600,
        "vmain-vertical-scrolling-2-rows.sfc",
        600,
        0xC1A0_6C42
    );

    undisbeliever_ppu_bg_test!(
        textbuffer_hello_world,
        "textbuffer-hello-world.sfc",
        120,
        0x29FA_FE50
    );
}
