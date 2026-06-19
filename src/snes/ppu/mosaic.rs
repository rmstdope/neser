//! Mosaic effect helpers for BG and Mode 7 rendering.
//!
//! MOSAIC $2106 register layout:
//! - Bits 7-4: mosaic size (0 = 1×1, 15 = 16×16)
//! - Bits 3-0: per-BG enable (bit 0 = BG1 … bit 3 = BG4)
//!
//! **Horizontal mosaic**: `x_mos = x - (x % block_size)` where `block_size = size + 1`. The
//! horizontal block starts at the left screen edge (x = 0) and uses the current register value
//! (immediate effect on $2106 write).
//!
//! **Vertical mosaic**: implemented by subtracting the vertical index within the current block
//! (`mosaic_vcount`) from BGnVOFS before adding the screen Y. The vertical block size change on a
//! mid-frame $2106 write takes effect only at the start of the *next* vertical block (the current
//! block finishes using the old size, per the fullsnes specification).

use super::{Ppu, VISIBLE_LINE_START};

impl Ppu {
    /// The current horizontal mosaic block size in pixels (1..=16), derived from the raw register.
    pub(super) fn mosaic_h_block_size(&self) -> u16 {
        ((self.mosaic >> 4) as u16) + 1
    }

    /// Returns `true` if mosaic is enabled for background layer `bg` (0-based, 0 = BG1).
    pub(super) fn mosaic_bg_enabled(&self, bg: usize) -> bool {
        self.mosaic & (1 << bg) != 0
    }

    /// Snap screen X coordinate to the left edge of its horizontal mosaic block.
    pub(super) fn mosaic_apply_x(&self, x: u16) -> u16 {
        let block = self.mosaic_h_block_size();
        x - x % block
    }

    /// Advance the vertical mosaic block counter at the start of a visible scanline.
    ///
    /// - At the first visible scanline (`VISIBLE_LINE_START`): reset `mosaic_vcount` to 0 and
    ///   latch the current mosaic size into `mosaic_vblock_size` (start of first block).
    /// - At subsequent visible scanlines: increment `mosaic_vcount`; when it equals
    ///   `mosaic_vblock_size` a new block starts — reset `mosaic_vcount` to 0 and latch the
    ///   pending size. This implements "finish current block before applying new size" (fullsnes).
    pub(super) fn advance_mosaic_vcount(&mut self, scanline: u16) {
        if scanline == VISIBLE_LINE_START || self.mosaic_vcount == self.mosaic_vblock_size {
            self.mosaic_vcount = 0;
            self.mosaic_vblock_size = self.mosaic >> 4;
        } else {
            self.mosaic_vcount += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Ppu;

    fn ppu_with_mosaic(reg: u8) -> Ppu {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2106, reg);
        ppu
    }

    // ── Horizontal mosaic ────────────────────────────────────────────────────

    #[test]
    fn size_0_means_1x1_no_horizontal_change() {
        // Size 0 in bits 7-4 means block_size 1 (1×1 = no mosaic).
        let ppu0 = ppu_with_mosaic(0x01); // size=0 in bits 7-4, BG1 enabled
        assert_eq!(ppu0.mosaic_h_block_size(), 1);
        // With block_size=1, every x maps to itself.
        for x in 0u16..=255 {
            assert_eq!(ppu0.mosaic_apply_x(x), x, "x={x}");
        }
    }

    #[test]
    fn horizontal_snaps_x_to_left_of_block() {
        let ppu = ppu_with_mosaic(0x31); // size=3 (bits 7-4=3) → block_size=4, BG1 enabled
        assert_eq!(ppu.mosaic_h_block_size(), 4);
        assert_eq!(ppu.mosaic_apply_x(0), 0);
        assert_eq!(ppu.mosaic_apply_x(1), 0);
        assert_eq!(ppu.mosaic_apply_x(3), 0);
        assert_eq!(ppu.mosaic_apply_x(4), 4);
        assert_eq!(ppu.mosaic_apply_x(7), 4);
        assert_eq!(ppu.mosaic_apply_x(8), 8);
        assert_eq!(ppu.mosaic_apply_x(255), 252);
    }

    #[test]
    fn horizontal_block_starts_at_left_edge() {
        let ppu = ppu_with_mosaic(0xF1); // size=15 → block_size=16, BG1 enabled
        assert_eq!(ppu.mosaic_apply_x(0), 0);
        assert_eq!(ppu.mosaic_apply_x(15), 0);
        assert_eq!(ppu.mosaic_apply_x(16), 16);
    }

    #[test]
    fn size_15_means_16x16_blocks() {
        let ppu = ppu_with_mosaic(0xF1); // bits 7-4 = 0xF = 15 → block_size = 16
        assert_eq!(ppu.mosaic_h_block_size(), 16);
    }

    // ── Per-BG enable ────────────────────────────────────────────────────────

    #[test]
    fn per_bg_enable_bit0_controls_bg1() {
        let ppu_on = ppu_with_mosaic(0x11); // BG1 on
        let ppu_off = ppu_with_mosaic(0x10); // BG1 off (bits 0 = 0)
        assert!(ppu_on.mosaic_bg_enabled(0));
        assert!(!ppu_off.mosaic_bg_enabled(0));
    }

    #[test]
    fn per_bg_enable_controls_each_layer_independently() {
        // 0b0000_1010 = BG2 + BG4 enabled
        let ppu = ppu_with_mosaic(0x0A);
        assert!(!ppu.mosaic_bg_enabled(0)); // BG1
        assert!(ppu.mosaic_bg_enabled(1)); // BG2
        assert!(!ppu.mosaic_bg_enabled(2)); // BG3
        assert!(ppu.mosaic_bg_enabled(3)); // BG4
    }

    // ── Vertical mosaic block counter ────────────────────────────────────────

    #[test]
    fn frame_start_resets_vcount_and_latches_size() {
        let mut ppu = ppu_with_mosaic(0x31); // size=3 → vblock_size should become 3
        ppu.mosaic_vcount = 2;
        ppu.mosaic_vblock_size = 7; // stale value from before

        ppu.advance_mosaic_vcount(1); // scanline 1 = VISIBLE_LINE_START

        assert_eq!(ppu.mosaic_vcount, 0);
        assert_eq!(ppu.mosaic_vblock_size, 3);
    }

    #[test]
    fn vcount_increments_each_visible_scanline() {
        let mut ppu = ppu_with_mosaic(0x31); // size=3
        ppu.advance_mosaic_vcount(1); // scanline 1: vcount=0, vblock_size=3
        ppu.advance_mosaic_vcount(2); // scanline 2: vcount=1
        ppu.advance_mosaic_vcount(3); // scanline 3: vcount=2
        ppu.advance_mosaic_vcount(4); // scanline 4: vcount=3
        assert_eq!(ppu.mosaic_vcount, 3);
    }

    #[test]
    fn vcount_resets_at_block_boundary() {
        let mut ppu = ppu_with_mosaic(0x31); // size=3 → 4-scanline blocks
        ppu.advance_mosaic_vcount(1); // scanline 1: vcount=0
        ppu.advance_mosaic_vcount(2); // vcount=1
        ppu.advance_mosaic_vcount(3); // vcount=2
        ppu.advance_mosaic_vcount(4); // vcount=3
        ppu.advance_mosaic_vcount(5); // vcount==vblock_size → new block → vcount=0
        assert_eq!(ppu.mosaic_vcount, 0);
    }

    #[test]
    fn size_0_means_1x1_vcount_always_resets() {
        let mut ppu = ppu_with_mosaic(0x01); // size=0 → vblock_size=0, 1-scanline blocks
        ppu.advance_mosaic_vcount(1); // scanline 1: vcount=0
        ppu.advance_mosaic_vcount(2); // vcount==vblock_size(0) → new block → vcount=0
        assert_eq!(ppu.mosaic_vcount, 0);
        ppu.advance_mosaic_vcount(3); // still 0
        assert_eq!(ppu.mosaic_vcount, 0);
    }

    #[test]
    fn mid_frame_size_change_finishes_current_block_before_applying_new_size() {
        // Start with size=3 (4-scanline blocks): vblock_size=3, blocks at scanlines 1,5,9,...
        let mut ppu = ppu_with_mosaic(0x31);
        ppu.advance_mosaic_vcount(1); // vcount=0, vblock_size=3
        ppu.advance_mosaic_vcount(2); // vcount=1
        ppu.advance_mosaic_vcount(3); // vcount=2

        // Mid-frame: change size to 1 (2-scanline blocks); should NOT take effect until next block
        ppu.write_register(0x2106, 0x11); // size=1 in bits 7-4

        ppu.advance_mosaic_vcount(4); // vcount=3 (still old block, vblock_size still 3)
        assert_eq!(ppu.mosaic_vcount, 3);
        assert_eq!(ppu.mosaic_vblock_size, 3); // old size still active

        ppu.advance_mosaic_vcount(5); // vcount==vblock_size(3) → new block with NEW size
        assert_eq!(ppu.mosaic_vcount, 0);
        assert_eq!(ppu.mosaic_vblock_size, 1); // new size now active
    }
}
