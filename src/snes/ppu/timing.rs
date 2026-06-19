//! PPU dot/scanline timing.
//!
//! The bus calls [`Ppu::tick`] once per master clock. The PPU accumulates master clocks and
//! advances one dot every [`MASTER_CYCLES_PER_DOT`] cycles, wrapping the dot counter at
//! [`DOTS_PER_SCANLINE`] and the scanline counter at [`NTSC_SCANLINES_PER_FRAME`].
//!
//! Note: long/short-dot quirks (the extra/short dot at 323/327, and the 1364-vs-1360 master
//! clocks on certain scanlines) are not yet modeled and are a documented refinement TODO.

use super::{
    DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME, Ppu, VBLANK_START_LINE,
    VISIBLE_LINE_START,
};

impl Ppu {
    /// Advance the PPU by one master clock.
    pub fn tick(&mut self) {
        self.master_cycle_accumulator += 1;
        if self.master_cycle_accumulator < MASTER_CYCLES_PER_DOT {
            return;
        }
        self.master_cycle_accumulator -= MASTER_CYCLES_PER_DOT;
        self.advance_dot();
    }

    fn advance_dot(&mut self) {
        self.position.dot += 1;
        if self.position.dot >= DOTS_PER_SCANLINE {
            self.position.dot = 0;
            self.position.scanline += 1;
            if self.position.scanline >= NTSC_SCANLINES_PER_FRAME {
                self.position.scanline = 0;
            }
            self.on_scanline_start();
        }
        let forced_blank = self.inidisp & 0x80 != 0;
        self.update_obj_pipeline(forced_blank);
        self.render_dot();
    }

    fn on_scanline_start(&mut self) {
        let scanline = self.position.scanline;
        // Advance the vertical mosaic block counter for visible scanlines.
        if (VISIBLE_LINE_START..VBLANK_START_LINE).contains(&scanline) {
            self.advance_mosaic_vcount(scanline);
        }
        match scanline {
            VBLANK_START_LINE => {
                // Begin VBlank: a full visible frame has been produced. Set the VBlank + RDNMI
                // flags (the flag is set even if NMI is disabled), then re-evaluate the NMI line.
                self.vblank_active = true;
                self.nmi_flag = true;
                self.frame_complete = true;
                self.update_nmi_line();
            }
            0 => {
                // End of VBlank / top of a new frame: clear the VBlank + RDNMI flags.
                self.vblank_active = false;
                self.nmi_flag = false;
                self.update_nmi_line();
            }
            _ => {}
        }
    }

    /// Re-evaluate the NMI line (`nmi_enable && nmi_flag`) and latch a rising edge for the CPU.
    pub(super) fn update_nmi_line(&mut self) {
        let line = self.nmi_enable && self.nmi_flag;
        if line && !self.nmi_line_prev {
            self.nmi_edge = true;
        }
        self.nmi_line_prev = line;
    }

    /// Latch the current H/V counters into OPHCT/OPVCT and set the STAT78 latch flag.
    pub(super) fn latch_counters(&mut self) {
        self.ophct_latch = self.position.dot;
        self.opvct_latch = self.position.scanline;
        self.counter_latch_flag = true;
    }

    /// SLHV ($2137) software strobe: latch counters only if WRIO ($4201) bit 7 is set.
    pub(super) fn latch_strobe(&mut self) {
        if self.wrio & 0x80 != 0 {
            self.latch_counters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, Ppu, ScanPosition};

    #[test]
    fn tick_should_advance_one_dot_every_four_master_clocks() {
        let mut ppu = Ppu::new();

        for _ in 0..MASTER_CYCLES_PER_DOT {
            ppu.tick();
        }

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 1
            }
        );
    }

    #[test]
    fn tick_should_advance_scanline_and_wrap_the_dot_counter() {
        let mut ppu = Ppu::new();

        for _ in 0..(DOTS_PER_SCANLINE as u32 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 1,
                dot: 0
            }
        );
    }

    fn tick_dots(ppu: &mut Ppu, dots: u32) {
        for _ in 0..(dots * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }
    }

    #[test]
    fn slhv_latches_the_h_and_v_counters_for_ophct_opvct() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 20);

        // SLHV strobe (WRIO bit7 is set on reset) latches the current H/V counters.
        ppu.read_register(0x2137);

        // OPHCT read-twice: low byte then high bit.
        assert_eq!(ppu.read_register(0x213C), 20);
        assert_eq!(ppu.read_register(0x213C), 0);
        // OPVCT read-twice.
        assert_eq!(ppu.read_register(0x213D), 0);
        assert_eq!(ppu.read_register(0x213D), 0);
    }

    #[test]
    fn slhv_latches_a_nonzero_scanline() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 3 + 5);

        ppu.read_register(0x2137);

        assert_eq!(ppu.read_register(0x213C), 5);
        assert_eq!(ppu.read_register(0x213C), 0);
        assert_eq!(ppu.read_register(0x213D), 3);
        assert_eq!(ppu.read_register(0x213D), 0);
    }

    #[test]
    fn stat78_read_resets_the_counter_read_flipflops() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 20);
        ppu.read_register(0x2137);

        let low_first = ppu.read_register(0x213C); // low byte, flip -> high
        ppu.read_register(0x213F); // resets both flipflops
        let low_again = ppu.read_register(0x213C); // low byte again

        assert_eq!(low_first, 20);
        assert_eq!(low_again, 20);
    }

    #[test]
    fn stat78_reports_and_clears_the_latch_flag() {
        let mut ppu = Ppu::new();
        ppu.read_register(0x2137); // latch (WRIO bit7 set on reset)

        let first = ppu.read_register(0x213F);
        let second = ppu.read_register(0x213F);

        assert_ne!(first & 0x40, 0);
        assert_eq!(second & 0x40, 0);
    }

    #[test]
    fn wrio_high_to_low_transition_latches_the_counters() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4201, 0x80); // bit7 = 1
        tick_dots(&mut ppu, 10);
        ppu.write_register(0x4201, 0x00); // 1 -> 0 transition latches

        let status = ppu.read_register(0x213F);
        assert_ne!(status & 0x40, 0);
        assert_eq!(ppu.read_register(0x213C), 10);
    }

    #[test]
    fn slhv_does_not_latch_when_wrio_bit7_is_clear() {
        let mut ppu = Ppu::new();
        // The reset->0x00 write is itself a 1->0 transition that latches; clear that flag first.
        ppu.write_register(0x4201, 0x00);
        ppu.read_register(0x213F);

        tick_dots(&mut ppu, 15);
        ppu.read_register(0x2137); // SLHV with WRIO bit7 clear must not latch.

        assert_eq!(ppu.read_register(0x213F) & 0x40, 0);
    }

    fn tick_to_vblank(ppu: &mut Ppu) {
        // Advance to the start of scanline 225 (VBlank entry).
        tick_dots(ppu, DOTS_PER_SCANLINE as u32 * 225);
    }

    #[test]
    fn entering_vblank_sets_flags_and_raises_an_nmi_edge_when_enabled() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4200, 0x80); // enable VBlank NMI

        tick_to_vblank(&mut ppu);

        assert!(ppu.in_vblank());
        assert!(
            ppu.poll_nmi(),
            "an NMI edge should be delivered at VBlank entry"
        );
        assert!(!ppu.poll_nmi(), "the edge is consumed only once");
        assert_ne!(ppu.read_register(0x4210) & 0x80, 0, "RDNMI flag is set");
    }

    #[test]
    fn vblank_flag_is_set_even_when_nmi_disabled_without_an_edge() {
        let mut ppu = Ppu::new();

        tick_to_vblank(&mut ppu);

        assert!(!ppu.poll_nmi(), "no edge while NMI is disabled");
        assert_ne!(ppu.read_register(0x4210) & 0x80, 0, "RDNMI flag still set");
    }

    #[test]
    fn enabling_nmi_during_vblank_raises_an_edge() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        assert!(!ppu.poll_nmi());

        ppu.write_register(0x4200, 0x80); // enable mid-VBlank while the flag is set

        assert!(ppu.poll_nmi(), "enabling NMI during VBlank raises an edge");
    }

    #[test]
    fn rdnmi_read_acknowledges_the_flag_and_reports_cpu_version() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);

        let first = ppu.read_register(0x4210);
        let second = ppu.read_register(0x4210);

        assert_ne!(first & 0x80, 0, "flag set on first read");
        assert_eq!(
            first & 0x0F,
            super::super::CPU_VERSION,
            "CPU version in low nibble"
        );
        assert_eq!(second & 0x80, 0, "flag cleared after read");
    }

    #[test]
    fn vblank_flag_clears_at_the_top_of_the_next_frame() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        assert!(ppu.in_vblank());

        // Advance through the rest of VBlank back to scanline 0.
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * (262 - 225));

        assert!(!ppu.in_vblank());
        assert_eq!(
            ppu.read_register(0x4210) & 0x80,
            0,
            "RDNMI flag cleared at frame top"
        );
    }

    #[test]
    fn hvbjoy_reports_vblank_and_hblank_flags() {
        let mut ppu = Ppu::new();

        // Scanline 0, dot 300: HBlank set (dot >= 274), VBlank clear.
        tick_dots(&mut ppu, 300);
        let mid = ppu.read_register(0x4212);
        assert_eq!(mid & 0x80, 0, "not in VBlank on scanline 0");
        assert_ne!(mid & 0x40, 0, "HBlank set at dot 300");

        // Advance to VBlank entry (scanline 225, dot 0): VBlank set, HBlank clear.
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        let vb = ppu.read_register(0x4212);
        assert_ne!(vb & 0x80, 0, "VBlank flag set");
        assert_eq!(vb & 0x40, 0, "HBlank clear at dot 0");
    }
}
