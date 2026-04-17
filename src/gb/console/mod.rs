use crate::gb::bus::CgbBus;
use crate::gb::bus::DmgBus;
use crate::gb::bus::GbBus;
use crate::gb::cpu::Sm83;
use crate::gb::model::DmgModel;

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
    /// Reset the console.
    ///
    /// - `soft_reset = true`: resets only the CPU registers to the
    ///   post-boot-ROM state (WRAM and other bus state are preserved).
    /// - `soft_reset = false`: resets CPU registers **and** all bus
    ///   state (WRAM zeroed, PPU/timer/joypad reinitialised).
    pub fn reset(&mut self, soft_reset: bool) {
        if soft_reset {
            // Soft reset: restore post-boot register state and continue from
            // the cartridge entry point, preserving WRAM and bus state.
            match self.cpu.bus.model() {
                DmgModel::DmgAbc => self.cpu.reset_registers(),
                DmgModel::Dmg0 => self.cpu.reset_registers_dmg0(),
            }
        } else {
            // Hard reset: reinitialise all bus hardware and restart execution
            // from the boot ROM entry point.
            match self.cpu.bus.model() {
                DmgModel::DmgAbc => self.cpu.reset_registers(),
                DmgModel::Dmg0 => self.cpu.reset_registers_dmg0(),
            }
            self.cpu.regs.pc = 0x0000;
            self.cpu.bus.reset();
        }
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
        Gb::new(DmgBus::new(minimal_cart(), DmgModel::DmgAbc))
    }

    // ── reset: CPU registers ──────────────────────────────────────────────

    #[test]
    fn test_reset_registers_restores_pc_to_0100() {
        // Given: a fresh DMG console
        let mut gb = make_dmg();
        // PC starts at 0 before any code runs
        assert_eq!(gb.cpu.regs.pc, 0x0000);
        // When: reset registers
        gb.cpu.reset_registers();
        // Then: PC = $0100 (post-boot-ROM entry)
        assert_eq!(gb.cpu.regs.pc, 0x0100);
    }

    #[test]
    fn test_reset_registers_sets_dmg_af() {
        // Given/When: reset CPU registers
        let mut gb = make_dmg();
        gb.cpu.reset_registers();
        // Then: AF = $01B0 (DMG post-boot value per Pan Docs)
        assert_eq!(gb.cpu.regs.af(), 0x01B0);
    }

    #[test]
    fn test_reset_registers_sets_dmg_bc_de_hl_sp() {
        let mut gb = make_dmg();
        gb.cpu.reset_registers();
        assert_eq!(gb.cpu.regs.bc(), 0x0013);
        assert_eq!(gb.cpu.regs.de(), 0x00D8);
        assert_eq!(gb.cpu.regs.hl(), 0x014D);
        assert_eq!(gb.cpu.regs.sp, 0xFFFE);
    }

    #[test]
    fn test_reset_registers_clears_ime_and_halted() {
        let mut gb = make_dmg();
        gb.cpu.ime = true;
        gb.cpu.halted = true;
        gb.cpu.reset_registers();
        assert!(!gb.cpu.ime);
        assert!(!gb.cpu.halted);
    }

    // ── reset: soft vs hard ───────────────────────────────────────────────

    #[test]
    fn test_soft_reset_preserves_wram() {
        // Given: write a known value to WRAM
        let mut gb = make_dmg();
        gb.cpu.bus.write(0xC100, 0xAB);
        // When: soft reset
        gb.reset(true);
        // Then: WRAM is unchanged
        assert_eq!(gb.cpu.bus.read(0xC100), 0xAB);
    }

    #[test]
    fn test_hard_reset_clears_wram() {
        // Given: write a known value to WRAM
        let mut gb = make_dmg();
        gb.cpu.bus.write(0xC100, 0xAB);
        // When: hard reset
        gb.reset(false);
        // Then: WRAM is zeroed
        assert_eq!(gb.cpu.bus.read(0xC100), 0x00);
    }

    #[test]
    fn test_hard_reset_restores_pc_and_clears_wram() {
        // After a hard reset the CPU starts at $0000 so the boot ROM runs,
        // matching real DMG power-on behaviour.  WRAM must still be zeroed.
        let mut gb = make_dmg();
        gb.cpu.bus.write(0xC000, 0xFF);
        gb.reset(false);
        assert_eq!(gb.cpu.regs.pc, 0x0000);
        assert_eq!(gb.cpu.bus.read(0xC000), 0x00);
    }

    #[test]
    fn test_soft_reset_restores_pc() {
        // Soft reset must still reset CPU registers (including PC)
        let mut gb = make_dmg();
        gb.reset(true);
        assert_eq!(gb.cpu.regs.pc, 0x0100);
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
