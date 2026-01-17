use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, DEFAULT_CHR_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};

/// Namco 163 (iNES mapper 19) – basic banking + IRQ (audio omitted).
pub struct Namco163Mapper {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: PrgRam,
    mirroring: MirroringMode,
    regs: [u8; 16],
    namco_ram: [u8; 128],
    irq_counter: u16, // 15-bit counter
    irq_enabled: bool,
    irq_pending: bool,
}

impl Namco163Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const IRQ_COUNTER_MAX: u16 = 0x7FFF;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let chr_ram = if chr_rom.is_empty() {
            vec![0; DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            mirroring,
            regs: [0; 16],
            namco_ram: [0; 128],
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
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

    fn prg_bank_index_8k(&self, bank: u8) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn map_mirroring(&mut self, value: u8) {
        self.mirroring = match value & 0x3 {
            0 => MirroringMode::Vertical,
            1 => MirroringMode::Horizontal,
            2 => MirroringMode::SingleScreenLower,
            3 => MirroringMode::SingleScreenUpper,
            _ => unreachable!("value is masked to 0..3"),
        };
    }

    fn load_irq_counter_from_regs(&mut self) {
        let high = (self.regs[13] as u16) & 0x7F;
        let low = self.regs[12] as u16;
        self.irq_counter = (high << 8) | low;
        self.irq_pending = false;
    }

    fn handle_register_write(&mut self, reg: usize, value: u8) {
        self.regs[reg] = value;
        match reg {
            11 => self.map_mirroring(value),
            12 => {
                // IRQ counter low bits
                self.load_irq_counter_from_regs();
            }
            13 => {
                // IRQ counter high bits + enable flag (bit 7)
                self.irq_enabled = (value & 0x80) != 0;
                self.load_irq_counter_from_regs();
            }
            _ => {}
        }
    }

    fn read_namco_ram(&self, addr: u16) -> u8 {
        let offset = ((addr as usize).saturating_sub(0x4800)) & 0x7F;
        self.namco_ram[offset]
    }

    fn write_namco_ram(&mut self, addr: u16, value: u8) {
        let offset = ((addr as usize).saturating_sub(0x4800)) & 0x7F;
        self.namco_ram[offset] = value;
    }
}

impl Mapper for Namco163Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x4800..=0x5FFF => self.read_namco_ram(addr),
            _ => {
                if let Some(value) = self.prg_ram.try_read(addr) {
                    return value;
                }

                if self.prg_rom.is_empty() {
                    return 0;
                }

                match addr {
                    0x8000..=0x9FFF => {
                        let bank = self.prg_bank_index_8k(self.regs[8]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xA000..=0xBFFF => {
                        let bank = self.prg_bank_index_8k(self.regs[9]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xC000..=0xDFFF => {
                        let bank = self.prg_bank_index_8k(self.regs[10]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xE000..=0xFFFF => {
                        let bank = self.prg_bank_count_8k().saturating_sub(1);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    _ => 0,
                }
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x4800..=0x5FFF).contains(&addr) {
            self.write_namco_ram(addr, value);
            return;
        }

        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = (addr & 0x000F) as usize;
            self.handle_register_write(reg, value);
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = (addr as usize) / Self::CHR_BANK_SIZE_1K;
        let bank_offset = (addr as usize) & (Self::CHR_BANK_SIZE_1K - 1);
        let bank_reg = self.regs.get(slot).copied().unwrap_or(0);
        let bank = self.chr_bank_index_1k(bank_reg);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;

        if self.has_chr_ram() {
            self.chr_ram.get(index).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(index).copied().unwrap_or(0)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram() {
            return;
        }

        let slot = (addr as usize) / Self::CHR_BANK_SIZE_1K;
        let bank_offset = (addr as usize) & (Self::CHR_BANK_SIZE_1K - 1);
        let bank_reg = self.regs.get(slot).copied().unwrap_or(0);
        let bank = self.chr_bank_index_1k(bank_reg);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;

        if index < self.chr_ram.len() {
            self.chr_ram[index] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {}

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }

        if self.irq_counter == Self::IRQ_COUNTER_MAX {
            self.irq_counter = 0;
            self.irq_pending = true;
        } else {
            self.irq_counter = (self.irq_counter + 1) & Self::IRQ_COUNTER_MAX;
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn reset(&mut self) {
        self.regs = [0; 16];
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
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
}

#[cfg(test)]
mod tests {
    use crate::cartridge::MirroringMode;
    use crate::cartridge::mapper::create_mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    #[test]
    fn namco163_prg_chr_banking_and_mirroring() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper(19, prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("Mapper 19 should be implemented");

        // Select PRG banks for $8000/$A000/$C000.
        mapper.write_prg(0x8008, 1);
        mapper.write_prg(0x8009, 2);
        mapper.write_prg(0x800A, 3);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR banking: 1KB banks across the 8 slots.
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);
        mapper.write_prg(0x8003, 7);
        mapper.write_prg(0x8004, 8);
        mapper.write_prg(0x8005, 9);
        mapper.write_prg(0x8006, 10);
        mapper.write_prg(0x8007, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);

        // Mirroring register (reg 11).
        mapper.write_prg(0x800B, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn namco163_irq_counter_overflow_triggers_and_write_clears() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper(19, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Mapper 19 should be implemented");

        // Load counter to 0x7FFF and enable (bit 7 of reg 13).
        mapper.write_prg(0x800C, 0xFF);
        mapper.write_prg(0x800D, 0xFF);

        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // Writing to reg 13 should clear the pending IRQ and disable when bit7 is 0.
        mapper.write_prg(0x800D, 0x00);
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn namco163_internal_ram_and_wram_snapshot() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = Vec::new(); // CHR-RAM path is fine for this test.

        let mut mapper = create_mapper(19, prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("Mapper 19 should be implemented");

        // Internal 128-byte RAM at $4800 mirrored.
        mapper.write_prg(0x4800, 0xAA);
        mapper.write_prg(0x487F, 0xBB);
        mapper.write_prg(0x4880, 0xCC); // Mirrors to start.

        assert_eq!(mapper.read_prg(0x4800), 0xCC);
        assert_eq!(mapper.read_prg(0x487F), 0xBB);
        assert_eq!(mapper.read_prg(0x4880), 0xCC);

        // PRG-RAM snapshot/restore.
        mapper.write_prg(0x6000, 0x11);
        assert_eq!(mapper.read_prg(0x6000), 0x11);

        let snap = mapper.wram_snapshot();
        mapper.write_prg(0x6000, 0x00);
        mapper.load_wram_snapshot(&snap);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
    }
}
