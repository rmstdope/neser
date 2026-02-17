use crate::console::TvSystem;

/// Tracks the master clock used to derive CPU/PPU timing.
///
/// Models the NES master clock with a PPU offset (matching Mesen2's
/// `_ppuOffset`). PPU cycles advance using `while ppu_clock < target`
/// semantics. This produces a 2-before/1-after split for writes and
/// 1-before/2-after for reads, accurately reflecting when bus writes
/// are visible to the PPU.
#[derive(Debug, Clone, Copy, Default)]
pub struct MasterClock {
    master_clock: u64,
    ppu_clock: u64,
    cpu_divider: u64,
    ppu_divider: u64,
    ppu_offset: u64,
}

impl MasterClock {
    const READ_WRITE_SHIFT: u64 = 1;
    pub fn new(tv_system: TvSystem) -> Self {
        let cpu_divider = if tv_system == TvSystem::Ntsc { 12 } else { 16 };
        let ppu_divider = if tv_system == TvSystem::Ntsc { 4 } else { 5 };
        Self {
            master_clock: cpu_divider,
            ppu_clock: 0,
            cpu_divider,
            ppu_divider,
            ppu_offset: 1,
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

    /// Returns the number of PPU cycles elapsed since the last call.
    ///
    /// Uses a target-based advancement model (matching Mesen2's `_ppuOffset`):
    /// the PPU clock advances in steps of `ppu_divider` until it reaches or
    /// exceeds `master_clock - ppu_offset`. This naturally produces the correct
    /// 2-before/1-after split for writes and 1-before/2-after for reads.
    pub fn ppu_cycles_since_last(&mut self) -> u64 {
        let target = self.master_clock - self.ppu_offset;
        let mut ppu_cycles = 0u64;
        while self.ppu_clock < target {
            self.ppu_clock += self.ppu_divider;
            ppu_cycles += 1;
        }
        ppu_cycles
    }

    pub fn ppu_cycles(&self) -> u64 {
        self.ppu_clock
    }

    pub fn set_ppu_cycles(&mut self, cycles: u64) {
        self.ppu_clock = cycles;
    }

    pub fn reset(&mut self) {
        self.master_clock = self.cpu_divider;
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
    fn test_ntsc_write_cycle_2_before_1_after() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);
        // Drain initial catchup from master_clock starting at cpu_divider
        let _ = clock.ppu_cycles_since_last();

        // Stabilise alignment with a few read cycles
        for _ in 0..3 {
            clock.before_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
            clock.after_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
        }

        clock.before_cpu_cycle(true);
        let before = clock.ppu_cycles_since_last();
        clock.after_cpu_cycle(true);
        let after = clock.ppu_cycles_since_last();
        assert_eq!(before, 2, "Write: 2 PPU before bus access");
        assert_eq!(after, 1, "Write: 1 PPU after bus access");
    }

    #[test]
    fn test_ntsc_read_cycle_1_before_2_after() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);
        let _ = clock.ppu_cycles_since_last();

        for _ in 0..3 {
            clock.before_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
            clock.after_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
        }

        clock.before_cpu_cycle(false);
        let before = clock.ppu_cycles_since_last();
        clock.after_cpu_cycle(false);
        let after = clock.ppu_cycles_since_last();
        assert_eq!(before, 1, "Read: 1 PPU before bus access");
        assert_eq!(after, 2, "Read: 2 PPU after bus access");
    }

    #[test]
    fn test_three_ppu_per_cpu_cycle_ntsc() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);
        let _ = clock.ppu_cycles_since_last();

        let mut total = 0u64;
        for _ in 0..1000 {
            clock.before_cpu_cycle(false);
            total += clock.ppu_cycles_since_last();
            clock.after_cpu_cycle(false);
            total += clock.ppu_cycles_since_last();
        }
        assert_eq!(total, 3000);
    }

    #[test]
    fn test_pal_ppu_cycles() {
        let mut clock = MasterClock::new(TvSystem::Pal);
        let _ = clock.ppu_cycles_since_last();

        let mut total = 0u64;
        for _ in 0..5 {
            clock.before_cpu_cycle(false);
            total += clock.ppu_cycles_since_last();
            clock.after_cpu_cycle(false);
            total += clock.ppu_cycles_since_last();
        }
        // PAL: 5 CPU cycles × 16 master / 5 ppu_div = 16 PPU
        assert_eq!(total, 16);
    }

    #[test]
    fn test_reset_preserves_dividers() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);
        clock.set_master_cycles(37);
        let _ = clock.ppu_cycles_since_last();

        let cd = clock.cpu_divider();
        let pd = clock.ppu_divider();
        clock.reset();
        assert_eq!(clock.cpu_divider(), cd);
        assert_eq!(clock.ppu_divider(), pd);
        // After reset, master_clock should be at cpu_divider
        assert_eq!(clock.master_cycles(), cd);
    }

    #[test]
    fn test_dma_flat_advance_gives_3_ppu() {
        let mut clock = MasterClock::new(TvSystem::Ntsc);
        let _ = clock.ppu_cycles_since_last();

        // Stabilise alignment
        for _ in 0..3 {
            clock.before_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
            clock.after_cpu_cycle(false);
            let _ = clock.ppu_cycles_since_last();
        }

        clock.advance_cpu_cycles(1);
        assert_eq!(clock.ppu_cycles_since_last(), 3);
    }
}
