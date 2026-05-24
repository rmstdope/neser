use crate::gb::bus::CgbBus;
use crate::gb::bus::DmgBus;
use crate::gb::bus::GbBus;
use crate::gb::cpu::Sm83;
#[cfg(test)]
use crate::gb::model::DmgModel;
use std::collections::VecDeque;

const MAX_CPU_TRACE_LINES: usize = 512;

/// Single CPU trace entry (executed instruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTraceLine {
    pub addr: u16,
    pub bytes: Vec<u8>,
    pub text: String,
}

/// Game Boy (DMG) console wrapper.
///
/// Wraps the SM83 CPU and a bus, providing the core console integration
/// needed to execute instructions and advance the attached hardware.
///
/// The generic `Gb<B>` interface exposes CPU stepping and cycle tracking.
/// DMG-specific integrations on `Gb<DmgBus>` additionally provide reset
/// behavior, screen/framebuffer access, and input handling through the bus.
pub struct Gb<B: GbBus> {
    pub cpu: Sm83<B>,
    /// Recent CPU trace (bounded ring buffer).
    recent_trace: VecDeque<CpuTraceLine>,
    /// When false, CPU trace capture is skipped for performance.
    /// Enable only when the debugger is open.
    cpu_trace_enabled: bool,
}

impl<B: GbBus> Gb<B> {
    pub fn new(bus: B) -> Self {
        Self {
            cpu: Sm83::new(bus),
            recent_trace: VecDeque::with_capacity(MAX_CPU_TRACE_LINES),
            cpu_trace_enabled: false,
        }
    }

    /// Step one CPU instruction. Returns the number of M-cycles consumed.
    pub fn step(&mut self) -> u8 {
        let before = self.cpu.cycles();
        self.cpu.execute();
        (self.cpu.cycles() - before) as u8
    }

    /// Total M-cycles elapsed.
    pub fn cycles(&self) -> u64 {
        self.cpu.cycles()
    }

    /// Enable or disable per-instruction CPU trace capture.
    /// Disable during normal emulation to avoid allocations.
    pub fn set_cpu_trace_enabled(&mut self, enabled: bool) {
        self.cpu_trace_enabled = enabled;
        if !enabled {
            self.recent_trace.clear();
        }
    }

    /// Check if CPU trace capture is enabled.
    pub fn cpu_trace_enabled(&self) -> bool {
        self.cpu_trace_enabled
    }

    /// Get recent CPU trace (last N instructions).
    pub fn recent_cpu_trace(&self, limit: usize) -> Vec<CpuTraceLine> {
        if limit == 0 || self.recent_trace.is_empty() {
            return Vec::new();
        }

        let start = self.recent_trace.len().saturating_sub(limit);
        self.recent_trace.iter().skip(start).cloned().collect()
    }

    /// Push a CPU trace line to the ring buffer.
    pub fn push_cpu_trace_line(&mut self, line: CpuTraceLine) {
        if self.recent_trace.len() == MAX_CPU_TRACE_LINES {
            self.recent_trace.pop_front();
        }
        self.recent_trace.push_back(line);
    }

    /// Read memory for debugger (no side effects).
    ///
    /// This provides safe read access for the debugger without triggering bus side effects.
    pub fn read_for_debugger(&self, addr: u16) -> u8 {
        self.cpu.bus.read_for_debugger(addr)
    }

    /// True if the PPU has completed a full frame since the last `clear_frame_ready`.
    pub fn is_frame_ready(&self) -> bool {
        self.cpu.bus.ppu().is_frame_ready()
    }

    /// Clear the frame-ready flag.
    pub fn clear_frame_ready(&mut self) {
        self.cpu.bus.ppu_mut().clear_frame_ready();
    }

    pub(crate) fn reconcile_stop_display_after_state_load(&mut self) {
        if self.cpu.stopped
            && self.cpu.bus.ppu().stop_display_mode() == crate::gb::ppu::StopDisplayMode::Inactive
        {
            self.cpu.bus.enter_stop_mode();
        } else if !self.cpu.stopped {
            self.cpu.bus.exit_stop_mode();
        }
    }
}

/// Reset support for Gb<DmgBus>.
impl Gb<DmgBus> {
    /// Reset the console to power-on state.
    ///
    /// The boot ROM is the single source of truth for all post-boot hardware
    /// state — CPU registers, IO registers, DIV phase, PPU/APU state.
    /// This method reinitialises all bus hardware (WRAM zeroed, PPU/timer/
    /// joypad/APU reset) and restarts execution from the boot ROM entry
    /// point at $0000.
    pub fn reset(&mut self) {
        self.cpu.reset_to_power_on();
        self.cpu.bus.reset();
    }

    /// Snapshot the current rendered screen as a 160×144 RGB byte vector.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.cpu.bus.ppu.screen_buffer().snapshot()
    }

    /// CRC32 of the current screen buffer.
    pub fn screen_crc32(&self) -> u32 {
        self.cpu.bus.ppu.screen_buffer().crc32()
    }
}

/// CGB screen and frame API.
impl Gb<CgbBus> {
    /// Reset the console.
    ///
    /// - `soft_reset = true`: resets only the CPU registers to the CGB
    ///   post-boot-ROM state (bus state preserved).
    /// - `soft_reset = false`: resets CPU registers **and** all bus state.
    ///   If boot ROM is active after reset, uses power-on register state;
    ///   otherwise uses post-boot-ROM state.
    pub fn reset(&mut self, soft_reset: bool) {
        if !soft_reset {
            self.cpu.bus.reset();
            if self.cpu.bus.is_boot_rom_active() {
                // Boot ROM will run: use power-on register state
                self.cpu.reset_to_power_on();
                self.cpu.regs.pc = 0x0000;
            } else {
                // Skip boot ROM: use post-boot register state
                self.cpu.reset_registers_cgb();
                self.cpu.regs.pc = 0x0100;
            }
        } else {
            // Soft reset: just restore post-boot registers
            self.cpu.reset_registers_cgb();
        }
    }

    /// Snapshot the current rendered screen as a 160×144 RGB byte vector.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.cpu.bus.ppu.screen_buffer().snapshot()
    }

    /// CRC32 of the current screen buffer.
    pub fn screen_crc32(&self) -> u32 {
        self.cpu.bus.ppu.screen_buffer().crc32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::cartridge::load_cartridge;

    /// Build a minimal valid ROM-only cartridge for reset tests.
    fn minimal_cart() -> Box<dyn crate::gb::cartridge::GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn make_dmg() -> Gb<DmgBus> {
        Gb::new(DmgBus::new(minimal_cart(), DmgModel::DmgB))
    }

    #[test]
    fn test_reset_starts_at_boot_rom_entry() {
        let mut gb = make_dmg();
        gb.step();
        assert_ne!(gb.cpu.regs.pc, 0x0000);
        gb.reset();
        assert_eq!(gb.cpu.regs.pc, 0x0000);
    }

    #[test]
    fn test_reset_clears_cpu_state() {
        let mut gb = make_dmg();
        gb.cpu.ime = true;
        gb.cpu.halted = true;
        gb.reset();
        assert!(!gb.cpu.ime);
        assert!(!gb.cpu.halted);
    }

    #[test]
    fn test_reset_clears_wram() {
        let mut gb = make_dmg();
        gb.cpu.bus.write(0xC100, 0xAB);
        gb.reset();
        assert_eq!(gb.cpu.bus.read(0xC100), 0x00);
    }

    #[test]
    fn test_reset_restores_boot_rom() {
        let mut gb = make_dmg();
        gb.reset();
        assert_eq!(gb.cpu.regs.pc, 0x0000);
        assert!(gb.cpu.bus.is_boot_rom_active());
    }

    struct TrackingBus {
        mem: [u8; 0x10000],
        ticked_cycles: u64,
    }

    impl TrackingBus {
        fn with_program(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                ticked_cycles: 0,
            }
        }
    }

    impl GbBus for TrackingBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }

        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }

        fn tick(&mut self, m_cycles: u8) {
            self.ticked_cycles += m_cycles as u64;
        }
    }

    #[test]
    fn test_step_ticks_bus_by_nop_m_cycle_count() {
        let bus = TrackingBus::with_program(&[0x00]);
        let mut console = Gb::new(bus);
        let before = console.cpu.cycles();
        console.step();
        let delta = console.cpu.cycles() - before;
        assert_eq!(delta, 1);
        assert_eq!(console.cpu.bus.ticked_cycles, delta);
    }

    #[test]
    fn test_step_ticks_bus_by_multi_cycle_instruction_cost() {
        let bus = TrackingBus::with_program(&[0x01, 0x00, 0x00]);
        let mut console = Gb::new(bus);
        console.step();
        assert_eq!(console.cpu.bus.ticked_cycles, 3);
    }
}
