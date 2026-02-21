//! Mapper 050 - N-32 (Romeo / Super Mario Bros. 2 Japanese conversion)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_050>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 050 - N-32 (Romeo / SMB2 Japanese FDS conversion)
///
/// Hardware: 761214 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_050>
/// - PRG-ROM: 128 KiB (8 KiB banks)
/// - PRG-RAM: None
/// - CHR: 8 KiB fixed (ROM or RAM)
/// - Mirroring: Fixed from header
///
/// Registers (range $4020-$5FFF, mask $4120):
/// - $4020: [.... HLLM] PRG bank selector (scrambled)
/// - $4120: [.... ...E] IRQ enable
///
/// PRG window map (8 KiB each):
/// - $6000-$7FFF: Fixed bank $0F
/// - $8000-$9FFF: Fixed bank $08
/// - $A000-$BFFF: Fixed bank $09
/// - $C000-$DFFF: Switchable via $4020
/// - $E000-$FFFF: Fixed bank $0B
///
/// PRG register unscrambling ([HLLM] → bank):
/// - bit 3 (H) → bank bit 3
/// - bit 2 (L_hi) → bank bit 1
/// - bit 1 (L_lo) → bank bit 0
/// - bit 0 (M) → bank bit 2
///
/// IRQ: CPU-cycle counter; fires on the $0FFF→$1000 transition; auto-disables.
/// Disable ($4120 E=0): clear IRQ, reset counter. Enable ($4120 E=1): start counting.
///
/// Known games: Romeo (N-32), Super Mario Bros. 2 (JU) Alt Levels pirate
pub struct Mapper50 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    prg_reg: u8,
    irq_enabled: bool,
    irq_counter: u16,
    irq_pending: bool,
}

impl Mapper50 {
    const MAPPER_NUMBER: u8 = 50;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;

    const BANK_AT_6000: usize = 0x0F;
    const BANK_AT_8000: usize = 0x08;
    const BANK_AT_A000: usize = 0x09;
    const BANK_AT_E000: usize = 0x0B;

    // IRQ fires on the $0FFF→$1000 transition (after 4096 cycles)
    const IRQ_FIRE_COUNT: u16 = 0x1000;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            prg_reg: 0,
            irq_enabled: false,
            irq_counter: 0,
            irq_pending: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn wrapped_bank(bank: usize, count: usize) -> usize {
        if count == 0 { 0 } else { bank % count }
    }

    /// Unscramble the PRG register [HLLM] to get the actual 4-bit bank number.
    /// bit3=H → bank_bit3, bit2=L_hi → bank_bit1, bit1=L_lo → bank_bit0, bit0=M → bank_bit2
    fn unscramble_prg(reg: u8) -> usize {
        ((reg & 0x08) as usize)          // H  → bit 3
            | (((reg & 0x01) << 2) as usize) // M  → bit 2
            | (((reg & 0x06) >> 1) as usize) // LL → bits 1:0
    }
}

impl Mapper for Mapper50 {
    fn read_prg(&self, addr: u16) -> u8 {
        let count = self.prg_bank_count();
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        let bank = match addr {
            0x6000..=0x7FFF => Self::wrapped_bank(Self::BANK_AT_6000, count),
            0x8000..=0x9FFF => Self::wrapped_bank(Self::BANK_AT_8000, count),
            0xA000..=0xBFFF => Self::wrapped_bank(Self::BANK_AT_A000, count),
            0xC000..=0xDFFF => Self::wrapped_bank(Self::unscramble_prg(self.prg_reg), count),
            0xE000..=0xFFFF => Self::wrapped_bank(Self::BANK_AT_E000, count),
            _ => return 0,
        };
        self.prg_rom
            .get(bank * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0x4120 {
            0x4020 => {
                // PRG bank register (scrambled [HLLM])
                self.prg_reg = value & 0x0F;
            }
            0x4120 => {
                // IRQ control
                if (value & 0x01) != 0 {
                    self.irq_enabled = true;
                } else {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                    self.irq_counter = 0;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        self.irq_counter = self.irq_counter.wrapping_add(1);
        if self.irq_counter == Self::IRQ_FIRE_COUNT {
            self.irq_pending = true;
            self.irq_enabled = false; // auto-disable on fire
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        vec![
            self.prg_reg,
            flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.prg_reg = data[0];
            self.irq_enabled = (data[1] & 1) != 0;
            self.irq_pending = (data[1] & 2) != 0;
            self.irq_counter = (data[2] as u16) | ((data[3] as u16) << 8);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper50;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 19 banks × 8 KiB = 152 KiB (non-power-of-two)
    const PRG_BANKS: usize = 19;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        create_mapper(MapperContext::new(50, prg, chr, NametableLayout::Vertical))
            .expect("Mapper 50 should be implemented")
    }

    fn make_mapper_direct() -> Mapper50 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        Mapper50::new(prg, chr, NametableLayout::Vertical)
    }

    // --- Factory ---

    #[test]
    fn mapper_50_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new(
            50,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(8 * 1024, 1),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 50 must be registered in the factory"
        );
    }

    // --- PRG fixed banks ---

    #[test]
    fn prg_6000_is_fixed_bank_0x0f() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0x0F % PRG_BANKS as u8,
            "$6000 window must read from fixed bank $0F"
        );
    }

    #[test]
    fn prg_8000_is_fixed_bank_0x08() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0x08,
            "$8000 window must read from fixed bank $08"
        );
    }

    #[test]
    fn prg_a000_is_fixed_bank_0x09() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0x09,
            "$A000 window must read from fixed bank $09"
        );
    }

    #[test]
    fn prg_e000_is_fixed_bank_0x0b() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            0x0B,
            "$E000 window must read from fixed bank $0B"
        );
    }

    // --- PRG switchable bank ($C000) ---

    #[test]
    fn prg_c000_defaults_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must default to bank 0");
    }

    #[test]
    fn prg_c000_selects_bank_via_4020_register() {
        let mut mapper = make_mapper();
        // Reg=2 (0010): H=0, L_hi=0, L_lo=1, M=0 → bank bit1=0, bit0=1 → bank=1
        mapper.write_prg(0x4020, 0x02);
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$4020=2 must select bank 1 at $C000"
        );
    }

    #[test]
    fn prg_register_unscrambling_bit0_m_maps_to_bit2() {
        let mut mapper = make_mapper();
        // reg=1 (0001): M=1 → bank bit2 set → bank=4
        mapper.write_prg(0x4020, 0x01);
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$4020=1 (M bit) must map to bank 4 (bit2)"
        );
    }

    #[test]
    fn prg_register_unscrambling_bit3_h_maps_to_bit3() {
        let mut mapper = make_mapper();
        // reg=8 (1000): H=1 → bank bit3 set → bank=8
        mapper.write_prg(0x4020, 0x08);
        assert_eq!(
            mapper.read_prg(0xC000),
            8,
            "$4020=8 (H bit) must map to bank 8 (bit3)"
        );
    }

    #[test]
    fn prg_register_only_uses_low_nibble() {
        let mut mapper = make_mapper();
        // High nibble must be ignored
        mapper.write_prg(0x4020, 0xF2); // low nibble = 2 → bank 1
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_initially() {
        let mapper = make_mapper_direct();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper_direct();
        for _ in 0..0x2000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_fires_after_4096_cycles_when_enabled() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn irq_does_not_fire_before_4096_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x0FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles"
        );
    }

    #[test]
    fn irq_auto_disables_when_it_fires() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        // After firing, counter should not advance further
        let count_before = mapper.irq_counter;
        mapper.cpu_cycle();
        assert_eq!(
            mapper.irq_counter,
            count_before.wrapping_add(0), // counter stopped
            "Counter must stop after IRQ fires"
        );
    }

    #[test]
    fn irq_acknowledged_and_reset_by_writing_4120_with_e_clear() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4120, 0x00); // disable + ack + reset
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_counter_resets_on_disable() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x4120, 0x00); // disable + reset
        mapper.write_prg(0x4120, 0x01); // re-enable
        for _ in 0..0x0FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire until 4096 cycles after re-enable"
        );
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4020, 0x04); // prg_reg=4
        mapper.write_prg(0x4120, 0x01); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored PRG bank must match"
        );
    }
}
