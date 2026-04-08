use crate::console::TimingMode;

/// Tracks the master clock used to derive CPU and PPU timing,
/// advancing per-CPU-cycle around each bus access.
#[derive(Debug, Clone, Copy, Default)]
pub struct MasterClock {
    master_clock: u64,
    ppu_clock: u64,
    cpu_divider: u64,
    ppu_divider: u64,
    /// Master-clock ticks consumed before the bus access within one CPU cycle.
    /// For NTSC=6, PAL=8, Dendy=7 (asymmetric: end = cpu_divider - start_clock = 8).
    start_clock: u64,
}

impl MasterClock {
    const READ_WRITE_SHIFT: u64 = 1;
    pub fn new(tv_system: TimingMode) -> Self {
        // Dividers from Mesen2 NesCpu.cpp SetMasterClockDivider():
        //   NTSC  – cpu=12, ppu=4, start=6  (PPU:CPU = 3.0, symmetric)
        //   PAL   – cpu=16, ppu=5, start=8  (PPU:CPU = 3.2, symmetric)
        //   Dendy – cpu=15, ppu=5, start=7  (PPU:CPU = 3.0, asymmetric: end=8)
        let (cpu_divider, ppu_divider, start_clock) = match tv_system {
            TimingMode::Ntsc | TimingMode::MultiRegion | TimingMode::Unknown(_) => (12, 4, 6),
            TimingMode::Pal => (16, 5, 8),
            // Dendy: master 26.601712 MHz / 15 = 1,773,448 Hz CPU; PPU:CPU = 3.0 (asymmetric split 7/8)
            TimingMode::Dendy => (15, 5, 7),
        };
        Self {
            master_clock: 0,
            ppu_clock: 0,
            cpu_divider,
            ppu_divider,
            start_clock,
        }
    }

    pub fn master_cycles(&self) -> u64 {
        self.master_clock
    }

    pub fn set_master_cycles(&mut self, cycles: u64) {
        self.master_clock = cycles;
    }

    pub fn advance_cpu_cycles(&mut self, cpu_cycles: u64) {
        self.master_clock += self.cpu_divider * cpu_cycles;
    }

    pub fn before_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock += if is_write {
            self.start_clock + Self::READ_WRITE_SHIFT
        } else {
            self.start_clock - Self::READ_WRITE_SHIFT
        };
    }

    pub fn after_cpu_cycle(&mut self, is_write: bool) {
        let end_clock = self.cpu_divider - self.start_clock;
        self.master_clock += if is_write {
            end_clock - Self::READ_WRITE_SHIFT
        } else {
            end_clock + Self::READ_WRITE_SHIFT
        };
    }

    pub fn ppu_cycles_since_last(&mut self) -> u64 {
        let elapsed_master_ticks = self.master_clock - self.ppu_clock;
        let ppu_cycles = elapsed_master_ticks / self.ppu_divider;
        self.ppu_clock += ppu_cycles * self.ppu_divider;
        ppu_cycles
    }

    pub fn ppu_cycles(&self) -> u64 {
        self.ppu_clock
    }

    pub fn set_ppu_cycles(&mut self, cycles: u64) {
        self.ppu_clock = cycles;
    }

    pub fn reset(&mut self) {
        self.master_clock = 0;
        self.ppu_clock = 0;
    }

    #[allow(dead_code)]
    pub fn cpu_divider(&self) -> u64 {
        self.cpu_divider
    }

    #[allow(dead_code)]
    pub fn ppu_divider(&self) -> u64 {
        self.ppu_divider
    }
}

#[cfg(test)]
mod tests {
    use super::MasterClock;
    use crate::console::TimingMode;

    #[test]
    fn test_ppu_cycles_since_last_ntsc_rounds_down_and_tracks_remainder() {
        // NTSC uses ppu_divider=4 (master ticks per PPU cycle)
        let mut clock = MasterClock::new(TimingMode::Ntsc);

        assert_eq!(clock.ppu_cycles_since_last(), 0);

        // Less than one PPU cycle worth of master ticks -> 0 PPU cycles
        clock.set_master_cycles(3);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        // Exactly one PPU cycle -> 1
        clock.set_master_cycles(4);
        assert_eq!(clock.ppu_cycles_since_last(), 1);

        // Remainder should be carried: going to 7 is still < 4 ticks since last alignment
        clock.set_master_cycles(7);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        // Crossing the next boundary -> 1 more
        clock.set_master_cycles(8);
        assert_eq!(clock.ppu_cycles_since_last(), 1);
    }

    #[test]
    fn test_ppu_cycles_since_last_ntsc_multiple_cycles_at_once() {
        let mut clock = MasterClock::new(TimingMode::Ntsc);

        clock.set_master_cycles(10);
        // floor(10 / 4) = 2
        assert_eq!(clock.ppu_cycles_since_last(), 2);

        // After alignment to 8, master=11 only adds 3 ticks -> still 0
        clock.set_master_cycles(11);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        // master=12 adds 4 ticks since last alignment -> 1
        clock.set_master_cycles(12);
        assert_eq!(clock.ppu_cycles_since_last(), 1);
    }

    #[test]
    fn test_ppu_cycles_since_last_pal_uses_divider_5() {
        // PAL uses ppu_divider=5
        let mut clock = MasterClock::new(TimingMode::Pal);

        clock.set_master_cycles(4);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        clock.set_master_cycles(5);
        assert_eq!(clock.ppu_cycles_since_last(), 1);

        clock.set_master_cycles(9);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        clock.set_master_cycles(10);
        assert_eq!(clock.ppu_cycles_since_last(), 1);
    }

    // --- Dendy master clock correctness (issue #1889) ---
    // Spec (Mesen2 NesCpu.cpp): Dendy cpu_divider=15, ppu_divider=5,
    // start_clock=7, end_clock=8. PPU:CPU ratio = 15/5 = 3.0.

    #[test]
    fn test_dendy_cpu_divider_is_15() {
        // Regression check: Dendy timing must use a 15-tick CPU divider, not 16.
        let clock = MasterClock::new(TimingMode::Dendy);
        assert_eq!(clock.cpu_divider(), 15);
    }

    #[test]
    fn test_dendy_before_read_cycle_is_6_master_ticks() {
        // start_clock=7, READ: start_clock - 1 = 6
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(false);
        assert_eq!(clock.master_cycles(), 6);
    }

    #[test]
    fn test_dendy_before_write_cycle_is_8_master_ticks() {
        // start_clock=7, WRITE: start_clock + 1 = 8
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(true);
        assert_eq!(clock.master_cycles(), 8);
    }

    #[test]
    fn test_dendy_read_cycle_total_is_15_master_ticks() {
        // before(6) + after(9) = 15 — one full CPU read cycle
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(false);
        clock.after_cpu_cycle(false);
        assert_eq!(clock.master_cycles(), 15);
    }

    #[test]
    fn test_dendy_write_cycle_total_is_15_master_ticks() {
        // before(8) + after(7) = 15 — one full CPU write cycle
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(true);
        clock.after_cpu_cycle(true);
        assert_eq!(clock.master_cycles(), 15);
    }

    #[test]
    fn test_dendy_one_cpu_cycle_yields_3_ppu_cycles_with_no_remainder() {
        // ppu_divider=5: 15 master ticks per CPU cycle -> 15/5 = 3 PPU cycles exactly.
        // Validates that the asymmetric 7/8 split leaves ppu_clock perfectly aligned.

        // Read cycle: before(6) + after(9) = 15
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(false);
        clock.after_cpu_cycle(false);
        assert_eq!(clock.ppu_cycles_since_last(), 3);
        assert_eq!(
            clock.ppu_cycles_since_last(),
            0,
            "ppu_clock must be aligned with no remainder after read"
        );

        // Write cycle: before(8) + after(7) = 15
        let mut clock = MasterClock::new(TimingMode::Dendy);
        clock.before_cpu_cycle(true);
        clock.after_cpu_cycle(true);
        assert_eq!(clock.ppu_cycles_since_last(), 3);
        assert_eq!(
            clock.ppu_cycles_since_last(),
            0,
            "ppu_clock must be aligned with no remainder after write"
        );
    }

    #[test]
    fn test_reset_restores_initial_state_but_preserves_dividers() {
        let mut clock = MasterClock::new(TimingMode::Ntsc);

        // Advance internal clocks to a non-zero, misaligned state.
        clock.set_master_cycles(37);
        let _ = clock.ppu_cycles_since_last();
        clock.before_cpu_cycle(false);
        clock.after_cpu_cycle(true);

        assert_ne!(clock.master_cycles(), 0);
        let cpu_divider_before = clock.cpu_divider();
        let ppu_divider_before = clock.ppu_divider();

        // New API: should restore the initial state for this TV system.
        clock.reset();

        assert_eq!(clock.master_cycles(), 0);
        assert_eq!(clock.ppu_cycles_since_last(), 0);
        assert_eq!(clock.cpu_divider(), cpu_divider_before);
        assert_eq!(clock.ppu_divider(), ppu_divider_before);
    }
}
