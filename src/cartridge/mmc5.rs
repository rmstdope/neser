use crate::cartridge::cartridge::MirroringMode;
use crate::cartridge::mapper::Mapper;

pub struct MMC5Mapper {
    prg_rom: Vec<u8>,
    chr: Chr,
    prg_ram: Vec<u8>,
    mirroring: MirroringMode,

    prg_mode: u8,
    prg_bank_5113: u8,
    prg_bank_5114: u8,
    prg_bank_5115: u8,
    prg_bank_5116: u8,
    prg_bank_5117: u8,
}

enum Chr {
    Rom(Vec<u8>),
    Ram(Vec<u8>),
}

impl MMC5Mapper {
    const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
    const PRG_RAM_BANK_COUNT: usize = 8;
    const PRG_ROM_BANK_SIZE: usize = 8 * 1024;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let prg_rom_bank_count_8k = prg_rom.len() / Self::PRG_ROM_BANK_SIZE;

        let chr = if chr_rom.is_empty() {
            Chr::Ram(vec![0u8; 8 * 1024])
        } else {
            Chr::Rom(chr_rom)
        };

        // A compatible superset (see nesdev): emulate 64KB PRG-RAM as 8 x 8KB banks.
        // Games that have less won't generally notice.
        let prg_ram = vec![0u8; Self::PRG_RAM_BANK_COUNT * Self::PRG_RAM_BANK_SIZE];

        // MMC5 PRG mode defaults to 3 at power-on.
        // $5117 defaults to $FF on real hardware; for our bank-indexed model, we map it to the
        // last available 8KB PRG ROM bank when present.
        Self {
            prg_rom,
            chr,
            prg_ram,
            mirroring,

            prg_mode: 3,
            prg_bank_5113: 0,
            prg_bank_5114: 0x80,
            prg_bank_5115: 0x80,
            prg_bank_5116: 0x80,
            prg_bank_5117: prg_rom_bank_count_8k.saturating_sub(1) as u8,
        }
    }

    fn prg_rom_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_ROM_BANK_SIZE
    }

    fn read_prg_rom_8k(&self, bank: u8, addr: u16, base: u16) -> u8 {
        let num_banks = self.prg_rom_bank_count_8k();
        if num_banks == 0 {
            return 0;
        }

        let bank_index = (bank as usize) % num_banks;
        let offset = (addr - base) as usize;
        self.prg_rom[bank_index * Self::PRG_ROM_BANK_SIZE + offset]
    }

    fn prg_ram_bank_index_8k(bank: u8) -> usize {
        // $5113 ignores bits 7..4; for $5114-$5116, bit 7 selects ROM/RAM.
        ((bank & 0x07) as usize) % Self::PRG_RAM_BANK_COUNT
    }

    fn read_prg_ram_8k(&self, bank: u8, addr: u16, base: u16) -> u8 {
        let bank_index = Self::prg_ram_bank_index_8k(bank);
        let offset = (addr - base) as usize;
        let index = bank_index * Self::PRG_RAM_BANK_SIZE + offset;
        self.prg_ram.get(index).copied().unwrap_or(0)
    }

    fn write_prg_ram_8k(&mut self, bank: u8, addr: u16, base: u16, value: u8) {
        let bank_index = Self::prg_ram_bank_index_8k(bank);
        let offset = (addr - base) as usize;
        let index = bank_index * Self::PRG_RAM_BANK_SIZE + offset;
        if let Some(slot) = self.prg_ram.get_mut(index) {
            *slot = value;
        }
    }

    fn read_window_8k(&self, reg: u8, addr: u16, base: u16) -> u8 {
        if (reg & 0x80) != 0 {
            self.read_prg_rom_8k(reg & 0x7F, addr, base)
        } else {
            self.read_prg_ram_8k(reg, addr, base)
        }
    }

    fn write_window_8k(&mut self, reg: u8, addr: u16, base: u16, value: u8) {
        if (reg & 0x80) == 0 {
            self.write_prg_ram_8k(reg, addr, base, value);
        }
    }

    fn read_window_16k_mode2(&self, reg: u8, addr: u16) -> u8 {
        let second_8k = if addr >= 0xA000 { 1u8 } else { 0u8 };
        if (reg & 0x80) != 0 {
            // ROM bank index in 8KB units; even-aligned for 16KB.
            let bank_base = (reg & 0x7F) & !1;
            if addr >= 0xA000 {
                self.read_prg_rom_8k(bank_base.wrapping_add(second_8k), addr, 0xA000)
            } else {
                self.read_prg_rom_8k(bank_base, addr, 0x8000)
            }
        } else if addr >= 0xA000 {
            self.read_prg_ram_8k(reg.wrapping_add(second_8k), addr, 0xA000)
        } else {
            self.read_prg_ram_8k(reg, addr, 0x8000)
        }
    }

    fn write_window_16k_mode2(&mut self, reg: u8, addr: u16, value: u8) {
        if (reg & 0x80) != 0 {
            return;
        }

        let second_8k = if addr >= 0xA000 { 1u8 } else { 0u8 };
        if addr >= 0xA000 {
            self.write_prg_ram_8k(reg.wrapping_add(second_8k), addr, 0xA000, value);
        } else {
            self.write_prg_ram_8k(reg, addr, 0x8000, value);
        }
    }
}

impl Mapper for MMC5Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_ram_8k(self.prg_bank_5113, addr, 0x6000),

            0x8000..=0xFFFF => {
                let prg_mode = self.prg_mode & 0x03;
                match prg_mode {
                    2 => match addr {
                        // $8000-$BFFF: 16KB bank via $5115 (bit 0 ignored)
                        0x8000..=0xBFFF => self.read_window_16k_mode2(self.prg_bank_5115, addr),

                        // $C000-$DFFF: 8KB bank via $5116
                        0xC000..=0xDFFF => self.read_window_8k(self.prg_bank_5116, addr, 0xC000),

                        // $E000-$FFFF: 8KB fixed ROM bank via $5117
                        0xE000..=0xFFFF => {
                            self.read_prg_rom_8k(self.prg_bank_5117 & 0x7F, addr, 0xE000)
                        }

                        _ => 0,
                    },

                    3 => match addr {
                        // Four 8KB banks.
                        0x8000..=0x9FFF => self.read_window_8k(self.prg_bank_5114, addr, 0x8000),
                        0xA000..=0xBFFF => self.read_window_8k(self.prg_bank_5115, addr, 0xA000),
                        0xC000..=0xDFFF => self.read_window_8k(self.prg_bank_5116, addr, 0xC000),
                        0xE000..=0xFFFF => {
                            // $5117 always maps ROM.
                            self.read_prg_rom_8k(self.prg_bank_5117 & 0x7F, addr, 0xE000)
                        }
                        _ => 0,
                    },

                    // Modes 0/1 not implemented yet; provide a safe-ish fixed-last-bank fallback.
                    _ => {
                        let fixed_bank = (self.prg_rom_bank_count_8k().saturating_sub(1)) as u8;
                        self.read_prg_rom_8k(fixed_bank, addr, addr & 0xE000)
                    }
                }
            }

            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5100 => {
                self.prg_mode = value & 0x03;
            }

            // PRG bankswitch registers.
            0x5113 => self.prg_bank_5113 = value,
            0x5114 => self.prg_bank_5114 = value,
            0x5115 => self.prg_bank_5115 = value,
            0x5116 => self.prg_bank_5116 = value,
            0x5117 => self.prg_bank_5117 = value,

            0x6000..=0x7FFF => {
                self.write_prg_ram_8k(self.prg_bank_5113, addr, 0x6000, value);
            }

            // Support basic PRG-RAM writes when a window is mapped to RAM.
            0x8000..=0xDFFF => {
                let prg_mode = self.prg_mode & 0x03;
                match prg_mode {
                    2 => match addr {
                        0x8000..=0xBFFF => {
                            self.write_window_16k_mode2(self.prg_bank_5115, addr, value);
                        }
                        0xC000..=0xDFFF => {
                            self.write_window_8k(self.prg_bank_5116, addr, 0xC000, value);
                        }
                        _ => {}
                    },

                    3 => match addr {
                        0x8000..=0x9FFF => {
                            self.write_window_8k(self.prg_bank_5114, addr, 0x8000, value);
                        }
                        0xA000..=0xBFFF => {
                            self.write_window_8k(self.prg_bank_5115, addr, 0xA000, value);
                        }
                        0xC000..=0xDFFF => {
                            self.write_window_8k(self.prg_bank_5116, addr, 0xC000, value);
                        }
                        _ => {}
                    },

                    _ => {}
                }
            }

            // Minimal: ignore other MMC5 registers for now (CHR banking, mirroring, IRQ, etc.).
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let addr = (addr & 0x1FFF) as usize;
        match &self.chr {
            Chr::Rom(data) => data.get(addr % data.len()).copied().unwrap_or(0),
            Chr::Ram(data) => data.get(addr).copied().unwrap_or(0),
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let addr = (addr & 0x1FFF) as usize;
        if let Chr::Ram(data) = &mut self.chr {
            if let Some(slot) = data.get_mut(addr) {
                *slot = value;
            }
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {}

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge::MirroringMode;
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
    fn test_mmc5_mapper_5_is_wired_in_factory() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");
    }

    #[test]
    fn test_mmc5_prg_mode_3_8kb_bank_mapping() {
        // MMC5 PRG mode 3: four 8KB banks at $8000-$FFFF.
        // - $8000-$9FFF uses $5114
        // - $A000-$BFFF uses $5115
        // - $C000-$DFFF uses $5116
        // - $E000-$FFFF uses $5117 (ROM only)
        //
        // For $5114-$5116 bit7 selects ROM (1) vs RAM (0). This test uses ROM.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Explicitly select PRG mode 3.
        mapper.write_prg(0x5100, 0x03);

        // Map banks 2/3/4/7 into the 4x 8KB slots.
        mapper.write_prg(0x5114, 0b1000_0000 | 2);
        mapper.write_prg(0x5115, 0b1000_0000 | 3);
        mapper.write_prg(0x5116, 0b1000_0000 | 4);
        mapper.write_prg(0x5117, 7);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc5_prg_ram_bank_switching_via_5113() {
        // MMC5 has switchable PRG-RAM; $5113 selects the PRG-RAM bank.
        // This test checks that selecting different banks changes what data is visible at $6000.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG-RAM bank 0 and write a value.
        mapper.write_prg(0x5113, 0);
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Select PRG-RAM bank 1; the value should not be present.
        mapper.write_prg(0x5113, 1);
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Write a different value in bank 1.
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);

        // Switch back to bank 0; original value should be visible again.
        mapper.write_prg(0x5113, 0);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn test_mmc5_prg_mode_2_16kb_plus_8kb_plus_fixed_8kb_mapping() {
        // MMC5 PRG mode 2:
        // - $8000-$BFFF: 16KB bank selected via $5115 (bit 0 ignored)
        // - $C000-$DFFF: 8KB bank selected via $5116
        // - $E000-$FFFF: 8KB fixed bank selected via $5117 (ROM only)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG mode 2.
        mapper.write_prg(0x5100, 0x02);

        // Select a 16KB bank for $8000-$BFFF using an odd value; bit 0 must be ignored,
        // so $8000 should still map to the even bank, and $A000 to the following bank.
        mapper.write_prg(0x5115, 0b1000_0011); // ROM, bank index 3 -> treated as 2 for 16KB

        // Select an 8KB bank at $C000.
        mapper.write_prg(0x5116, 0b1000_0101); // ROM, bank 5

        // Fixed last bank window uses ROM only.
        mapper.write_prg(0x5117, 7);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }
}
