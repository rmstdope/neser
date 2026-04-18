use crate::gb::bus::CgbBus;
use crate::gb::bus::DmgBus;
use crate::gb::bus::GbBus;
use crate::gb::cpu::Sm83;
#[cfg(test)]
use crate::gb::model::DmgModel;

pub mod config;
pub mod gameboy;

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
}

impl<B: GbBus> Gb<B> {
    pub fn new(bus: B) -> Self {
        Self {
            cpu: Sm83::new(bus),
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
}

/// DMG-specific screen and frame API.
impl Gb<DmgBus> {
    /// Snapshot the current rendered screen as a 160×144 RGB byte vector.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.cpu.bus.ppu.screen_buffer().snapshot()
    }

    /// True if the PPU has completed a full frame since the last `clear_frame_ready`.
    pub fn is_frame_ready(&self) -> bool {
        self.cpu.bus.ppu.is_frame_ready()
    }

    /// Clear the frame-ready flag.
    pub fn clear_frame_ready(&mut self) {
        self.cpu.bus.ppu.clear_frame_ready();
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
    pub fn reset(&mut self, soft_reset: bool) {
        self.cpu.reset_registers_cgb();
        if !soft_reset {
            self.cpu.regs.pc = 0x0100;
            self.cpu.bus.reset();
        }
    }

    /// Snapshot the current rendered screen as a 160×144 RGB byte vector.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.cpu.bus.ppu.screen_buffer().snapshot()
    }

    /// True if the PPU has completed a full frame since the last `clear_frame_ready`.
    pub fn is_frame_ready(&self) -> bool {
        self.cpu.bus.ppu.is_frame_ready()
    }

    /// Clear the frame-ready flag.
    pub fn clear_frame_ready(&mut self) {
        self.cpu.bus.ppu.clear_frame_ready();
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

    // ── DMG reset helpers ─────────────────────────────────────────────────

    /// Build a minimal valid ROM-only cartridge for reset tests.
    fn minimal_cart() -> Box<dyn crate::gb::cartridge::GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn make_dmg() -> Gb<DmgBus> {
        Gb::new(DmgBus::new(minimal_cart(), DmgModel::DmgB))
    }

    // ── reset: CPU registers ──────────────────────────────────────────────

    #[test]
    fn test_reset_starts_at_boot_rom_entry() {
        // Given: a console that has executed some instructions
        let mut gb = make_dmg();
        gb.step(); // advance PC from $0000
        assert_ne!(gb.cpu.regs.pc, 0x0000);
        // When: reset
        gb.reset();
        // Then: PC = $0000 (boot ROM entry point)
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

    // ── reset: bus state ──────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_wram() {
        // Given: write a known value to WRAM
        let mut gb = make_dmg();
        gb.cpu.bus.write(0xC100, 0xAB);
        // When: reset
        gb.reset();
        // Then: WRAM is zeroed
        assert_eq!(gb.cpu.bus.read(0xC100), 0x00);
    }

    #[test]
    fn test_reset_restores_boot_rom() {
        // After reset, the boot ROM must be active again at $0000.
        let mut gb = make_dmg();
        gb.reset();
        assert_eq!(gb.cpu.regs.pc, 0x0000);
        assert!(gb.cpu.bus.is_boot_rom_active());
    }

    /// A bus that counts the total M-cycles passed to `tick()`.
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
        // Given: a NOP instruction at $0000 (costs 1 M-cycle)
        let bus = TrackingBus::with_program(&[0x00]); // NOP
        let mut console = Gb::new(bus);
        let before = console.cpu.cycles();
        // When: step executes one instruction
        console.step();
        let delta = console.cpu.cycles() - before;
        // Then: the bus was ticked by the NOP's M-cycle count (1)
        assert_eq!(delta, 1);
        assert_eq!(console.cpu.bus.ticked_cycles, delta);
    }

    #[test]
    fn test_step_ticks_bus_by_multi_cycle_instruction_cost() {
        // LD BC, nn is a 3-byte instruction costing 3 M-cycles
        let bus = TrackingBus::with_program(&[0x01, 0x00, 0x00]); // LD BC, $0000
        let mut console = Gb::new(bus);
        // When: step executes one instruction
        console.step();
        // Then: the bus was ticked by 3 M-cycles
        assert_eq!(console.cpu.bus.ticked_cycles, 3);
    }
}
