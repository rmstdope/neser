use crate::nes::TvSystem;

/// Tracks the master clock used to derive CPU/PPU timing.
///
/// NOTE: This is currently used by the legacy CPU implementation to model
/// per-CPU-cycle master clock advancement around each bus access.
#[derive(Debug, Clone, Copy, Default)]
pub struct MasterClock {
    master_clock: u64,
    cpu_divider: u64,
    ppu_divider: u64,
    master_ticks_before_cpu: u64,
    master_ticks_after_cpu: u64,
}

impl MasterClock {
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            master_clock: 0,
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

    pub fn before_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock += if is_write {
            self.master_ticks_before_cpu + 1
        } else {
            self.master_ticks_before_cpu - 1
        };
    }

    pub fn after_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock += if is_write {
            self.master_ticks_after_cpu - 1
        } else {
            self.master_ticks_after_cpu + 1
        };
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
