use crate::cartridge::{Mapper, MirroringMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vrc6Variant {
    Mapper24,
    Mapper26,
}

/// Konami VRC6 mapper (iNES Mapper 24/26).
///
/// This implementation currently focuses on PRG/CHR banking + mirroring control.
/// VRC6 audio expansion is handled elsewhere.
pub struct VRC6Mapper {
    variant: Vrc6Variant,

    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: Vec<u8>,

    prg_bank_16k: u8,
    prg_bank_8k: u8,
    chr_banks_1k: [u8; 8],

    b003: u8,
    mirroring: MirroringMode,
}

impl VRC6Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const PRG_BANK_SIZE_16K: usize = 0x4000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const PRG_RAM_SIZE: usize = 0x2000;
    const DEFAULT_CHR_RAM_SIZE: usize = 0x2000;

    pub fn new(
        mapper_number: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> Self {
        let variant = match mapper_number {
            24 => Vrc6Variant::Mapper24,
            26 => Vrc6Variant::Mapper26,
            _ => Vrc6Variant::Mapper24,
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
            prg_ram: vec![0; Self::PRG_RAM_SIZE],
            prg_bank_16k: 0,
            prg_bank_8k: 0,
            chr_banks_1k: [0; 8],
            b003: 0,
            mirroring,
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

    fn normalize_reg_addr(&self, addr: u16) -> u16 {
        // Only A0, A1, and A12-A15 are used for register selection.
        // Mirrors can be found by ANDing with $F003.
        let mut a = addr & 0xF003;

        // Mapper 26 swaps A0 and A1.
        if self.variant == Vrc6Variant::Mapper26 {
            let bit0 = a & 0x0001;
            let bit1 = a & 0x0002;
            a = (a & !0x0003) | (bit0 << 1) | (bit1 >> 1);
        }

        a
    }

    fn update_mirroring_from_b003(&mut self) {
        // Commercial VRC6 games use banking mode 0 and write values where (b003 & 0x0F)
        // is one of: 0, 4, 8, C.
        self.mirroring = match self.b003 & 0x0F {
            0x0 => MirroringMode::Vertical,
            0x4 => MirroringMode::Horizontal,
            0x8 | 0xC => MirroringMode::SingleScreen,
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
}

impl Mapper for VRC6Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
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
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if offset < self.prg_ram.len() {
                    self.prg_ram[offset] = value;
                }
            }
            0x8000..=0xFFFF => {
                let reg = self.normalize_reg_addr(addr);
                match reg {
                    0x8000..=0x8003 => self.prg_bank_16k = value & 0x0F,
                    0xC000..=0xC003 => self.prg_bank_8k = value & 0x1F,
                    0xB003 => {
                        self.b003 = value;
                        self.update_mirroring_from_b003();
                    }
                    0xD000..=0xD003 => {
                        let idx = (reg & 0x0003) as usize;
                        self.chr_banks_1k[idx] = value;
                    }
                    0xE000..=0xE003 => {
                        let idx = 4 + (reg & 0x0003) as usize;
                        self.chr_banks_1k[idx] = value;
                    }
                    // IRQ and audio registers are not yet modeled.
                    _ => {}
                }
            }
            _ => {}
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
        // VRC6 does not use A12 edge IRQs (VRC IRQ is CPU-cycle based).
    }

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
    fn test_vrc6_mapper_24_prg_banking() {
        // VRC6 banking (nesdev):
        // - $8000-$BFFF: 16KB switchable bank (selected via $8000-$8003)
        // - $C000-$DFFF: 8KB switchable bank (selected via $C000-$C003)
        // - $E000-$FFFF: 8KB fixed to last bank
        // This test uses PRG ROM filled with one byte value per 8KB bank.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        // Select 16KB bank #1 at $8000-$BFFF.
        // With 8KB banks, this is banks 2 and 3.
        mapper.write_prg(0x8000, 0x01);

        // Select 8KB bank #5 at $C000-$DFFF.
        mapper.write_prg(0xC000, 0x05);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_vrc6_chr_register_address_swap_mapper_24_vs_26() {
        // VRC6 registers use only A0, A1, and A12-A15.
        // For mapper 26, A0/A1 are swapped, i.e. swap bits 0 and 1 of the address.
        // This should swap the meaning of writes to $D001 and $D002 (R1 and R2).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 32);

        // Mapper 24: write R1 at $D001 to bank 7 -> $0400-$07FF reads bank 7.
        let mut m24 = create_mapper(24, prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");
        m24.write_prg(0xD001, 7);
        assert_eq!(m24.read_chr(0x0400), 7);

        // Mapper 26: the same CPU address $D001 should target internal R2 (not R1).
        // So $0400 should remain at default bank 0, while $0800 uses bank 7.
        let mut m26 = create_mapper(26, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("VRC6 (mapper 26) should be implemented");
        m26.write_prg(0xD001, 7);
        assert_eq!(m26.read_chr(0x0400), 0);
        assert_eq!(m26.read_chr(0x0800), 7);

        // And writing $D002 should then target internal R1.
        m26.write_prg(0xD002, 9);
        assert_eq!(m26.read_chr(0x0400), 9);
    }
}
