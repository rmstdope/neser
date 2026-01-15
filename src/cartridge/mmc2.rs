use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};

/// MMC2 mapper (iNES Mapper 9)
///
/// Used by (Mike Tyson's) Punch-Out!!
///
/// Key behavior:
/// - PRG: one switchable 8KB bank at $8000-$9FFF, fixed last 24KB at $A000-$FFFF
/// - CHR: two 4KB regions ($0000-$0FFF and $1000-$1FFF) selected via latches
///   - Latch 0 triggers on PPU reads to $0FD8-$0FDF (select FD) and $0FE8-$0FEF (select FE)
///   - Latch 1 triggers on PPU reads to $1FD8-$1FDF (select FD) and $1FE8-$1FEF (select FE)
/// - Mirroring: horizontal/vertical control via register write
pub struct MMC2Mapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,

    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    has_chr_ram: bool,

    mirroring: MirroringMode,

    // --- PRG banking ---
    prg_bank_8k: u8,

    // --- CHR banking + latches ---
    chr_bank_0_fd: u8,
    chr_bank_0_fe: u8,
    chr_bank_1_fd: u8,
    chr_bank_1_fe: u8,

    latch0_is_fd: bool,
    latch1_is_fd: bool,
}

impl MMC2Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_SIZE: usize = 0x1000; // 4KB

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let has_chr_ram = chr_rom.is_empty();
        let chr_ram = if has_chr_ram {
            vec![0u8; 0x2000]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_rom,
            chr_ram,
            has_chr_ram,
            mirroring,
            prg_bank_8k: 0,
            chr_bank_0_fd: 0,
            chr_bank_0_fe: 0,
            chr_bank_1_fd: 0,
            chr_bank_1_fe: 0,
            // Power-on latch state is hardware-defined; FE is a common default in emulators.
            latch0_is_fd: false,
            latch1_is_fd: false,
        }
    }

    fn prg_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn chr_bank_count_4k(&self) -> usize {
        let chr_len = if self.has_chr_ram {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        chr_len / Self::CHR_BANK_SIZE
    }

    fn clamp_prg_bank_8k(&self, bank: u8) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn clamp_chr_bank_4k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_4k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn read_prg_rom_bank(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_bank_4k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE + bank_offset;
        if self.has_chr_ram {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        }
    }

    fn chr_bank_for_addr(&self, addr: u16) -> usize {
        let bank = if addr < 0x1000 {
            if self.latch0_is_fd {
                self.chr_bank_0_fd
            } else {
                self.chr_bank_0_fe
            }
        } else if self.latch1_is_fd {
            self.chr_bank_1_fd
        } else {
            self.chr_bank_1_fe
        };

        self.clamp_chr_bank_4k(bank)
    }

    fn update_latches_for_chr_read(&mut self, addr: u16) {
        match addr {
            0x0FD8..=0x0FDF => self.latch0_is_fd = true,
            0x0FE8..=0x0FEF => self.latch0_is_fd = false,
            0x1FD8..=0x1FDF => self.latch1_is_fd = true,
            0x1FE8..=0x1FEF => self.latch1_is_fd = false,
            _ => {}
        }
    }
}

impl Mapper for MMC2Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0x9FFF => {
                let bank = self.clamp_prg_bank_8k(self.prg_bank_8k);
                let offset = (addr - 0x8000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            0xA000..=0xBFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(3);
                let offset = (addr - 0xA000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            0xC000..=0xDFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(2);
                let offset = (addr - 0xC000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            0xE000..=0xFFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(1);
                let offset = (addr - 0xE000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        match addr {
            // PRG bank select ($A000-$AFFF)
            0xA000..=0xAFFF => {
                self.prg_bank_8k = value & 0x0F;
            }

            // CHR bank registers
            0xB000..=0xBFFF => self.chr_bank_0_fd = value & 0x1F,
            0xC000..=0xCFFF => self.chr_bank_0_fe = value & 0x1F,
            0xD000..=0xDFFF => self.chr_bank_1_fd = value & 0x1F,
            0xE000..=0xEFFF => self.chr_bank_1_fe = value & 0x1F,

            // Mirroring control ($F000-$FFFF)
            0xF000..=0xFFFF => {
                self.mirroring = if (value & 0x01) != 0 {
                    MirroringMode::Horizontal
                } else {
                    MirroringMode::Vertical
                };
            }

            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        // Latches are updated on PPU reads. This requires internal mutation, so we
        // implement latch updates via ppu_address_changed for now as a no-op and
        // do latch updates by taking &mut self in write_chr/read_chr below.
        //
        // Since the trait signature is `&self`, we rely on the fact that the rest of
        // the emulator calls `ppu_address_changed` on address changes for latch-like
        // mechanisms (MMC3). For MMC2, we update latches in `ppu_address_changed`.
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        self.read_chr_bank_4k(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram {
            return;
        }
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        let index = bank * Self::CHR_BANK_SIZE + offset;
        if let Some(byte) = self.chr_ram.get_mut(index) {
            *byte = value;
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        // MMC2 latches are clocked by reads in the pattern tables. We approximate
        // this by updating latches on address bus activity.
        self.update_latches_for_chr_read(addr);
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
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
    use super::*;

    fn filled_banks(bank_size: usize, banks: usize) -> Vec<u8> {
        (0..banks)
            .flat_map(|bank| vec![bank as u8; bank_size])
            .collect()
    }

    #[test]
    fn test_mmc2_prg_bank_8000_is_switchable_and_upper_banks_are_fixed() {
        let prg_banks = 8;
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, prg_banks);
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Power-on: bank 0 at $8000.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Fixed region should map to last 3 banks.
        assert_eq!(mapper.read_prg(0xA000), (prg_banks - 3) as u8);
        assert_eq!(mapper.read_prg(0xC000), (prg_banks - 2) as u8);
        assert_eq!(mapper.read_prg(0xE000), (prg_banks - 1) as u8);

        // Switch $8000 bank via $A000.
        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0xA123, 7);
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    #[test]
    fn test_mmc2_chr_latches_select_between_fd_and_fe_banks() {
        // Provide at least 6 4KB banks.
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Configure banks.
        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE
        mapper.write_prg(0xD000, 3); // high FD
        mapper.write_prg(0xE000, 4); // high FE

        // Low region latch: FD
        mapper.ppu_address_changed(0x0FD8);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Low region latch: FE
        mapper.ppu_address_changed(0x0FE8);
        assert_eq!(mapper.read_chr(0x0000), 2);

        // High region latch: FD
        mapper.ppu_address_changed(0x1FD8);
        assert_eq!(mapper.read_chr(0x1000), 3);

        // High region latch: FE
        mapper.ppu_address_changed(0x1FE8);
        assert_eq!(mapper.read_chr(0x1000), 4);
    }
}
