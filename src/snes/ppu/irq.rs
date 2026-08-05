//! The S-CPU interrupt counter circuit (issues #3144 and #3145, parent #3093).
//!
//! A line-for-line port of Mesen2's schematic-derived circuit
//! (`Core/SNES/InternalRegisters.h:81-162` for `UpdateIrqLevel` /
//! `ProcessIrqCounters`, `InternalRegisters.cpp:180-187` for `SetIrqFlag`;
//! schematics: <https://github.com/rgalland/SNES_S-CPU_Schematics/>). It runs at
//! master-clock/4 with the signal inverted, i.e. it ticks at intra-line clocks
//! 2, 6, 10, ... and drives *both* CPU interrupt signals, exactly as the
//! hardware does -- the H/V-IRQ (#3144) and, since #3145, the VBlank NMI. One
//! circuit, one derivation of the line clocks: the NMI half used to re-derive
//! clocks 2 and 6 for itself in the old `Ppu::evaluate_nmi_flag_events`
//! (removed by #3145).
//!
//! The circuit keeps its own H/V counters (distinct from the render counters):
//! H resets at clocks 6 and 10 and then counts 4-clock ticks, V increments at
//! clock 6 of every scanline but 0 and resets at clock 2 of scanline 0. A
//! continuous compare *level* is derived from them, and the rising edge of that
//! level arms a countdown (`need_irq`) that first sets the TIMEUP flag and one
//! tick later raises the CPU-facing IRQ line.
//!
//! The IRQ half replaces the bsnes-style single-instant point compare (the old
//! `evaluate_hv_irq`): the level+edge model is what byuu's `test_irq.asm` /
//! `test_irq4200.asm` ROMs measure -- $4211 reads within 4 clocks of the flag
//! rising do not acknowledge, register writes re-evaluate the level, and
//! enabling a mode whose compare already holds fires an IRQ.
//!
//! The tests below pin the IRQ semantics; the clock numbers cited in each test
//! are derived from Mesen2's model and byuu's `test_irq.asm` header (vendored
//! under `roms/snes/automated_tests/snes_test_roms/`). The NMI clock events are
//! pinned by the sub-scanline group in `ppu/timing.rs`'s tests.

use super::Ppu;
use crate::trace_ppu;

impl Ppu {
    /// One 4-clock tick of the interrupt counter circuit, called from
    /// `Ppu::tick_one_clock` whenever `line_clock & 3 == 2` (Mesen2
    /// `InternalRegisters::ProcessIrqCounters`).
    ///
    /// Order matters and matches Mesen2: the `need_irq` countdown resolves
    /// first (against the *previous* tick's level edge), then the counters and
    /// the NMI events, then the IRQ level is re-derived. DRAM-refresh stolen
    /// clocks tick through here like any others, matching Mesen2's
    /// `IncMasterClock40`.
    ///
    /// The "IRQs can't trigger on V=261/H=339" hardware note is emergent, not
    /// coded: the V counter resets to 0 at clock 2 of scanline 0, the same
    /// tick the H counter reaches 339, so that HV conjunction is never
    /// observable.
    ///
    /// # The NMI half (#3145)
    ///
    /// The RDNMI ($4210) vblank flag rises at clock 2 of the first vblank
    /// scanline (anomie timing.txt INTERRUPTS: "the internal timer will set its
    /// NMI output low at H=0.5") and falls at clock 2 of scanline 0. The
    /// CPU-facing NMI line follows 4 clocks later, at clock 6. That gap is the
    /// $4210 read-hold window: a read landing in it returns the flag set
    /// without acknowledging it (see `Ppu::read_register`), which is what lets
    /// a tight RDNMI poll loop observe the same vblank twice.
    ///
    /// Two structural differences from Mesen2 remain. Both are pre-existing
    /// NESER behaviour that the merge carried over unchanged, not anything it
    /// introduced, and no ROM or measurement currently distinguishes either --
    /// left alone deliberately, not overlooked.
    ///
    /// - **Where the edge is latched.** Mesen2 arms the CPU line here at clock
    ///   6 with an unconditional `SetNmiFlag(1)`; NESER goes through
    ///   [`Ppu::update_nmi_line`], which latches an edge only when the level
    ///   `nmi_enable && nmi_flag` is *rising*. That level rises at clock 2,
    ///   with the flag -- but nothing *samples* it there, since the clock-2 arm
    ///   does not call `update_nmi_line`. In the common case clock 6 is the
    ///   first sample and finds the rise, so the two agree; any non-disabling
    ///   `$4200` write landing in clocks 2-5 samples it earlier, and clock 6
    ///   then sees a level that is no longer rising and does nothing. An
    ///   enabling write keeps its 2-cycle recognition arm (#3081)
    ///   where Mesen2 re-arms to 1 at clock 6; a rewrite with NMI already
    ///   enabled latches at the write where Mesen2 latches at clock 6 (see
    ///   `a_nmitimen_rewrite_discovering_the_vblank_rise_arms_one_cycle`).
    /// - **How the NMI scanline is derived.** Mesen2 compares against
    ///   `_nmiScanline`, latched once per frame (`SnesPpu::UpdateNmiScanline`,
    ///   called from `ProcessEndOfScanline` at scanline 0); NESER derives it
    ///   live from [`Ppu::vblank_start_line`], which reads SETINI ($2133 bit 2)
    ///   as it currently stands. So *any* overscan toggle after a frame's
    ///   scanline-0 boundary moves NESER's NMI scanline for that frame and not
    ///   Mesen2's. Because `Ppu::read_register`'s $4210 hold window is derived
    ///   the same way, a toggle between clocks 2 and 6 also takes that window
    ///   away mid-line, letting a read clear `nmi_flag` before the CPU line
    ///   would have been raised.
    pub(super) fn process_irq_counters(&mut self) {
        if self.need_irq > 0 {
            self.need_irq -= 1;
            if self.need_irq == 1 {
                self.set_irq_flag(true);
            } else if self.need_irq == 0 {
                // Propagate the flag (possibly cleared meanwhile by a $4211
                // read or $4200 disable) to the CPU-facing line.
                self.irq_line = self.timeup_flag;
            }
        }

        match self.line_clock {
            2 => {
                self.hv_h_counter += 1;
                // Fused exactly as Mesen2 fuses them. The `else if` can only
                // shadow the V-counter reset if the vblank line were 0, and
                // `vblank_start_line()` is 225 or 240.
                if self.position.scanline == self.vblank_start_line() {
                    self.nmi_flag = true;
                } else if self.position.scanline == 0 {
                    self.nmi_flag = false;
                    self.update_nmi_line();
                    self.hv_v_counter = 0;
                }
            }
            6 => {
                self.hv_h_counter = 0;
                if self.position.scanline > 0 {
                    self.hv_v_counter += 1;
                }
                if self.position.scanline == self.vblank_start_line() {
                    self.update_nmi_line();
                }
            }
            10 => self.hv_h_counter = 0,
            _ => self.hv_h_counter += 1,
        }

        self.update_irq_level();
    }

    /// Re-derive the compare level and arm the edge countdown (Mesen2
    /// `InternalRegisters::UpdateIrqLevel`). Called from every circuit tick
    /// and directly from the `$4207-$420A` write handlers, so a timer rewrite
    /// that lands between two ticks raises the level at write time (Mesen2's
    /// `$4209` Shin Nihon Pro Wrestling fix). `$4200` enables are *not*
    /// re-evaluated at write time -- the next circuit tick picks them up.
    ///
    /// The rising edge arms `need_irq = 2`, or 3 when an H-enabled edge lands
    /// on the clock-6 tick: H-IRQs that depend on the H counter's clock-6
    /// reset (the HTIME=0 case) are delayed one extra tick (Mesen2's "IRQs for
    /// H=0 seem to be delayed by an extra tick" note).
    pub(super) fn update_irq_level(&mut self) {
        let level = self.compute_irq_level();

        if !self.irq_level && level {
            self.need_irq = if self.irq_mode & 0x01 != 0 && self.line_clock == 6 {
                3
            } else {
                2
            };
        }

        self.irq_level = level;
    }

    /// The compare level the circuit would derive from the current counters
    /// and timer targets, without touching the edge state. Split out so
    /// save-state restore can seed [`Ppu::irq_level`] for legacy states
    /// without arming a spurious edge.
    pub(super) fn compute_irq_level(&self) -> bool {
        if self.irq_mode == 0 {
            return false;
        }
        let h_enabled = self.irq_mode & 0x01 != 0;
        let v_enabled = self.irq_mode & 0x02 != 0;
        (!h_enabled || self.htime == self.hv_h_counter)
            && (!v_enabled || self.vtime == self.hv_v_counter)
    }

    /// Set or clear the TIMEUP flag (Mesen2 `InternalRegisters::SetIrqFlag`):
    /// the flag only ever sets while an IRQ mode is enabled, and clearing it
    /// also drops the CPU-facing line.
    pub(super) fn set_irq_flag(&mut self, flag: bool) {
        let new_flag = flag && self.irq_mode != 0;
        if new_flag && !self.timeup_flag {
            trace_ppu!(2; "timeup y={} lc={} irq_mode={} htime={:03X} vtime={:03X}",
                self.position.scanline,
                self.line_clock,
                self.irq_mode,
                self.htime,
                self.vtime,
            );
        }
        self.timeup_flag = new_flag;
        if !self.timeup_flag {
            self.irq_line = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, Ppu};

    fn tick_cycles(ppu: &mut Ppu, cycles: u32) {
        for _ in 0..cycles {
            ppu.tick();
        }
    }

    fn tick_scanlines(ppu: &mut Ppu, scanlines: u32) {
        tick_cycles(
            ppu,
            DOTS_PER_SCANLINE as u32 * MASTER_CYCLES_PER_DOT * scanlines,
        );
    }

    /// Enables a V-only IRQ for `vtime` before any clock has ticked.
    fn enable_v_irq(ppu: &mut Ppu, vtime: u8) {
        ppu.write_register(0x4209, vtime);
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x20);
    }

    /// byuu `test_irq.asm` sub-tests 6-7: reading `$4211` at the TIMEUP set
    /// instant (HC+0) or two clocks later (HC+2) leaves bit 7 set; Mesen2
    /// implements this as the `_needIrq` countdown forcing the flag set for
    /// its first 4 master clocks (`InternalRegisters.cpp` `$4211` read arm).
    ///
    /// V-only VTIME=2: the level rises at clock 6 of scanline 2 (the V counter
    /// increment tick), TIMEUP sets on the next circuit tick at clock 10, and
    /// the countdown expires at clock 14.
    #[test]
    fn timeup_read_within_four_clocks_of_the_flag_rising_does_not_acknowledge() {
        let mut ppu = Ppu::new();
        enable_v_irq(&mut ppu, 0x02);
        tick_scanlines(&mut ppu, 2);
        tick_cycles(&mut ppu, 10);

        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP is set at its rise clock (HC+0)"
        );
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "a read at HC+0 must not acknowledge TIMEUP (4-clock hold window)"
        );
        tick_cycles(&mut ppu, 2);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP is still set at HC+2 after the HC+0 reads"
        );
    }

    /// Companion to the hold-window test: from the clock the countdown expires
    /// (HC+4, where the CPU IRQ line also rises) a `$4211` read acknowledges
    /// normally. byuu `test_irq.asm` sub-test 8.
    #[test]
    fn timeup_read_from_the_cpu_line_clock_onward_acknowledges() {
        let mut ppu = Ppu::new();
        enable_v_irq(&mut ppu, 0x02);
        tick_scanlines(&mut ppu, 2);
        tick_cycles(&mut ppu, 14);

        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP is still set at HC+4 (nothing has read it yet)"
        );
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "the HC+4 read must acknowledge TIMEUP"
        );
    }

    /// `test_irq4200.asm`: a `$4200` write enabling the V-IRQ mid-scanline
    /// while V already equals VTIME fires an IRQ. In Mesen2 the enable is
    /// picked up by the next 4-clock circuit tick, whose `UpdateIrqLevel`
    /// sees the level rise and arms the 2-tick `_needIrq` countdown: writing
    /// at clock 200 means the edge lands on the tick at 202, TIMEUP sets at
    /// 206, and the CPU line rises at 210.
    #[test]
    fn enabling_v_irq_mid_scanline_while_v_matches_fires_within_two_circuit_ticks() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0x01);
        ppu.write_register(0x420A, 0x00);
        tick_scanlines(&mut ppu, 1);
        tick_cycles(&mut ppu, 200);

        ppu.write_register(0x4200, 0x20);
        tick_cycles(&mut ppu, 2); // the edge tick at clock 202 arms the countdown
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP must not be set before the countdown's second tick"
        );
        tick_cycles(&mut ppu, 4);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP sets two circuit ticks after an enable that finds V == VTIME"
        );
        tick_cycles(&mut ppu, 4);
        assert!(
            ppu.poll_irq_dispatch(),
            "the CPU IRQ line rises one circuit tick after TIMEUP"
        );
    }

    /// `test_irq4200.asm` round `(20,20,20,20)`: after an acknowledged V-IRQ,
    /// disabling and re-enabling on the same matching scanline fires again --
    /// the disable drops the level, so the re-enable is a fresh rising edge.
    #[test]
    fn re_enabling_v_irq_on_the_same_matching_line_fires_again() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0x01);
        ppu.write_register(0x420A, 0x00);
        tick_scanlines(&mut ppu, 1);
        tick_cycles(&mut ppu, 200);

        ppu.write_register(0x4200, 0x20);
        tick_cycles(&mut ppu, 14); // clock 214: countdown expired, flag + line up
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "first enable fires (and this read acknowledges it)"
        );
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "acknowledged before the re-enable"
        );

        ppu.write_register(0x4200, 0x00);
        tick_cycles(&mut ppu, 4); // clock 218: the disabled level drops
        ppu.write_register(0x4200, 0x20);
        tick_cycles(&mut ppu, 8); // edge at 222, TIMEUP at 226
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "re-enabling on the same matching line must fire a second IRQ"
        );
    }

    /// Mesen2 re-evaluates the level directly in the `$4207-$420A` write
    /// handlers (`InternalRegisters.cpp`, including the `$4209` comment citing
    /// Shin Nihon Pro Wrestling): rewriting VTIME to the currently displayed
    /// line raises the level at write time, so TIMEUP sets on the very next
    /// circuit tick and the CPU line one tick later.
    #[test]
    fn writing_vtime_to_the_current_line_mid_scanline_raises_the_level_at_write_time() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0xC8); // VTIME=200, far from the lines below
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x20);
        tick_scanlines(&mut ppu, 5);
        tick_cycles(&mut ppu, 201);

        assert_eq!(ppu.read_register(0x4211) & 0x80, 0, "nothing fired yet");
        ppu.write_register(0x4209, 0x05); // VTIME = the line currently displayed
        tick_cycles(&mut ppu, 1); // countdown tick at clock 202
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP sets on the first circuit tick after the VTIME rewrite"
        );
        tick_cycles(&mut ppu, 4); // clock 206
        assert!(
            ppu.poll_irq_dispatch(),
            "the CPU IRQ line follows one circuit tick later"
        );
    }

    /// byuu `test_irq.asm` sub-test 9 analog: a `$4200` disable landing after
    /// TIMEUP has set but before the countdown propagates the flag to the CPU
    /// line clears the flag, so the propagation tick finds it low and no IRQ
    /// dispatches. Guards the `SetIrqFlag(false)` path in the countdown.
    #[test]
    fn disabling_between_timeup_and_the_cpu_line_tick_prevents_the_dispatch() {
        let mut ppu = Ppu::new();
        enable_v_irq(&mut ppu, 0x02);
        tick_scanlines(&mut ppu, 2);
        tick_cycles(&mut ppu, 11); // TIMEUP set at 10; line would rise at 14

        ppu.write_register(0x4200, 0x00);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "disabling clears TIMEUP immediately"
        );
        tick_cycles(&mut ppu, 7); // through clock 18, past the would-be line rise
        assert!(
            !ppu.poll_irq_dispatch(),
            "the cancelled IRQ must not reach the CPU line"
        );
    }

    /// V-only VTIME=0 (Mesen2 derivation, no ROM coverage -- flagged in
    /// #3144 for trace verification): the V counter resets to 0 on the tick
    /// at clock 2 of scanline 0, so the level edge lands there, TIMEUP sets
    /// at clock 6 and the CPU line rises at clock 10 -- one circuit tick
    /// earlier than a non-zero VTIME line's 10/14.
    #[test]
    fn v_irq_with_vtime_zero_fires_at_clock_6_of_scanline_0() {
        let mut ppu = Ppu::new();
        enable_v_irq(&mut ppu, 0x00);
        tick_cycles(&mut ppu, 5);
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "TIMEUP must not be set before clock 6"
        );
        tick_cycles(&mut ppu, 1);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "VTIME=0 sets TIMEUP at clock 6 of scanline 0"
        );
        tick_cycles(&mut ppu, 4);
        assert!(
            ppu.poll_irq_dispatch(),
            "the VTIME=0 CPU line rises at clock 10"
        );
    }

    /// H-only HTIME=339 (Mesen2 derivation, no ROM coverage -- flagged in
    /// #3144 for trace verification): the H counter only reaches 339 on the
    /// increment tick at clock 2 of the *following* line, which on the last
    /// scanline wraps into the frame origin. Mesen2 fires there -- only the
    /// exact V=261/H=339 HV conjunction is impossible (the V counter resets
    /// on the same tick H reaches 339). The old bsnes-derived point compare
    /// suppressed the frame-origin match wholesale.
    #[test]
    fn h_irq_with_htime_339_also_fires_when_wrapping_into_the_frame_origin() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x53); // HTIME = 0x153 = 339
        ppu.write_register(0x4208, 0x01);
        ppu.write_register(0x4200, 0x10);

        // Settle on the frame's last scanline and acknowledge its own trigger.
        tick_scanlines(&mut ppu, 261);
        tick_cycles(&mut ppu, 1000);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "line 261 fires like any other line (and this read acknowledges)"
        );

        // Cross into scanline 0 of the next frame: H reaches 339 on the
        // clock-2 tick, TIMEUP sets on the clock-6 tick.
        tick_cycles(&mut ppu, 364 + 6);
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "the wrapped HTIME=339 match on the frame origin must fire"
        );
    }
}
