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
    DOTS_PER_SCANLINE, DRAM_REFRESH_BASE_POSITION, HDMA_INIT_BASE_POSITION, HDMA_TRANSFER_POSITION,
    MASTER_CYCLES_PER_DOT, Ppu, PpuLineTimingProfile, VISIBLE_LINE_START,
};
use crate::platform::debugging::ppu_trace_level;
use crate::trace_ppu;

impl Ppu {
    /// Advance the PPU by one master clock.
    ///
    /// DRAM refresh (stealing [`DRAM_REFRESH_STOLEN_CLOCKS`] extra clocks once per scanline) is
    /// *not* handled here: it is a CPU/bus-wide stall, not a PPU-only event, so every stolen
    /// clock must also tick the APU and input latch the same way a normal clock does. The bus
    /// (`SnesSystemBus::tick`) is responsible for calling [`Ppu::dram_refresh_due`] after each
    /// single-clock `tick()` and looping its *entire* per-clock sequence (APU/PPU/input) for the
    /// stolen clocks, keeping every ticked device on the same master-clock timeline.
    pub fn tick(&mut self) {
        self.tick_one_clock();
    }

    /// Returns `true` if this clock was the scanline's DRAM-refresh trigger, i.e. the caller
    /// must additionally tick every master-clock-driven device (APU, PPU, input) for
    /// [`DRAM_REFRESH_STOLEN_CLOCKS`] more clocks. See [`Ppu::tick`] for why this can't be
    /// handled internally by the PPU alone.
    pub fn dram_refresh_due(&self) -> bool {
        u32::from(self.line_clock) == u32::from(self.dram_refresh_position)
    }

    /// Recompute this scanline's DRAM-refresh trigger clock from the current cumulative
    /// master clock count, matching the refresh circuit's phase jitter on real hardware.
    fn recompute_dram_refresh_position(&mut self) {
        self.dram_refresh_position =
            DRAM_REFRESH_BASE_POSITION - (self.total_master_clocks & 0x07) as u16;
    }

    /// Returns `true` if this clock is the once-per-frame HDMA channel reload point (only
    /// meaningful on scanline 0). The caller (`SnesSystemBus::tick`) must call `hdma_init()`
    /// on the bus's DMA controller when this fires.
    pub fn hdma_init_due(&self) -> bool {
        self.position.scanline == 0
            && u32::from(self.line_clock) == u32::from(self.hdma_init_position)
    }

    /// Recompute this scanline's HDMA-init trigger clock, matching the same phase-jitter
    /// formula as DRAM refresh (see [`HDMA_INIT_BASE_POSITION`]).
    fn recompute_hdma_init_position(&mut self) {
        self.hdma_init_position =
            HDMA_INIT_BASE_POSITION + (self.total_master_clocks & 0x07) as u16;
    }

    /// Returns `true` if this clock is the once-per-active-scanline HDMA transfer point. The
    /// caller (`SnesSystemBus::tick`) must call `hdma_do_line()` on the bus's DMA controller
    /// when this fires. Does not fire during VBlank (see [`HDMA_TRANSFER_POSITION`]).
    pub fn hdma_transfer_due(&self) -> bool {
        self.position.scanline < self.vblank_start_line()
            && u32::from(self.line_clock) == u32::from(HDMA_TRANSFER_POSITION)
    }

    /// The per-clock advance logic (dot/scanline/IRQ bookkeeping). Split out from [`Ppu::tick`]
    /// so DRAM refresh can steal extra clocks without re-checking the refresh trigger itself.
    fn tick_one_clock(&mut self) {
        if ppu_trace_level() >= 5 {
            trace_ppu!(5; "{}", self.format_trace_tick_line());
        }
        self.total_master_clocks += 1;
        self.master_cycle_accumulator += 1;
        let cycles_per_dot = self.cycles_per_current_dot();
        if self.master_cycle_accumulator >= cycles_per_dot {
            self.master_cycle_accumulator -= cycles_per_dot;
            if self.advance_dot() {
                // A new scanline just began: restart the intra-line clock at 0.
                self.line_clock = 0;
                self.recompute_dram_refresh_position();
                self.recompute_hdma_init_position();
            } else {
                self.line_clock += 1;
            }
        } else {
            self.line_clock += 1;
        }
        // Super Scope: latch the aimed coordinates once the beam reaches them.
        self.process_location_latch_request();
        // The interrupt counter circuit runs at master-clock/4 with the signal
        // inverted, ticking at intra-line clocks 2, 6, 10, ... (Mesen2
        // `InternalRegisters::ProcessIrqCounters`). Every scanline length
        // (1360/1364/1368) is divisible by 4, so the cadence holds across line
        // and frame wraps without special-casing. Both the H/V-IRQ and the
        // VBlank NMI events are generated there (#3144, #3145).
        if self.line_clock & 3 == 2 {
            self.process_irq_counters();
        }
    }

    /// Advance the dot/scanline counters by one dot. Returns `true` if the scanline wrapped
    /// (i.e. a new scanline began), so the caller can restart the intra-line clock.
    fn advance_dot(&mut self) -> bool {
        let mut new_scanline = false;
        self.position.dot += 1;
        if self.position.dot >= self.dots_in_current_scanline() {
            self.position.dot = 0;
            self.position.scanline += 1;
            if self.position.scanline >= self.effective_scanlines_per_frame() {
                self.position.scanline = 0;
            }
            self.on_scanline_start();
            new_scanline = true;
        }
        if self.position.scanline == self.vblank_start_line()
            && self.position.dot == super::AUTO_JOYPAD_LATCH_DOT
        {
            self.auto_joypad_latch = true;
        }
        let forced_blank = self.inidisp & 0x80 != 0;
        self.update_obj_pipeline(forced_blank);
        self.render_dot();
        new_scanline
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
        // HBLANK is reported for hClock outside [4, 1096]: it clears one
        // dot into the line and rises one master clock INTO dot 274
        // (hardware/Mesen `$4212`; the sub-dot edge is observable, #2914).
        self.position.dot == 0
            || self.position.dot > super::HBLANK_START_DOT
            || (self.position.dot == super::HBLANK_START_DOT && self.master_cycle_accumulator >= 1)
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
                // Begin VBlank: a full visible frame has been produced. The RDNMI flag and CPU
                // NMI line rise a few clocks into the line (see `Ppu::process_irq_counters`).
                self.vblank_active = true;
                self.pending_completed_frames = self.pending_completed_frames.saturating_add(1);
                trace_ppu!(1; "vblank enter y={} x={} inidisp={:02X} mode={} tm={:02X} ts={:02X}",
                    self.position.scanline,
                    self.position.dot,
                    self.inidisp,
                    self.bg_mode,
                    self.tm,
                    self.ts,
                );
            }
            0 => {
                // End of VBlank / top of a new frame. The RDNMI flag falls at intra-line
                // clock 2 (see `Ppu::process_irq_counters`).
                self.vblank_active = false;
                // Raise the once-per-frame edge the bus uses to re-arm the Super Scope
                // aim latch before the beam sweeps the visible area.
                self.frame_start_edge = true;
                // The field parity still advances even when interlace output is disabled, because
                // the short/long scanline exceptions are keyed off the latched field state.
                self.interlace_field = !self.interlace_field;
                // Latch this frame's length (Mesen2 UpdateNmiScanline, run after the
                // toggle): interlaced even fields get one extra scanline.
                self.frame_has_extra_scanline = self.interlace_enabled() && !self.interlace_field;
                trace_ppu!(1; "frame wrap y={} x={} field={} inidisp={:02X} mode={} tm={:02X} ts={:02X}",
                    self.position.scanline,
                    self.position.dot,
                    self.interlace_field as u8,
                    self.inidisp,
                    self.bg_mode,
                    self.tm,
                    self.ts,
                );
            }
            _ => {}
        }
        if scanline == VISIBLE_LINE_START {
            // Latch this frame's output layout (Mesen2 ProcessEndOfScanline resets
            // `_useHighResOutput` after rendering scanline 0, i.e. at exactly this
            // instant). From here on only `convert_to_hires` may change it, and only
            // upwards, so every row of the frame shares one layout (#3034).
            self.use_high_res_output = self.hires_output_enabled() || self.interlace_enabled();
        }
    }

    /// Re-evaluate the NMI line (`nmi_enable && nmi_flag`) and latch a rising edge for
    /// the CPU with the normal 1-cycle recognition arm.
    pub(super) fn update_nmi_line(&mut self) {
        self.update_nmi_line_arming(1);
    }

    /// Like [`Self::update_nmi_line`], but a rising edge latched by THIS call carries
    /// `arm` CPU cycles of recognition delay. The NMITIMEN write path passes 2
    /// (Mesen2 `SetNmiFlag(2)`, byuu test_nmi v1.1 test 27, #3081); every other
    /// caller uses the 1-cycle wrapper.
    pub(super) fn update_nmi_line_arming(&mut self, arm: u8) {
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
            self.nmi_edge_arm = arm;
        }
        self.nmi_line_prev = line;
    }

    /// The H position exposed to software via OPHCT/`$213C`, derived from `line_clock` the
    /// same way Mesen2's `SnesPpu::GetCycle()` derives it from `hClock` -- NOT simply the
    /// render-dot counter (`position.dot`). The two diverge only across the paired long-dot
    /// compensation region (dots 323/324 and 327/328, each widened to 6 or narrowed to 2
    /// master clocks so the scanline's total width stays correct): the render-dot counter
    /// tracks the true pixel position there, but the readable H counter is a separate
    /// hardware circuit that lags by up to one dot (4 clocks) through that region --
    /// oscillating between 0 and -1 dot -- before settling at a constant one dot behind from
    /// dot 328 onward (#3120).
    fn readable_h_position(line_clock: u16) -> u16 {
        if line_clock <= 1292 {
            line_clock >> 2
        } else if line_clock <= 1310 {
            (line_clock - 2) >> 2
        } else {
            (line_clock - 4) >> 2
        }
    }

    /// Latch the current H/V counters into OPHCT/OPVCT and set the STAT78 latch flag.
    pub(super) fn latch_counters(&mut self) {
        self.ophct_latch = Self::readable_h_position(self.line_clock);
        self.opvct_latch = self.position.scanline;
        self.counter_latch_flag = true;
    }

    /// SLHV ($2137) software strobe: latch counters only if WRIO ($4201) bit 7 is set.
    pub(super) fn latch_strobe(&mut self) {
        if self.wrio & 0x80 != 0 {
            self.latch_counters();
        }
    }

    /// Super Scope: request a latch of OPHCT/OPVCT at `(x, y)` once the beam
    /// sweeps past that position this frame (Mesen2 `SetLocationLatchRequest`).
    pub fn set_location_latch_request(&mut self, x: u16, y: u16) {
        self.location_latch_request = true;
        self.location_latch_x = x;
        self.location_latch_y = y;
    }

    /// Consume the once-per-frame frame-start edge (top of scanline 0). The bus
    /// polls the Super Scope and re-arms the aim latch when this returns `true`.
    pub fn take_frame_start(&mut self) -> bool {
        std::mem::take(&mut self.frame_start_edge)
    }

    /// Latch the requested Super Scope aim into OPHCT/OPVCT once the beam
    /// reaches it (Mesen2 `ProcessLocationLatchRequest`). The latched values are
    /// the *requested* coordinates, so they are deterministic regardless of the
    /// exact dot numbering; the beam comparison only decides the timing.
    fn process_location_latch_request(&mut self) {
        if !self.location_latch_request {
            return;
        }
        let (x, y) = (self.location_latch_x, self.location_latch_y);
        if self.position.scanline > y || (self.position.scanline == y && self.position.dot >= x) {
            self.ophct_latch = x;
            self.opvct_latch = y;
            self.counter_latch_flag = true;
            self.location_latch_request = false;
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

    /// #3120: `latch_counters` must expose the same *readable* H position Mesen2's
    /// `SnesPpu::GetCycle()` derives from `hClock`, not the raw render-dot counter. The two
    /// diverge only across the paired long-dot compensation region (dots 323/324 and
    /// 327/328, where a dot is widened to 6 or narrowed to 2 master clocks): Mesen2 reports a
    /// value up to one dot "behind" the render position there, oscillating between 0 and -1
    /// dot, before settling at a constant -1 dot from dot 328 onward. Table derived by
    /// running Mesen2's exact formula (`hClock<=1292 -> hClock>>2`; `hClock<=1310 ->
    /// (hClock-2)>>2`; else `(hClock-4)>>2`) against NESER's own
    /// (already-correct-for-rendering) dot-width model.
    #[test]
    fn latch_counters_reports_mesen2s_readable_h_position_not_the_raw_render_dot() {
        // (line_clock, raw render dot at that clock, Mesen2-readable H position)
        let cases: &[(u16, u16, u16)] = &[
            (100, 25, 25),    // well below the long-dot region: no correction
            (1292, 323, 323), // first threshold boundary, still exact
            (1310, 327, 327), // second threshold boundary, still exact
            (1340, 335, 334), // #3120's actual test1 residual: raw dot 335, readable 334
            (1400, 350, 349), // settled -1 offset persists well past the correction region
        ];
        for &(line_clock, raw_dot, expected_h) in cases {
            let mut ppu = Ppu::new();
            ppu.line_clock = line_clock;
            ppu.position.dot = raw_dot;
            ppu.latch_counters();
            assert_eq!(
                ppu.ophct_latch, expected_h,
                "line_clock={line_clock}: expected readable H position {expected_h}"
            );
        }
    }

    /// #3120: on a `Short` profile scanline (NTSC scanline 240, interlace field 1) NESER's own
    /// render-dot counter skips the 323/327 widening entirely (see
    /// `ntsc_short_scanline_has_no_extra_cycles_at_dot_327` below), so `position.dot` already
    /// equals `line_clock/4` exactly with no divergence. The readable H position must still
    /// apply Mesen2's correction here: `SnesPpu::GetCycle()` is an unconditional function of
    /// `hClock` alone (no scanline-type check anywhere in its source), so a real ROM reading
    /// SLHV on this specific scanline still needs `readable_h_position`'s "-4" adjustment even
    /// though `position.dot` is already the "true" value NESER's own model would render.
    #[test]
    fn latch_counters_applies_the_correction_on_a_short_profile_scanline_too() {
        let mut ppu = Ppu::new();
        ppu.line_timing_profile = PpuLineTimingProfile::Short;
        ppu.line_clock = 1340;
        ppu.position.dot = 335; // no widening on Short: raw dot is exact here
        ppu.latch_counters();
        assert_eq!(
            ppu.ophct_latch, 334,
            "the readable H correction is unconditional on hClock, not gated by scanline profile"
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

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32);

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

        // OPHCT read-twice: low byte, then high bit (bit 8 = 0 here) OR'd with bits 1-7 of
        // PPU2 open bus -- which the low-byte read we just did left sitting at 20 (0x14),
        // so the high read echoes `20 & 0xFE` = 20 rather than a bare 0. See
        // `ophct_read_flipflop_keeps_alternating_across_a_relatch_without_a_stat78_read` for
        // why this open-bus echo matters.
        assert_eq!(ppu.read_register(0x213C), 20);
        assert_eq!(ppu.read_register(0x213C), 20);
        // OPVCT read-twice: low byte (scanline 0) leaves PPU2 open bus at 0, so the high
        // read's `0 & 0xFE` echo is also 0.
        assert_eq!(ppu.read_register(0x213D), 0);
        assert_eq!(ppu.read_register(0x213D), 0);
    }

    #[test]
    fn slhv_latches_a_nonzero_scanline() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 3 + 5);

        ppu.read_register(0x2137);

        // High-byte reads echo bits 1-7 of PPU2 open bus, i.e. the preceding low-byte read's
        // own value masked with 0xFE (bit 8 is 0 for both H=5 and V=3, so it doesn't show).
        assert_eq!(ppu.read_register(0x213C), 5);
        assert_eq!(ppu.read_register(0x213C), 5 & 0xFE);
        assert_eq!(ppu.read_register(0x213D), 3);
        assert_eq!(ppu.read_register(0x213D), 3 & 0xFE);
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
    fn ophct_read_flipflop_keeps_alternating_across_a_relatch_without_a_stat78_read() {
        // Per the Nocash SNES spec ("the flipflops aren't automatically reset when latching
        // occurs") and bsnes/Mesen2 (`ophct_byte = ~ophct_byte` on every $213C read), the
        // OPHCT/OPVCT read flip-flop is NOT reset by a fresh SLHV/WRIO latch -- only a STAT78
        // ($213F) read does that. But the flip-flop itself unconditionally *toggles* on every
        // $213C/$213D read, regardless of any intervening latch: it alternates low, high, low,
        // high, ... rather than sticking to "high" forever after the first read.
        //
        // The "high" read's bits 1-7 are PPU2 open bus rather than 0: since a $213C read
        // leaves its own return value sitting in PPU2 open bus, and H=20/25 both have bit 8
        // clear, the "high" reads below echo the preceding "low" read's byte (`& 0xFE`) --
        // this is exactly what lets a ROM that reads OPHCT once per H-IRQ firing keep getting
        // a usable (non-near-zero) value on every other firing. See #2953.
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 20);
        ppu.read_register(0x2137); // first latch: H=20

        assert_eq!(
            ppu.read_register(0x213C),
            20,
            "first read returns the low byte"
        );
        assert_eq!(
            ppu.read_register(0x213C),
            20 & 0xFE,
            "second read returns the high bit (0, since H=20 < 256) OR'd with PPU2 open \
             bus, which the first read left at 20"
        );

        tick_dots(&mut ppu, 5);
        ppu.read_register(0x2137); // second latch, no STAT78 read in between: H=25

        assert_eq!(
            ppu.read_register(0x213C),
            25,
            "third read (no intervening STAT78 read) toggles back to the low byte of \
             the new latch, since the flip-flop toggles on every read regardless of \
             intervening latches"
        );
        assert_eq!(
            ppu.read_register(0x213C),
            25 & 0xFE,
            "fourth read toggles back to the high bit, again echoing the preceding low \
             read (25) through PPU2 open bus"
        );
    }

    #[test]
    fn stat78_reports_ppu2_version_3() {
        // Real consoles overwhelmingly ship 5C78 revision 3: both Mesen2
        // (hardcoded 0x03 in SnesPpu.cpp) and ares (PPU2 Version default 3)
        // report version 3, and byuu's test_oam displays STA78=43 on them.
        let mut ppu = Ppu::new();
        assert_eq!(ppu.read_register(0x213F) & 0x0F, 3);
    }

    #[test]
    fn stat78_frame_rate_bit_reports_the_video_region() {
        // fullsnes "213Fh - STAT78": bit 4 is "Frame Rate (PPU2.Pin30)
        // (0=NTSC/60Hz, 1=PAL/50Hz)". ares agrees (sfc/ppu/io.cpp:
        // `ppu2.mdr.bit(4) = Region::PAL()`), as does Mesen2 (SnesPpu.cpp
        // ORs 0x10 when the console region is PAL). This is the only way a
        // ROM can discover at runtime which console it is running on.
        let mut ntsc = Ppu::new_with_region(SnesVideoRegion::Ntsc);
        let mut pal = Ppu::new_with_region(SnesVideoRegion::Pal);

        assert_eq!(ntsc.read_register(0x213F) & 0x10, 0x00);
        assert_eq!(pal.read_register(0x213F) & 0x10, 0x10);
    }

    #[test]
    fn stat78_frame_rate_bit_does_not_disturb_the_other_fields() {
        let mut pal = Ppu::new_with_region(SnesVideoRegion::Pal);
        pal.read_register(0x2137); // latch (WRIO bit7 set on reset)

        let status = pal.read_register(0x213F);

        assert_eq!(status & 0x0F, 3, "PPU2 version must still read 3");
        assert_ne!(status & 0x40, 0, "latch flag must still be reported");
        assert_eq!(status & 0x10, 0x10, "PAL frame-rate bit must be set");
    }

    /// fullsnes "213Fh - STAT78" bit 5: "Not used (PPU2 open bus) (same as
    /// last value read from PPU2)". Mesen2 builds the same value with
    /// `(_state.Ppu2OpenBus & 0x20)` ORed in. Building STAT78 without an
    /// open-bus term and then latching the result back into PPU2 open bus
    /// pins bit 5 to 0 forever, which also corrupts the following
    /// OPHCT/OPVCT high-byte reads that echo open bus.
    #[test]
    fn stat78_bit_5_echoes_the_last_value_read_from_ppu2() {
        let mut ppu = Ppu::new();

        // Latch at H=32 (0x20, bit 5 set) and read OPHCT's low byte, which
        // leaves 0x20 in PPU2 open bus.
        tick_dots(&mut ppu, 32);
        ppu.read_register(0x2137);
        assert_eq!(ppu.read_register(0x213C), 32);

        assert_eq!(
            ppu.read_register(0x213F) & 0x20,
            0x20,
            "STAT78 bit 5 must echo the 0x20 the OPHCT read left in PPU2 open bus"
        );

        // Now leave a value WITHOUT bit 5 in open bus and confirm it follows.
        tick_dots(&mut ppu, 40); // H=72 (0x48): bit 5 clear
        ppu.read_register(0x2137);
        let low = ppu.read_register(0x213C);
        assert_eq!(low, 72);
        assert_eq!(low & 0x20, 0x00, "H={low} should have bit 5 clear");

        assert_eq!(
            ppu.read_register(0x213F) & 0x20,
            0x00,
            "STAT78 bit 5 must follow open bus down as well as up"
        );
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
        // The RDNMI flag rises at intra-line clock 2 and the CPU NMI line at
        // clock 6 (anomie H=0.5; Mesen2 ProcessIrqCounters) -- see the #2990
        // tests below for the exact edges.
        tick_cycles(&mut ppu, 6);

        assert!(ppu.in_vblank());
        assert_eq!(
            ppu.poll_nmi(),
            1,
            "an NMI edge with the normal 1-cycle arm should be delivered at VBlank entry"
        );
        assert_eq!(ppu.poll_nmi(), 0, "the edge is consumed only once");
        assert_ne!(ppu.read_register(0x4210) & 0x80, 0, "RDNMI flag is set");
    }

    #[test]
    fn vblank_flag_is_set_even_when_nmi_disabled_without_an_edge() {
        let mut ppu = Ppu::new();

        tick_to_vblank(&mut ppu);
        tick_cycles(&mut ppu, 6); // past the clock-2 flag rise

        assert_eq!(ppu.poll_nmi(), 0, "no edge while NMI is disabled");
        assert_ne!(ppu.read_register(0x4210) & 0x80, 0, "RDNMI flag still set");
    }

    #[test]
    fn enabling_nmi_during_vblank_raises_an_edge_with_a_two_cycle_arm() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        tick_cycles(&mut ppu, 6); // past the clock-2 flag rise
        assert_eq!(ppu.poll_nmi(), 0);

        ppu.write_register(0x4200, 0x80); // enable mid-VBlank while the flag is set

        assert_eq!(
            ppu.poll_nmi(),
            2,
            "an edge raised by the NMITIMEN write carries Mesen2's SetNmiFlag(2) \
             two-cycle arm, so a wide $4200 store's NMI still lets the following \
             instruction complete (byuu test_nmi v1.1 test 27, #3081)"
        );
    }

    /// The 2-cycle arm applies only to a disabled->enabled NMITIMEN transition
    /// (Mesen2: `if(_nmiFlag && enableNmi && !_state.EnableNmi) SetNmiFlag(2)`).
    /// A REWRITE with NMI already enabled that lands between the flag rise
    /// (clock 2) and the PPU's own edge evaluation (clock 6) discovers the
    /// vblank rise itself, and that edge must carry the normal 1-cycle arm --
    /// it is the vblank edge, not an enable-raised one (#3081).
    #[test]
    fn a_nmitimen_rewrite_discovering_the_vblank_rise_arms_one_cycle() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4200, 0x80); // enabled well before vblank
        tick_to_vblank(&mut ppu);
        tick_cycles(&mut ppu, 3); // flag is up (clock 2), edge not yet evaluated (clock 6)

        ppu.write_register(0x4200, 0x80); // rewrite, enable bit unchanged

        assert_eq!(
            ppu.poll_nmi(),
            1,
            "an edge discovered by a $4200 rewrite with NMI already enabled is \
             the vblank edge and carries the normal 1-cycle arm"
        );
    }

    #[test]
    fn rdnmi_read_acknowledges_the_flag_and_reports_cpu_version() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        tick_cycles(&mut ppu, 6); // past the clock 2-5 read-hold window

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

        // Advance through the rest of VBlank back to scanline 0. The flag
        // falls at intra-line clock 2 of scanline 0 (Mesen2 ProcessIrqCounters).
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * (262 - 225));
        tick_cycles(&mut ppu, 2);

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

    // The HBLANK flag rises one master clock INTO dot 274, not on the dot
    // boundary: hardware reports HBLANK for hClock outside [4, 1096]
    // (Mesen InternalRegisters $4212: `hClock >= 1*4 && hClock <= 274*4`
    // reads as not-HBLANK). blargg's SPC-APU shell counts $4212 polls
    // inside the HBLANK window; the one-clock edge position is observable
    // (#2914 timer_at_power_reset).
    #[test]
    fn hblank_flag_rises_one_clock_into_dot_274() {
        let mut ppu = Ppu::new();
        // Dots 0-273 are 4 clocks each: hClock 1096 is dot 274's first clock.
        tick_cycles(&mut ppu, 1096);
        assert_eq!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "hClock 1096 (dot 274 boundary) must not yet report HBlank"
        );
        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4212) & 0x40,
            0,
            "hClock 1097 must report HBlank"
        );
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
    fn h_irq_fires_at_the_hardware_clock_offset() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x01); // HTIME = 1
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10); // H-IRQ enable
        // Power-on artifact (shared with Mesen2): the circuit's H counter
        // starts at 0 and its clock-2 increment makes it 1, so HTIME=1 fires
        // once spuriously on scanline 0. Cross into scanline 1 and acknowledge
        // before measuring the steady-state position.
        tick_cycles(&mut ppu, 1364);
        ppu.read_register(0x4211);
        // The circuit's H counter reaches HTIME=1 on the tick at clock 14 and
        // the TIMEUP flag sets one 4-clock tick later, at HTIME*4 + 14 = 18
        // (byuu test_irq.asm: "$4211.d7 is set at H=(HTIME)?(HTIME*4+14):(10)").
        tick_cycles(&mut ppu, 17);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "H-IRQ must not set TIMEUP before clock HTIME*4 + 14"
        );
        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "H-IRQ sets TIMEUP at clock HTIME*4 + 14"
        );
    }

    #[test]
    fn h_irq_sets_timeup_and_a_4211_read_acknowledges_after_the_hold_window() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x01);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);

        // Cross the power-on-artifact fire on scanline 0 (see
        // `h_irq_fires_at_the_hardware_clock_offset`) and acknowledge it.
        tick_cycles(&mut ppu, 1364);
        ppu.read_register(0x4211);

        // HTIME=1 sets TIMEUP at clock 18; reads before the countdown expires
        // at clock 22 (where the CPU line rises) must not acknowledge (byuu
        // test_irq.asm sub-tests 6-7; Mesen2 `_needIrq` hold).
        tick_cycles(&mut ppu, 18);
        let at_rise = ppu.read_register(0x4211);
        let still_held = ppu.read_register(0x4211);
        assert_ne!(at_rise & 0x80, 0, "TIMEUP is set at the H-IRQ point");
        assert_ne!(
            still_held & 0x80,
            0,
            "a read inside the 4-clock hold window must not acknowledge"
        );

        tick_cycles(&mut ppu, 4);
        let after_hold = ppu.read_register(0x4211);
        let acknowledged = ppu.read_register(0x4211);
        assert_ne!(after_hold & 0x80, 0, "TIMEUP still set at clock 22");
        assert_eq!(
            acknowledged & 0x80,
            0,
            "the clock-22 read acknowledges TIMEUP"
        );
    }

    #[test]
    fn h_irq_triggers_on_every_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x02);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);

        // HTIME=2 sets TIMEUP at clock 2*4 + 14 = 22 (no power-on artifact:
        // the clock-2 increment only reaches H counter 1).
        tick_cycles(&mut ppu, 22);
        assert_ne!(ppu.read_register(0x4211) & 0x80, 0, "line 0 trigger");

        // Acknowledge once the hold window has passed, so the next line's
        // assertion observes a fresh trigger rather than the stale flag.
        tick_cycles(&mut ppu, 4);
        ppu.read_register(0x4211);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "acknowledged between lines"
        );

        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "line 1 trigger at same H position"
        );
    }

    #[test]
    fn h_irq_with_late_htime_fires_at_clock_6_via_the_wrapping_h_counter() {
        // HTIME = 339 (0x153): the circuit's H counter only reaches 339 on the
        // increment tick at clock 2 of the *following* scanline (it tops out
        // at 338 on the line itself), so the level edge lands there and TIMEUP
        // sets one tick later, at clock 6. On scanline 0 from power-on the
        // counter never gets past 338, so nothing fires until line 1. The
        // frame-origin wrap of this compare (line 261 -> line 0) does fire --
        // pinned by `irq::tests::h_irq_with_htime_339_also_fires_when_wrapping_
        // into_the_frame_origin`. This late-HTIME setup is the case that
        // previously hung the SPC700 timing ROMs.
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x53); // HTIME low
        ppu.write_register(0x4208, 0x01); // HTIME high => 0x153 = 339
        ppu.write_register(0x4200, 0x10); // H-IRQ enable

        // Advance through scanline 0 and into scanline 1 (dot 0, intra-line clock 0).
        tick_cycles(&mut ppu, 1364);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "nothing fires on scanline 0: the H counter never reaches 339 there"
        );

        // Six clocks into scanline 1 the countdown from the clock-2 edge sets TIMEUP.
        tick_cycles(&mut ppu, 6);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "H-IRQ sets TIMEUP at clock 6 of the following scanline"
        );
    }

    #[test]
    fn disabling_irq_mode_clears_timeup_flag() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x01);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10);
        // Cross the power-on-artifact fire on scanline 0 (see
        // `h_irq_fires_at_the_hardware_clock_offset`) and acknowledge it.
        tick_cycles(&mut ppu, 1364);
        ppu.read_register(0x4211);
        tick_cycles(&mut ppu, 18); // HTIME=1 sets TIMEUP at clock 18
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

        // The circuit's V counter increments to VTIME on the clock-6 tick of
        // the matching line; the countdown sets TIMEUP one tick later, at
        // clock 10.
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 2);
        tick_cycles(&mut ppu, 10);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "V-IRQ should trigger at VTIME"
        );

        // That clock-10 read landed inside the 4-clock hold window, so it did
        // not acknowledge; a read from clock 14 onward does.
        tick_cycles(&mut ppu, 4);
        ppu.read_register(0x4211);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "the clock-14 read acknowledges"
        );

        // The compare level stays high for the whole matching line but only
        // its rising edge fires: no retrigger after the acknowledge.
        tick_dots(&mut ppu, 20);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "V-IRQ must not retrigger later on the same line"
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

        // HV IRQ fires at clock (5+1)*4 + 10 = 34 of the VTIME=3 scanline.
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 3);
        tick_cycles(&mut ppu, 33);
        assert_eq!(ppu.read_register(0x4211) & 0x80, 0);

        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "HV IRQ should trigger at the programmed H position on the programmed V line"
        );
    }

    #[test]
    fn hv_irq_with_htime_zero_triggers_at_clock_14_of_vtime_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x00);
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4209, 0x04);
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x30);

        // HTIME=0: the level edge lands on the clock-6 tick (H counter reset
        // and V increment coincide), which arms the *3*-tick countdown --
        // Mesen2's "IRQs for H=0 are delayed by an extra tick" note -- so
        // TIMEUP sets at clock 14 and the CPU line rises at clock 18 of the
        // VTIME line. byuu's test_irq.asm header claims d7 at H=10 for
        // HTIME=0, but no ROM sub-test probes it (sub-tests 1-2 are V-only,
        // 3-4 use HTIME=1); Mesen2, which passes the entire KungFuFurby IRQ
        // suite, gives 14/18 (#3144).
        tick_dots(&mut ppu, DOTS_PER_SCANLINE as u32 * 4);
        tick_cycles(&mut ppu, 13);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "the 3-tick countdown must not set TIMEUP before clock 14"
        );
        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "HTIME=0 HV mode sets TIMEUP at clock 14 of the VTIME line"
        );
        assert!(
            !ppu.poll_irq_dispatch(),
            "the CPU line lags TIMEUP by one circuit tick"
        );
        tick_cycles(&mut ppu, 4);
        assert!(
            ppu.poll_irq_dispatch(),
            "the CPU line rises at clock 18 of the VTIME line"
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
    fn ntsc_interlace_even_field_frame_is_263_scanlines() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x01); // screen interlace
        ppu.interlace_field = true;
        tick_scanlines(&mut ppu, 262); // first (power-on, unlatched) frame

        // The wrap toggled to the even field and latched the long frame (Mesen2
        // UpdateNmiScanline: extra scanline when ScreenInterlace && !oddFrame).
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
        assert!(!ppu.interlace_field, "wrap toggled to the even field");

        tick_scanlines(&mut ppu, 262);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 262,
                dot: 0
            },
            "the even field has an extra scanline 262"
        );
        tick_scanlines(&mut ppu, 1);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
        assert!(ppu.interlace_field, "long frame wrapped into the odd field");
    }

    #[test]
    fn ntsc_interlace_odd_field_frame_is_262_scanlines() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = false;
        tick_scanlines(&mut ppu, 262); // unlatched power-on frame; wrap -> odd field

        assert!(ppu.interlace_field, "wrap toggled to the odd field");
        tick_scanlines(&mut ppu, 262);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            },
            "the odd field keeps the normal 262-scanline length"
        );
    }

    #[test]
    fn pal_interlace_even_field_frame_is_313_scanlines() {
        let mut ppu = Ppu::new_with_region(SnesVideoRegion::Pal);
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = true;
        // First (power-on, unlatched) frame runs on the odd field, whose line 311
        // is the Long 1368-cycle line: spend the 4 extra cycles explicitly.
        tick_scanlines(&mut ppu, 312);
        tick_cycles(&mut ppu, 4);

        assert!(!ppu.interlace_field);
        // Even-field lines are all Normal (the PAL Long exception needs field 1),
        // so plain scanline ticking is cycle-exact here.
        tick_scanlines(&mut ppu, 312);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 312,
                dot: 0
            },
            "the even field has an extra scanline 312"
        );
        tick_scanlines(&mut ppu, 1);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
    }

    #[test]
    fn interlace_enabled_mid_frame_does_not_extend_the_current_frame() {
        let mut ppu = Ppu::new();
        tick_scanlines(&mut ppu, 100);
        ppu.write_register(0x2133, 0x01); // enable mid-frame
        tick_scanlines(&mut ppu, 162);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            },
            "the in-progress frame keeps its latched 262 scanlines"
        );

        // Power-on field was even -> this wrap enters the odd field: still 262.
        tick_scanlines(&mut ppu, 262);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
        // Next wrap enters the even field: the long frame latches now.
        tick_scanlines(&mut ppu, 262);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 262,
                dot: 0
            },
            "the first even-field frame after enabling is 263 scanlines"
        );
    }

    #[test]
    fn interlace_disabled_mid_frame_keeps_the_latched_263() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = true;
        tick_scanlines(&mut ppu, 262); // wrap -> even field, long frame latched

        tick_scanlines(&mut ppu, 10);
        ppu.write_register(0x2133, 0x00); // disable mid-frame
        tick_scanlines(&mut ppu, 252);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 262,
                dot: 0
            },
            "the latched long frame keeps its extra scanline"
        );
        tick_scanlines(&mut ppu, 1);
        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 0
            }
        );
    }

    #[test]
    fn save_state_round_trips_the_latched_extra_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x01);
        ppu.interlace_field = true;
        tick_scanlines(&mut ppu, 262); // wrap -> even field, long frame latched
        tick_scanlines(&mut ppu, 100); // mid-frame

        let state = ppu.capture_state();
        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");

        tick_scanlines(&mut ppu, 162);
        tick_scanlines(&mut restored, 162);
        for p in [&ppu, &restored] {
            assert_eq!(
                p.position(),
                ScanPosition {
                    scanline: 262,
                    dot: 0
                },
                "the latched extra scanline survives capture/restore"
            );
        }
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

    // --- #2990: every-vblank frame counting + RDNMI hold window ---
    //
    // Frame counting: every vblank entry must be observable even when the CPU
    // does not drain `take_completed_frames` between them (a >1-frame DMA
    // previously collapsed multiple vblanks into one bool flag).
    //
    // RDNMI timing (anomie timing.txt INTERRUPTS: the internal timer asserts
    // its NMI output at H=0.5 of the first vblank line; Mesen2
    // InternalRegisters: flag set at hclock 2, CPU NMI line raised at hclock
    // 6, and a $4210 read during hclock 2-5 of that line returns the flag set
    // WITHOUT acknowledging it -- hardware-verified via Terranigma).

    #[test]
    fn vblank_entries_accumulate_when_not_drained() {
        let mut ppu = Ppu::new();

        // Two full vblank entries without draining in between.
        tick_scanlines(&mut ppu, 225);
        tick_scanlines(&mut ppu, 262);

        assert_eq!(
            ppu.take_completed_frames(),
            2,
            "both vblank entries must be reported even without an interim drain"
        );
        assert_eq!(ppu.take_completed_frames(), 0, "drained");
    }

    #[test]
    fn rdnmi_flag_rises_at_hclock_2_of_the_vblank_scanline() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu); // scanline 225, intra-line clock 0

        assert!(!ppu.nmi_flag, "flag not yet set at hclock 0");
        tick_cycles(&mut ppu, 1);
        assert!(!ppu.nmi_flag, "flag not yet set at hclock 1");
        tick_cycles(&mut ppu, 1);
        assert!(ppu.nmi_flag, "flag set at hclock 2 (anomie H=0.5)");
    }

    #[test]
    fn rdnmi_read_in_the_hold_window_does_not_acknowledge() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        tick_cycles(&mut ppu, 3); // hclock 3: inside the 2-5 hold window

        let first = ppu.read_register(0x4210);
        let second = ppu.read_register(0x4210);
        assert_ne!(first & 0x80, 0, "read in the hold window returns the flag");
        assert_ne!(
            second & 0x80,
            0,
            "the hold-window read must NOT acknowledge the flag"
        );

        tick_cycles(&mut ppu, 3); // hclock 6: hold window over
        let third = ppu.read_register(0x4210);
        let fourth = ppu.read_register(0x4210);
        assert_ne!(third & 0x80, 0, "flag still set before acknowledge");
        assert_eq!(fourth & 0x80, 0, "read at hclock >= 6 acknowledges");
    }

    #[test]
    fn nmi_edge_is_delivered_at_hclock_6_of_the_vblank_scanline() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4200, 0x80); // enable VBlank NMI
        tick_to_vblank(&mut ppu);

        tick_cycles(&mut ppu, 5);
        assert_eq!(ppu.poll_nmi(), 0, "no CPU NMI edge before hclock 6");
        tick_cycles(&mut ppu, 1);
        assert_eq!(ppu.poll_nmi(), 1, "CPU NMI edge raised at hclock 6");
    }

    #[test]
    fn rdnmi_flag_clears_at_hclock_2_of_scanline_0() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        tick_scanlines(&mut ppu, 262 - 225); // wrap to scanline 0, hclock 0

        assert!(ppu.nmi_flag, "flag still set at hclock 0-1 of scanline 0");
        tick_cycles(&mut ppu, 2);
        assert!(!ppu.nmi_flag, "flag cleared at hclock 2 of scanline 0");
    }

    /// Pins the whole `$4210` read-hold rule against Mesen2's own formulation
    /// (`InternalRegisters.cpp` `Read` $4210): *clear iff `_nmiFlag &&
    /// (hClock >= 6 || scanline != nmiScanline)`*. NESER expresses it the other
    /// way round -- hold iff `scanline == vblank_start_line() &&
    /// (2..6).contains(&line_clock)` -- and the two differ only at clocks 0-1 of
    /// the NMI scanline, where the flag has not risen yet so there is nothing to
    /// acknowledge under either rule. Walking every clock of the window turns
    /// that argument into checked facts rather than a claim in a comment: the
    /// clock 0-1 assertions pin its *premise* (neither rule is observable there,
    /// so they cannot be told apart), and the rest pin the behaviour the two
    /// rules agree on (#3145).
    #[test]
    fn rdnmi_hold_window_matches_mesen2s_hclock_rule() {
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu); // scanline 225, intra-line clock 0

        // Clocks 0-1: the flag rises at clock 2, so NESER's extra lower bound on
        // the window is unobservable -- both rules read back an empty bit 7.
        // This pins the premise, not a difference: widening the window to
        // (0..6) would leave these two assertions green, which is the point.
        for clock in 0..2 {
            assert_eq!(
                ppu.read_register(0x4210) & 0x80,
                0,
                "clock {clock} of the NMI scanline is before the flag rises"
            );
            tick_cycles(&mut ppu, 1);
        }

        // Clocks 2-5: the flag is up and the CPU forces it to stay up, so no
        // read acknowledges (Mesen2's `hClock >= 6` guard).
        for clock in 2..6 {
            assert_ne!(
                ppu.read_register(0x4210) & 0x80,
                0,
                "the flag is set at clock {clock}"
            );
            assert_ne!(
                ppu.read_register(0x4210) & 0x80,
                0,
                "the read at clock {clock} must not acknowledge"
            );
            tick_cycles(&mut ppu, 1);
        }

        // Clock 6: the CPU NMI line is raised and reads acknowledge again.
        assert_ne!(
            ppu.read_register(0x4210) & 0x80,
            0,
            "the flag is still set at clock 6"
        );
        assert_eq!(
            ppu.read_register(0x4210) & 0x80,
            0,
            "the read at clock 6 acknowledges"
        );

        // Off the NMI scanline the window does not apply at any clock: the same
        // clock 3 that held one line earlier acknowledges here.
        let mut ppu = Ppu::new();
        tick_to_vblank(&mut ppu);
        tick_scanlines(&mut ppu, 1);
        tick_cycles(&mut ppu, 3); // scanline 226, intra-line clock 3

        assert_ne!(
            ppu.read_register(0x4210) & 0x80,
            0,
            "the flag stays set past the NMI scanline"
        );
        assert_eq!(
            ppu.read_register(0x4210) & 0x80,
            0,
            "a clock-3 read off the NMI scanline acknowledges"
        );
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
