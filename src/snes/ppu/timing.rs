//! PPU dot/scanline timing.
//!
//! The bus calls [`Ppu::tick`] once per master clock. The PPU accumulates master clocks and
//! advances one dot at a time using the active scanline timing profile, which can shorten/lengthen
//! the line and insert paired long-dot phases. Normal dots are 4 master clocks wide; scanline
//! totals still wrap the scanline counter at the active region's `scanlines_per_frame()` (262 NTSC
//! / 312 PAL).
//!
//! Note: long/short-dot quirks now use a latched per-scanline profile, including the paired
//! long-dot phase around H=323/327 and the NTSC/PAL short/long scanline exceptions.

use super::{
    DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, Ppu, PpuLineTimingProfile, VISIBLE_LINE_START,
};
use crate::platform::debugging::ppu_trace_level;
use crate::trace_ppu;

impl Ppu {
    /// Advance the PPU by one master clock.
    pub fn tick(&mut self) {
        if ppu_trace_level() >= 5 {
            trace_ppu!(5; "{}", self.format_trace_tick_line());
        }
        self.master_cycle_accumulator += 1;
        let cycles_per_dot = self.cycles_per_current_dot();
        if self.master_cycle_accumulator < cycles_per_dot {
            return;
        }
        self.master_cycle_accumulator -= cycles_per_dot;
        self.advance_dot();
    }

    fn advance_dot(&mut self) {
        self.position.dot += 1;
        if self.position.dot >= self.dots_in_current_scanline() {
            self.position.dot = 0;
            self.position.scanline += 1;
            if self.position.scanline >= self.scanlines_per_frame() {
                self.position.scanline = 0;
            }
            self.on_scanline_start();
        }
        if self.position.scanline == self.vblank_start_line()
            && self.position.dot == super::AUTO_JOYPAD_LATCH_DOT
        {
            self.auto_joypad_latch = true;
        }
        self.evaluate_hv_irq();
        let forced_blank = self.inidisp & 0x80 != 0;
        self.update_obj_pipeline(forced_blank);
        self.render_dot();
    }

    fn dots_in_current_scanline(&self) -> u16 {
        match self.line_timing_profile {
            PpuLineTimingProfile::Short => DOTS_PER_SCANLINE - 1,
            PpuLineTimingProfile::Long => DOTS_PER_SCANLINE + 1,
            PpuLineTimingProfile::Normal => DOTS_PER_SCANLINE,
        }
    }

    fn cycles_per_current_dot(&self) -> u32 {
        match self.line_timing_profile {
            PpuLineTimingProfile::Short => MASTER_CYCLES_PER_DOT,
            PpuLineTimingProfile::Normal | PpuLineTimingProfile::Long => match self.position.dot {
                323 | 327 => MASTER_CYCLES_PER_DOT + 2,
                324 | 328 => MASTER_CYCLES_PER_DOT - 2,
                _ => MASTER_CYCLES_PER_DOT,
            },
        }
    }

    pub(super) fn hblank_active(&self) -> bool {
        self.position.dot == 0 || self.position.dot >= super::HBLANK_START_DOT
    }

    fn line_timing_profile_for_scanline(&self) -> PpuLineTimingProfile {
        if self.video_region == super::SnesVideoRegion::Ntsc
            && !self.interlace_enabled()
            && self.interlace_field
            && self.position.scanline == 240
        {
            PpuLineTimingProfile::Short
        } else if self.video_region == super::SnesVideoRegion::Pal
            && self.interlace_enabled()
            && self.interlace_field
            && self.position.scanline == 311
        {
            PpuLineTimingProfile::Long
        } else {
            PpuLineTimingProfile::Normal
        }
    }

    fn latch_line_timing_profile(&mut self) {
        self.line_timing_profile = self.line_timing_profile_for_scanline();
    }

    fn on_scanline_start(&mut self) {
        let scanline = self.position.scanline;
        self.latch_line_timing_profile();
        let vblank_start_line = self.vblank_start_line();
        // Advance the vertical mosaic block counter for visible scanlines.
        if (VISIBLE_LINE_START..vblank_start_line).contains(&scanline) {
            self.advance_mosaic_vcount(scanline);
        }
        match scanline {
            _ if scanline == vblank_start_line => {
                // Begin VBlank: a full visible frame has been produced. Set the VBlank + RDNMI
                // flags (the flag is set even if NMI is disabled), then re-evaluate the NMI line.
                self.vblank_active = true;
                self.nmi_flag = true;
                self.frame_complete = true;
                trace_ppu!(1; "vblank enter y={} x={} inidisp={:02X} mode={} tm={:02X} ts={:02X}",
                    self.position.scanline,
                    self.position.dot,
                    self.inidisp,
                    self.bg_mode,
                    self.tm,
                    self.ts,
                );
                self.update_nmi_line();
            }
            0 => {
                // End of VBlank / top of a new frame: clear the VBlank + RDNMI flags.
                self.vblank_active = false;
                self.nmi_flag = false;
                // The field parity still advances even when interlace output is disabled, because
                // the short/long scanline exceptions are keyed off the latched field state.
                self.interlace_field = !self.interlace_field;
                trace_ppu!(1; "frame wrap y={} x={} field={} inidisp={:02X} mode={} tm={:02X} ts={:02X}",
                    self.position.scanline,
                    self.position.dot,
                    self.interlace_field as u8,
                    self.inidisp,
                    self.bg_mode,
                    self.tm,
                    self.ts,
                );
                self.update_nmi_line();
            }
            _ => {}
        }
    }

    /// Re-evaluate the NMI line (`nmi_enable && nmi_flag`) and latch a rising edge for the CPU.
    pub(super) fn update_nmi_line(&mut self) {
        let line = self.nmi_enable && self.nmi_flag;
        if line && !self.nmi_line_prev {
            trace_ppu!(2; "nmi edge y={} x={} inidisp={:02X} nmi={} vblank={}",
                self.position.scanline,
                self.position.dot,
                self.inidisp,
                self.nmi_enable as u8,
                self.vblank_active as u8,
            );
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

    fn evaluate_hv_irq(&mut self) {
        let h = self.position.dot;
        let v = self.position.scanline;
        let triggered = match self.irq_mode {
            // H IRQ each scanline at HTIME.
            1 => h == self.htime,
            // V IRQ once on matching scanline (current dot model: dot 0 of that line).
            2 => h == 0 && v == self.vtime,
            // HV IRQ at HTIME of VTIME line, with HTIME=0 as the dot-0 special case.
            3 => {
                v == self.vtime
                    && ((self.htime == 0 && h == 0) || (self.htime != 0 && h == self.htime))
            }
            _ => false,
        };
        if triggered && !self.timeup_flag {
            trace_ppu!(2; "timeup y={} x={} irq_mode={} htime={:03X} vtime={:03X}",
                h,
                self.position.dot,
                self.irq_mode,
                self.htime,
                self.vtime,
            );
        }
        if triggered {
            self.timeup_flag = true;
            self.irq_line = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, Ppu, PpuLineTimingProfile, ScanPosition,
        SnesVideoRegion,
    };

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
    fn hblank_is_visible_for_the_entire_first_dot_of_a_scanline() {
        let mut ppu = Ppu::new();
        ppu.position.scanline = 6;
        ppu.position.dot = 0;
        ppu.line_timing_profile = PpuLineTimingProfile::Normal;

        assert_ne!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "HBlank is set at dot 0"
        );

        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "HBlank is still set on dot 0"
        );

        tick_cycles(&mut ppu, 3);
        assert_eq!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "HBlank clears on dot 1"
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

    fn tick_cycles(ppu: &mut Ppu, cycles: u32) {
        for _ in 0..cycles {
            ppu.tick();
        }
    }

    fn tick_scanlines(ppu: &mut Ppu, scanlines: u32) {
        tick_dots(ppu, DOTS_PER_SCANLINE as u32 * scanlines);
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

    #[test]
    fn auto_joypad_latch_fires_at_dot_32_of_first_vblank_scanline() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        assert!(
            !ppu.poll_auto_joypad_latch(),
            "latch should not fire at the very start of VBlank"
        );

        tick_dots(&mut ppu, super::super::AUTO_JOYPAD_LATCH_DOT as u32);
        assert!(
            ppu.poll_auto_joypad_latch(),
            "latch fires once the first VBlank scanline reaches the latch dot"
        );
        assert!(
            !ppu.poll_auto_joypad_latch(),
            "the auto-joypad latch signal is one-shot"
        );
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

        // Advance to VBlank entry (scanline 225, dot 0): VBlank set, HBlank still set.
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        let vb = ppu.read_register(0x4212);
        assert_ne!(vb & 0x80, 0, "VBlank flag set");
        assert_ne!(vb & 0x40, 0, "HBlank remains set at dot 0");
    }

    #[test]
    fn hblank_flag_stays_set_at_dot_0_and_clears_at_dot_1() {
        let mut ppu = Ppu::new();
        ppu.position.scanline = 6;
        ppu.position.dot = 0;
        ppu.line_timing_profile = PpuLineTimingProfile::Normal;

        assert_ne!(ppu.read_register(0x4212) & 0x40, 0, "HBlank set at dot 0");

        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "HBlank remains set during dot 0"
        );

        tick_cycles(&mut ppu, 3);
        assert_eq!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "HBlank clears at dot 1"
        );
    }

    #[test]
    fn overscan_239_line_mode_moves_vblank_entry_to_scanline_240() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x04); // SETINI overscan/tall-screen bit

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 239);
        assert!(!ppu.in_vblank(), "line 239 should still be visible");

        // One additional full scanline reaches scanline 240, where VBlank begins.
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32);
        assert!(ppu.in_vblank(), "VBlank should begin at scanline 240");
    }

    #[test]
    fn h_irq_sets_timeup_and_4211_read_acknowledges_it() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x01);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);

        tick_dots(&mut ppu, 1);

        let first = ppu.read_register(0x4211);
        let second = ppu.read_register(0x4211);
        assert_ne!(first & 0x80, 0, "TIMEUP should be set at the H-IRQ point");
        assert_eq!(second & 0x80, 0, "reading TIMEUP should acknowledge it");
    }

    #[test]
    fn h_irq_triggers_on_every_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x02);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);

        tick_dots(&mut ppu, 2);
        assert_ne!(ppu.read_register(0x4211) & 0x80, 0, "line 0 trigger");

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "line 1 trigger at same H position"
        );
    }

    #[test]
    fn disabling_irq_mode_clears_timeup_flag() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x01);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);
        tick_dots(&mut ppu, 1);
        assert_ne!(ppu.read_register(0x4211) & 0x80, 0);

        ppu.write_register(0x4200, 0x00);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "disabling IRQ mode must clear TIMEUP"
        );
    }

    #[test]
    fn v_irq_triggers_once_on_the_matching_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0x02);
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x20);

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 2);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "V-IRQ should trigger at VTIME"
        );

        tick_dots(&mut ppu, 1);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "V-IRQ must not retrigger on every dot of the same line"
        );
    }

    #[test]
    fn hv_irq_triggers_on_htime_of_vtime_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x05);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4209, 0x03);
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x30);

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 3 + 4);
        assert_eq!(ppu.read_register(0x4211) & 0x80, 0);

        tick_dots(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "HV IRQ should trigger at the programmed H position on the programmed V line"
        );
    }

    #[test]
    fn hv_irq_with_htime_zero_triggers_at_dot_zero_of_vtime_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x00);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4209, 0x04);
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x30);

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 4);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "HTIME=0 HV mode should trigger at dot 0 of the VTIME line"
        );
    }

    #[test]
    fn ntsc_scanline_counter_wraps_after_262_scanlines() {
        let mut ppu = Ppu::new();
        tick_scanlines(&mut ppu, 262);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
    }

    #[test]
    fn pal_scanline_counter_wraps_after_312_scanlines() {
        let mut ppu = Ppu::new_with_region(SnesVideoRegion::Pal);
        tick_scanlines(&mut ppu, 312);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
    }

    #[test]
    fn ntsc_short_scanline_240_field_1_is_1360_master_cycles() {
        let mut ppu = Ppu::new();
        ppu.position.scanline = 240;
        ppu.position.dot = 0;
        ppu.interlace_field = true;
        ppu.line_timing_profile = PpuLineTimingProfile::Short;
        ppu.write_register(0x2133, 0x00); // non-interlaced output

        // fullsnes: NTSC short line at V=240, field=1, non-interlace is 1360 cycles.
        for _ in 0..1360 {
            ppu.tick();
        }

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 241,
                dot: 0
            }
        );
    }

    #[test]
    fn pal_interlace_long_scanline_311_field_1_is_1368_master_cycles() {
        let mut ppu = Ppu::new_with_region(SnesVideoRegion::Pal);
        ppu.position.scanline = 311;
        ppu.position.dot = 0;
        ppu.interlace_field = true;
        ppu.line_timing_profile = PpuLineTimingProfile::Long;
        ppu.write_register(0x2133, 0x01); // interlaced output

        // fullsnes: PAL interlaced line 311 in field=1 is a long 1368-cycle line.
        for _ in 0..1364 {
            ppu.tick();
        }
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 311,
                dot: 341
            }
        );

        for _ in 0..4 {
            ppu.tick();
        }
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
    }

    #[test]
    fn pal_long_scanline_applies_extra_cycles_at_dot_327_not_line_end() {
        let mut ppu = Ppu::new_with_region(SnesVideoRegion::Pal);
        ppu.position.scanline = 311;
        ppu.position.dot = 327;
        ppu.interlace_field = true;
        ppu.line_timing_profile = PpuLineTimingProfile::Long;
        ppu.write_register(0x2133, 0x01); // interlaced output

        for _ in 0..4 {
            ppu.tick();
        }
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 311,
                dot: 327
            }
        );

        for _ in 0..2 {
            ppu.tick();
        }
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 311,
                dot: 328
            }
        );
    }

    #[test]
    fn ntsc_short_scanline_has_no_extra_cycles_at_dot_327() {
        let mut ppu = Ppu::new();
        ppu.position.scanline = 240;
        ppu.position.dot = 327;
        ppu.interlace_field = true;
        ppu.line_timing_profile = PpuLineTimingProfile::Short;
        ppu.write_register(0x2133, 0x00); // non-interlaced output

        for _ in 0..4 {
            ppu.tick();
        }
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 240,
                dot: 328
            }
        );
    }

    #[test]
    fn normal_scanline_uses_the_paired_long_dot_phase() {
        let mut ppu = Ppu::new();
        ppu.line_timing_profile = PpuLineTimingProfile::Normal;
        ppu.position.scanline = 10;
        ppu.position.dot = 323;

        tick_cycles(&mut ppu, 4);
        assert_eq!(ppu.position().dot, 323);
        tick_cycles(&mut ppu, 2);
        assert_eq!(ppu.position().dot, 324);

        ppu.position.dot = 327;
        tick_cycles(&mut ppu, 4);
        assert_eq!(ppu.position().dot, 327);
        tick_cycles(&mut ppu, 2);
        assert_eq!(ppu.position().dot, 328);
    }

    #[test]
    fn interlace_toggle_during_a_scanline_does_not_retime_the_active_line() {
        let mut ppu = Ppu::new();
        ppu.position.scanline = 240;
        ppu.position.dot = 0;
        ppu.interlace_field = true;
        ppu.line_timing_profile = PpuLineTimingProfile::Short;

        ppu.write_register(0x2133, 0x01);
        tick_cycles(&mut ppu, 1360);

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 241,
                dot: 0
            }
        );
    }
}
