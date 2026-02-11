use crate::console::TvSystem;

/// Tracks the master clock used to derive CPU/PPU timing.
///
/// per-CPU-cycle master clock advancement around each bus access.
#[derive(Debug, Clone, Copy, Default)]
pub struct MasterClock {
    master_clock: u64,
    ppu_clock: u64,
    cpu_divider: u64,
    ppu_divider: u64,
    master_ticks_before_cpu: u64,
    master_ticks_after_cpu: u64,
}

impl MasterClock {
    const READ_WRITE_SHIFT: u64 = 1;
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            master_clock: 0,
            ppu_clock: 0,
            cpu_divider: if tv_system == TvSystem::Ntsc { 12 } else { 16 },
            ppu_divider: if tv_system == TvSystem::Ntsc { 4 } else { 5 },
            master_ticks_before_cpu: if tv_system == TvSystem::Ntsc { 6 } else { 8 },
            master_ticks_after_cpu: if tv_system == TvSystem::Ntsc { 6 } else { 8 },
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
            self.cpu_divider / 2 + Self::READ_WRITE_SHIFT
        } else {
            self.cpu_divider / 2 - Self::READ_WRITE_SHIFT
        };
    }

    pub fn after_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock += if is_write {
            self.cpu_divider / 2 - Self::READ_WRITE_SHIFT
        } else {
            self.cpu_divider / 2 + Self::READ_WRITE_SHIFT
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
    use crate::console::TvSystem;

    #[test]
    fn test_ppu_cycles_since_last_ntsc_rounds_down_and_tracks_remainder() {
        // NTSC uses ppu_divider=4 (master ticks per PPU cycle)
        let mut clock = MasterClock::new(TvSystem::Ntsc);

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
        let mut clock = MasterClock::new(TvSystem::Ntsc);

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
        let mut clock = MasterClock::new(TvSystem::Pal);

        clock.set_master_cycles(4);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        clock.set_master_cycles(5);
        assert_eq!(clock.ppu_cycles_since_last(), 1);

        clock.set_master_cycles(9);
        assert_eq!(clock.ppu_cycles_since_last(), 0);

        clock.set_master_cycles(10);
        assert_eq!(clock.ppu_cycles_since_last(), 1);
    }

    #[test]
    fn test_reset_restores_initial_state_but_preserves_dividers() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);

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
