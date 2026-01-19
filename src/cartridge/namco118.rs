use crate::cartridge::{Mapper, MirroringMode};

/// Namco 118 / DxROM (iNES mapper 206)
///
/// A simplified MMC3:
/// - Same bank register format ($8000/$8001) for PRG/CHR banking
/// - No IRQ functionality
/// - Mirroring is hardwired from the cartridge header (writes to $A000 are ignored)
pub struct Namco118Mapper {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: Vec<u8>,

    mirroring: MirroringMode,

    bank_select: u8,
    regs: [u8; 8],
}

impl Namco118Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1KB
    const PRG_RAM_SIZE: usize = 0x2000; // 8KB
    const DEFAULT_CHR_RAM_SIZE: usize = 0x2000; // 8KB
    const PRG_MODE_MASK: u8 = 0b0100_0000;
    const CHR_MODE_MASK: u8 = 0b1000_0000;
    const REG_SELECT_MASK: u8 = 0b0000_0111;
    const EVEN_ALIGN_MASK: u8 = 0xFE;
    const CHR_ADDR_MASK: u16 = 0x1FFF;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let chr_ram = if chr_rom.is_empty() {
            vec![0; Self::DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: vec![0; Self::PRG_RAM_SIZE],
            mirroring,
            bank_select: 0,
            regs: [0; 8],
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn chr_bank_count_1k(&self) -> usize {
        let chr_len = if self.chr_rom.is_empty() {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        chr_len / Self::CHR_BANK_SIZE
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        let count = self.prg_bank_count();
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

    fn prg_mode(&self) -> bool {
        (self.bank_select & Self::PRG_MODE_MASK) != 0
    }

    fn chr_mode(&self) -> bool {
        (self.bank_select & Self::CHR_MODE_MASK) != 0
    }

    fn selected_reg(&self) -> usize {
        (self.bank_select & Self::REG_SELECT_MASK) as usize
    }

    fn read_prg_rom_bank(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_bank_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE + bank_offset;
        if self.chr_rom.is_empty() {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        }
    }

    fn map_chr_addr_to_bank_1k(&self, chr_addr: usize) -> (usize, usize) {
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE - 1);

        let r0 = self.regs[0] & Self::EVEN_ALIGN_MASK; // 2KB bank, even-aligned
        let r1 = self.regs[1] & Self::EVEN_ALIGN_MASK; // 2KB bank, even-aligned
        let r2 = self.regs[2];
        let r3 = self.regs[3];
        let r4 = self.regs[4];
        let r5 = self.regs[5];

        let bank_1k = if !self.chr_mode() {
            // CHR mode 0
            match chr_addr {
                0x0000..=0x03FF => r0,
                0x0400..=0x07FF => r0.wrapping_add(1),
                0x0800..=0x0BFF => r1,
                0x0C00..=0x0FFF => r1.wrapping_add(1),
                0x1000..=0x13FF => r2,
                0x1400..=0x17FF => r3,
                0x1800..=0x1BFF => r4,
                0x1C00..=0x1FFF => r5,
                _ => 0,
            }
        } else {
            // CHR mode 1
            match chr_addr {
                0x0000..=0x03FF => r2,
                0x0400..=0x07FF => r3,
                0x0800..=0x0BFF => r4,
                0x0C00..=0x0FFF => r5,
                0x1000..=0x13FF => r0,
                0x1400..=0x17FF => r0.wrapping_add(1),
                0x1800..=0x1BFF => r1,
                0x1C00..=0x1FFF => r1.wrapping_add(1),
                _ => 0,
            }
        };

        (self.chr_bank_index_1k(bank_1k), bank_offset)
    }
}

impl Mapper for Namco118Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let prg_count = self.prg_bank_count();
                if prg_count == 0 {
                    return 0;
                }

                let bank_offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);

                let fixed_last = prg_count.saturating_sub(1);
                let fixed_second_last = prg_count.saturating_sub(2);

                let r6 = self.prg_bank_index(self.regs[6]);
                let r7 = self.prg_bank_index(self.regs[7]);

                let bank_index = match addr {
                    0x8000..=0x9FFF => {
                        if self.prg_mode() {
                            fixed_second_last
                        } else {
                            r6
                        }
                    }
                    0xA000..=0xBFFF => r7,
                    0xC000..=0xDFFF => {
                        if self.prg_mode() {
                            r6
                        } else {
                            fixed_second_last
                        }
                    }
                    0xE000..=0xFFFF => fixed_last,
                    _ => 0,
                };

                self.read_prg_rom_bank(bank_index, bank_offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if let Some(byte) = self.prg_ram.get_mut(offset) {
                    *byte = value;
                }
            }
            0x8000..=0x9FFF => {
                if (addr & 1) == 0 {
                    // Bank select
                    self.bank_select = value;
                } else {
                    // Bank data
                    let reg = self.selected_reg();
                    self.regs[reg] = value;
                }
            }
            // $A000/$A001 (mirroring / PRG-RAM protect on MMC3) are ignored; mirroring is hardwired.
            0xA000..=0xBFFF => {}
            // $C000/$C001 and $E000/$E001 are IRQ-related on MMC3; ignored for Namco 118.
            0xC000..=0xFFFF => {}
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let chr_addr = (addr & Self::CHR_ADDR_MASK) as usize;
        let (bank_index, bank_offset) = self.map_chr_addr_to_bank_1k(chr_addr);
        self.read_chr_bank_1k(bank_index, bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_rom.is_empty() {
            return;
        }

        let chr_addr = (addr & Self::CHR_ADDR_MASK) as usize;
        let (bank_index, bank_offset) = self.map_chr_addr_to_bank_1k(chr_addr);
        let mapped_addr = bank_index * Self::CHR_BANK_SIZE + bank_offset;
        if let Some(byte) = self.chr_ram.get_mut(mapped_addr) {
            *byte = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {}

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        206
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
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
        // Serialize Namco118 internal registers:
        // [0]: bank_select
        // [1-8]: regs[0-7]
        let mut snapshot = Vec::with_capacity(9);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge_core::MirroringMode;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::namco118::Namco118Mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    fn create_namco118_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(206, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn namco118_prg_chr_banking_matches_mmc3_subset() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_namco118_mapper(prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("Mapper 206 should be implemented");

        // PRG mode 0 (bit 6 clear): R6 @ $8000, R7 @ $A000, fixed second-last @ $C000, last @ $E000.
        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // Switch to PRG mode 1 (bit 6 set): fixed second-last @ $8000, R7 @ $A000, R6 @ $C000, fixed last @ $E000.
        mapper.write_prg(0x8000, 0b0100_0110); // select R6 with PRG mode 1
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR mode 0 (bit 7 clear): R0/1 are 2KB even-aligned, R2-5 are 1KB.
        mapper.write_prg(0x8000, 0b0000_0000); // select R0, CHR mode 0
        mapper.write_prg(0x8001, 4); // R0 maps banks 4+5 at $0000-$07FF
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 6); // R1 maps banks 6+7 at $0800-$0FFF

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);
    }

    #[test]
    fn namco118_mirroring_and_irq_registers_are_noops() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_namco118_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Mapper 206 should be implemented");

        // Mirroring should stay hardwired to the cartridge header; writes to $A000 must not change it.
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // IRQ-related registers should have no effect; mapper never asserts IRQ.
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE000, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..3 {
            mapper.ppu_address_changed(0x1000);
            mapper.ppu_scanline(0, true);
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }
    }

    #[test]
    fn namco118_prg_ram_works_and_snapshots() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = Vec::new(); // CHR-RAM path

        let mut mapper = Namco118Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        let snap = mapper.wram_snapshot();
        assert_eq!(snap[0], 0xAA);

        mapper.write_prg(0x6000, 0x00);
        mapper.load_wram_snapshot(&snap);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn namco118_registers_snapshot_restores_bank_mapping() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper =
            create_namco118_mapper(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal)
                .expect("Mapper 206 should be implemented");

        // Set PRG bank registers R6/R7.
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 4);

        // Set CHR registers R0/R1 (2KB) and R2 (1KB).
        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 6);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 10);

        let regs = mapper.registers_snapshot();

        let mut restored = create_namco118_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Mapper 206 should be implemented");
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xA000), 4);
        assert_eq!(restored.read_chr(0x0000), 6);
        assert_eq!(restored.read_chr(0x0400), 7);
        assert_eq!(restored.read_chr(0x0800), 8);
        assert_eq!(restored.read_chr(0x1000), 10);
    }
}
