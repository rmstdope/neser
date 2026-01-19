use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};
use crate::trace_mapper;

/// VRC2/VRC4 mapper variants (iNES Mappers 21-23, 25).
///
/// These mappers represent different pin configurations of Konami's VRC2 and VRC4 chips.
/// They share the same functionality but access registers at different addresses due to
/// different address line connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vrc2Vrc4Variant {
    Mapper21, // VRC4a, VRC4c
    Mapper22, // VRC2a (no IRQ)
    Mapper23, // VRC2b, VRC4e (has IRQ, typically treated as VRC4)
    Mapper25, // VRC4b, VRC4d
}

impl Vrc2Vrc4Variant {
    fn has_irq(&self) -> bool {
        match self {
            Vrc2Vrc4Variant::Mapper21 => true,
            Vrc2Vrc4Variant::Mapper22 => false, // VRC2 has no IRQ
            Vrc2Vrc4Variant::Mapper23 => true,
            Vrc2Vrc4Variant::Mapper25 => true,
        }
    }
}

/// Konami VRC2/VRC4 mapper (iNES Mapper 21, 22, 23, 25).
///
/// This implementation supports PRG/CHR banking + mirroring control.
/// VRC4 variants (21, 23, 25) also support the VRC IRQ system.
/// VRC2 variant (22) has no IRQ support.
///
/// Unlike VRC6, these mappers have no expansion audio.
pub struct Vrc2Vrc4Mapper {
    variant: Vrc2Vrc4Variant,

    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: PrgRam,

    prg_bank_16k: u8,
    prg_bank_8k: u8,
    chr_banks_1k: [u8; 8],

    b003: u8,
    mirroring: MirroringMode,

    // --- VRC IRQ (used by VRC4 variants only) ---
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    irq_mode_cycle: bool,
    irq_enable_after_ack: bool,
    irq_asserted: bool,
    irq_prescaler: i32,
}

impl Vrc2Vrc4Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const DEFAULT_CHR_RAM_SIZE: usize = 0x2000;

    pub fn new(
        mapper_number: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> Self {
        let variant = match mapper_number {
            21 => Vrc2Vrc4Variant::Mapper21,
            22 => Vrc2Vrc4Variant::Mapper22,
            23 => Vrc2Vrc4Variant::Mapper23,
            25 => Vrc2Vrc4Variant::Mapper25,
            _ => Vrc2Vrc4Variant::Mapper21,
        };

        let chr_ram = if chr_rom.is_empty() {
            vec![0; Self::DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            variant,
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_bank_16k: 0,
            prg_bank_8k: 0,
            chr_banks_1k: [0; 8],
            b003: 0,
            mirroring,

            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_mode_cycle: false,
            irq_enable_after_ack: false,
            irq_asserted: false,
            irq_prescaler: 0,
        }
    }

    fn has_chr_ram(&self) -> bool {
        self.chr_rom.is_empty()
    }

    fn prg_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE_8K
    }

    fn chr_bank_count_1k(&self) -> usize {
        let chr_len = if self.has_chr_ram() {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        chr_len / Self::CHR_BANK_SIZE_1K
    }

    fn prg_bank_index_8k(&self, bank: usize) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        bank % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn fixed_last_prg_bank_8k(&self) -> usize {
        let count = self.prg_bank_count_8k();
        count.saturating_sub(1)
    }

    /// Normalize register address based on the mapper variant.
    ///
    /// Each mapper variant has different CPU address line connections to the chip's
    /// register address inputs:
    /// - Mapper 21: CPU A1→chip A0, CPU A2→chip A1 (VRC4a/VRC4c)
    /// - Mapper 22: CPU A1→chip A0, CPU A0→chip A1 (VRC2a) - swapped from normal
    /// - Mapper 23: CPU (A0|A1)→chip A0, CPU (A2|A3)→chip A1 (VRC2b/VRC4e) - uses OR of address lines
    /// - Mapper 25: CPU A1→chip A0, CPU A3→chip A1 (VRC4b/VRC4d)
    fn normalize_reg_addr(&self, addr: u16) -> u16 {
        // Base address uses A12-A15 for register selection
        let base = addr & 0xF000;

        match self.variant {
            Vrc2Vrc4Variant::Mapper21 => {
                // VRC4a/VRC4c: CPU A1→chip A0, CPU A2→chip A1 (registers on bits 1-2, shifted left by 1)
                let a0 = (addr >> 1) & 0x01;
                let a1 = (addr >> 2) & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper22 => {
                // VRC2a: CPU A1→chip A0, CPU A0→chip A1 (swapped on bits 0-1)
                let a0 = (addr >> 1) & 0x01;
                let a1 = addr & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper23 => {
                // VRC2b/VRC4e: CPU (A0|A1)→chip A0, CPU (A2|A3)→chip A1
                let a0 = ((addr & 0x01) | ((addr >> 1) & 0x01)) & 0x01;
                let a1 = (((addr >> 2) & 0x01) | ((addr >> 3) & 0x01)) & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper25 => {
                // VRC4b/VRC4d: CPU A1→chip A0, CPU A3→chip A1 (bits 1 and 3)
                let a0 = (addr >> 1) & 0x01;
                let a1 = (addr >> 3) & 0x01;
                base | (a1 << 1) | a0
            }
        }
    }

    fn update_mirroring_from_b003(&mut self) {
        // Mirroring control bits (same as VRC6)
        self.mirroring = match self.b003 & 0x03 {
            0x0 => MirroringMode::Vertical,
            0x1 => MirroringMode::Horizontal,
            0x2 | 0x3 => MirroringMode::SingleScreen,
            _ => self.mirroring,
        };
    }

    fn read_prg_rom_8k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE_8K + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE_1K + bank_offset;
        if self.has_chr_ram() {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        }
    }

    fn reset_irq_prescaler(&mut self) {
        // VRC IRQ scanline-mode prescaler (nesdev): 341 master ticks / 3 per CPU cycle.
        // Using the simple model: start at 341 and subtract 3 each CPU cycle; when <= 0,
        // add 341 and clock the IRQ counter. This makes the first clock after 114 cycles.
        self.irq_prescaler = 341;
    }

    fn acknowledge_irq(&mut self) {
        self.irq_asserted = false;
    }

    fn clock_vrc_irq_counter(&mut self) {
        // VRC IRQ (nesdev):
        // If counter is $FF, reload from latch and trip IRQ; otherwise increment.
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_asserted = true;
        } else {
            self.irq_counter = self.irq_counter.wrapping_add(1);
        }
    }

    fn tick_vrc_irq(&mut self) {
        if !self.variant.has_irq() {
            return;
        }

        if !self.irq_enabled {
            return;
        }

        if self.irq_mode_cycle {
            self.clock_vrc_irq_counter();
            return;
        }

        self.irq_prescaler -= 3;
        if self.irq_prescaler <= 0 {
            self.irq_prescaler += 341;
            self.clock_vrc_irq_counter();
        }
    }
}

impl Mapper for Vrc2Vrc4Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0xBFFF => {
                let offset = (addr - 0x8000) as usize;

                // 16KB bank at $8000-$BFFF, selected by 4-bit value.
                // Express in 8KB banks: bank16k * 2, then +0/+1 based on address.
                let bank16k = (self.prg_bank_16k & 0x0F) as usize;
                let bank8k = bank16k * 2 + (offset / Self::PRG_BANK_SIZE_8K);
                let bank_offset = offset % Self::PRG_BANK_SIZE_8K;

                self.read_prg_rom_8k(self.prg_bank_index_8k(bank8k), bank_offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                let bank8k = (self.prg_bank_8k & 0x1F) as usize;
                self.read_prg_rom_8k(self.prg_bank_index_8k(bank8k), offset)
            }
            0xE000..=0xFFFF => {
                let offset = (addr - 0xE000) as usize;
                self.read_prg_rom_8k(self.fixed_last_prg_bank_8k(), offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = self.normalize_reg_addr(addr);
            match reg {
                0x8000..=0x8003 => self.prg_bank_16k = value & 0x0F,
                0x9000..=0x9003 => {
                    self.b003 = value;
                    self.update_mirroring_from_b003();
                }
                0xA000..=0xA003 => self.prg_bank_8k = value & 0x1F,
                // CHR banking: after address normalization, Bxxx/Dxxx map to banks 0-3
                // and Cxxx/Exxx map to banks 4-7. This is a simplified view of the
                // VRC2/VRC4 split-nibble CHR registers.
                0xB000..=0xB003 | 0xD000..=0xD003 => {
                    let idx = (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                }
                0xC000..=0xC003 | 0xE000..=0xE003 => {
                    let idx = 4 + (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                }
                0xF000 => {
                    // IRQ Latch (VRC4 only)
                    if self.variant.has_irq() {
                        self.irq_latch = value;
                    }
                }
                0xF001 => {
                    // IRQ Control (VRC4 only)
                    if self.variant.has_irq() {
                        self.acknowledge_irq();
                        self.reset_irq_prescaler();

                        self.irq_mode_cycle = (value & 0b0000_0100) != 0;
                        let enable = (value & 0b0000_0010) != 0;
                        self.irq_enable_after_ack = (value & 0b0000_0001) != 0;

                        if enable {
                            self.irq_enabled = true;
                            self.irq_counter = self.irq_latch;
                        } else {
                            self.irq_enabled = false;
                        }
                    }
                }
                0xF002 | 0xF003 => {
                    // IRQ Acknowledge (VRC4 only)
                    if self.variant.has_irq() {
                        self.acknowledge_irq();
                        self.irq_enabled = self.irq_enable_after_ack;
                    }
                }
                _ => {}
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        let bank_slot = (addr as usize) / Self::CHR_BANK_SIZE_1K;
        let bank_offset = (addr as usize) % Self::CHR_BANK_SIZE_1K;

        let bank = self.chr_banks_1k.get(bank_slot).copied().unwrap_or(0);
        self.read_chr_1k(self.chr_bank_index_1k(bank), bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram() {
            return;
        }
        let addr = (addr & 0x1FFF) as usize;
        if addr < self.chr_ram.len() {
            self.chr_ram[addr] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // VRC2/VRC4 does not use A12 edge IRQs (VRC IRQ is CPU-cycle based).
    }

    fn cpu_cycle(&mut self) {
        trace_mapper!(5; "[vrc2_vrc4] cpu_cycle (irq)");
        if self.variant.has_irq() {
            self.tick_vrc_irq();
        }
    }

    fn irq_pending(&self) -> bool {
        if self.variant.has_irq() {
            self.irq_asserted
        } else {
            false
        }
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        match self.variant {
            Vrc2Vrc4Variant::Mapper21 => 21,
            Vrc2Vrc4Variant::Mapper22 => 22,
            Vrc2Vrc4Variant::Mapper23 => 23,
            Vrc2Vrc4Variant::Mapper25 => 25,
        }
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.clone()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.chr_ram.len());
        if to_copy > 0 {
            self.chr_ram[..to_copy].copy_from_slice(&data[..to_copy]);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize VRC2/VRC4 internal registers:
        // [0]: prg_bank_16k
        // [1]: prg_bank_8k
        // [2-9]: chr_banks_1k[0-7]
        // [10]: b003
        // [11]: irq_latch
        // [12]: irq_counter
        // [13]: flags (irq_enabled, irq_mode_cycle, irq_enable_after_ack, irq_asserted)
        // [14-17]: irq_prescaler (little endian i32)
        // [18]: mirroring
        let mut snapshot = Vec::with_capacity(19);
        snapshot.push(self.prg_bank_16k);
        snapshot.push(self.prg_bank_8k);
        snapshot.extend_from_slice(&self.chr_banks_1k);
        snapshot.push(self.b003);
        snapshot.push(self.irq_latch);
        snapshot.push(self.irq_counter);
        let flags = (self.irq_enabled as u8)
            | ((self.irq_mode_cycle as u8) << 1)
            | ((self.irq_enable_after_ack as u8) << 2)
            | ((self.irq_asserted as u8) << 3);
        snapshot.push(flags);
        let prescaler_bytes = self.irq_prescaler.to_le_bytes();
        snapshot.extend_from_slice(&prescaler_bytes);
        let mirroring = match self.mirroring {
            MirroringMode::Horizontal => 0,
            MirroringMode::Vertical => 1,
            MirroringMode::SingleScreen => 2,
            MirroringMode::FourScreen => 3,
            MirroringMode::SingleScreenLower => 2,
            MirroringMode::SingleScreenUpper => 2,
        };
        snapshot.push(mirroring);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 19 {
            self.prg_bank_16k = data[0];
            self.prg_bank_8k = data[1];
            self.chr_banks_1k.copy_from_slice(&data[2..10]);
            self.b003 = data[10];
            self.irq_latch = data[11];
            self.irq_counter = data[12];
            let flags = data[13];
            self.irq_enabled = (flags & 1) != 0;
            self.irq_mode_cycle = (flags & 2) != 0;
            self.irq_enable_after_ack = (flags & 4) != 0;
            self.irq_asserted = (flags & 8) != 0;
            self.irq_prescaler = i32::from_le_bytes([data[14], data[15], data[16], data[17]]);
            self.mirroring = match data[18] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreen,
                3 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge_core::MirroringMode;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    fn create_vrc_mapper(
        mapper_number: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new(
            mapper_number,
            prg_rom,
            chr_rom,
            mirroring,
        ))
        .expect("VRC mapper should be implemented")
    }

    #[test]
    fn test_vrc4_mapper_21_prg_banking() {
        // VRC4 banking (same as VRC6):
        // - $8000-$BFFF: 16KB switchable bank
        // - $C000-$DFFF: 8KB switchable bank
        // - $E000-$FFFF: 8KB fixed to last bank
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, MirroringMode::Horizontal);

        // Select 16KB bank #1 at $8000-$BFFF (8KB banks 2 and 3)
        mapper.write_prg(0x8000, 0x01);

        // Select 8KB bank #5 at $C000-$DFFF
        mapper.write_prg(0xA000, 0x05);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_vrc2_mapper_22_no_irq() {
        // Mapper 22 is VRC2a which has no IRQ support
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, MirroringMode::Horizontal);

        // Try to enable IRQ (should be ignored for VRC2)
        mapper.write_prg(0xF000, 0xFF);
        mapper.write_prg(0xF001, 0b0000_0110); // Enable in cycle mode

        // Run many cycles
        for _ in 0..1000 {
            mapper.cpu_cycle();
        }

        // IRQ should never trigger on VRC2
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_23_irq_cycle_mode() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, MirroringMode::Horizontal);

        mapper.write_prg(0xF000, 0xFE);
        mapper.write_prg(0xF001, 0b0000_0110); // M=1, E=1, A=0

        // After enable, counter reloaded to 0xFE
        // Cycle 1: 0xFE -> 0xFF (no IRQ)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        // Cycle 2: counter == 0xFF -> trip IRQ
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // Ack should clear IRQ
        mapper.write_prg(0xF002, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_25_chr_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 32);

        let mut mapper = create_vrc_mapper(25, prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set CHR bank 0 to bank 7
        mapper.write_prg(0xB000, 7);
        assert_eq!(mapper.read_chr(0x0000), 7);

        // Set CHR bank 4 to bank 15
        mapper.write_prg(0xC000, 15);
        assert_eq!(mapper.read_chr(0x1000), 15);
    }

    #[test]
    fn test_vrc2_vrc4_mirroring_control() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, MirroringMode::Horizontal);

        // Test vertical mirroring
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Test horizontal mirroring
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Test single screen mirroring
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_vrc2_vrc4_registers_snapshot_restores_state() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(
            21,
            prg_rom.clone(),
            chr_rom.clone(),
            MirroringMode::Horizontal,
        );

        mapper.write_prg(0x8000, 0x03); // prg_bank_16k
        mapper.write_prg(0xA000, 0x05); // prg_bank_8k
        mapper.write_prg(0xB000, 0x02); // chr bank 0
        mapper.write_prg(0xC000, 0x04); // chr bank 4
        mapper.write_prg(0x9000, 0x01); // mirroring horizontal

        mapper.write_prg(0xF000, 0xFE);
        mapper.write_prg(0xF001, 0b0000_0110);
        mapper.cpu_cycle();

        let regs = mapper.registers_snapshot();

        let mut restored = create_vrc_mapper(21, prg_rom, chr_rom, MirroringMode::Vertical);
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 6);
        assert_eq!(restored.read_prg(0xC000), 5);
        assert_eq!(restored.read_chr(0x0000), 2);
        assert_eq!(restored.read_chr(0x1000), 4);
        assert_eq!(restored.get_mirroring(), MirroringMode::Horizontal);
        assert_eq!(restored.irq_pending(), mapper.irq_pending());
    }
}
