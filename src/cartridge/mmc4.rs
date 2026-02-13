use std::cell::Cell;

use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};

/// Mapper 10 - MMC4 (FxROM boards)
///
/// Hardware: Similar to MMC2 but with 16KB PRG banking instead of 8KB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC4>
/// - Latch behavior: Same as MMC2, see <https://www.nesdev.org/wiki/MMC2#Latch_Behavior>
/// - PRG-ROM: 128KB or 256KB (16KB switchable + 16KB fixed)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR-ROM: 128KB with two 4KB regions controlled by PPU address latches
/// - Mirroring: Programmable (horizontal or vertical)
///
/// Common boards: NES-FxROM
///
/// Notes:
/// - Same CHR latch mechanism as MMC2 (FD/FE switching)
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - Last 16KB PRG bank fixed at $C000-$FFFF
/// - Used in Fire Emblem (Japan), Fire Emblem Gaiden (Japan)
pub struct MMC4Mapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,

    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    has_chr_ram: bool,

    mirroring: MirroringMode,

    // --- PRG banking ---
    prg_bank_16k: u8,

    // --- CHR banking + latches ---
    chr_bank_0_fd: u8,
    chr_bank_0_fe: u8,
    chr_bank_1_fd: u8,
    chr_bank_1_fe: u8,

    latch0_is_fd: Cell<bool>,
    latch1_is_fd: Cell<bool>,
}

impl MMC4Mapper {
    const PRG_BANK_SIZE: usize = 0x4000; // 16KB
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
            prg_bank_16k: 0,
            chr_bank_0_fd: 0,
            chr_bank_0_fe: 0,
            chr_bank_1_fd: 0,
            chr_bank_1_fe: 0,
            latch0_is_fd: Cell::new(false),
            latch1_is_fd: Cell::new(false),
        }
    }

    fn prg_bank_count_16k(&self) -> usize {
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

    fn clamp_prg_bank_16k(&self, bank: u8) -> usize {
        let count = self.prg_bank_count_16k();
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
            if self.latch0_is_fd.get() {
                self.chr_bank_0_fd
            } else {
                self.chr_bank_0_fe
            }
        } else if self.latch1_is_fd.get() {
            self.chr_bank_1_fd
        } else {
            self.chr_bank_1_fe
        };

        self.clamp_chr_bank_4k(bank)
    }

    fn update_latches(&self, addr: u16) {
        match addr {
            0x0FD8..=0x0FDF => self.latch0_is_fd.set(true),
            0x0FE8..=0x0FEF => self.latch0_is_fd.set(false),
            0x1FD8..=0x1FDF => self.latch1_is_fd.set(true),
            0x1FE8..=0x1FEF => self.latch1_is_fd.set(false),
            _ => {}
        }
    }
}

impl Mapper for MMC4Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0xBFFF => {
                let bank = self.clamp_prg_bank_16k(self.prg_bank_16k);
                let offset = (addr - 0x8000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            0xC000..=0xFFFF => {
                let count = self.prg_bank_count_16k();
                let bank = count.saturating_sub(1);
                let offset = (addr - 0xC000) as usize;
                self.read_prg_rom_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        match addr {
            0xA000..=0xAFFF => {
                self.prg_bank_16k = value & 0x0F;
            }
            0xB000..=0xBFFF => self.chr_bank_0_fd = value & 0x1F,
            0xC000..=0xCFFF => self.chr_bank_0_fe = value & 0x1F,
            0xD000..=0xDFFF => self.chr_bank_1_fd = value & 0x1F,
            0xE000..=0xEFFF => self.chr_bank_1_fe = value & 0x1F,
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
        self.update_latches(addr);

        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        self.read_chr_bank_4k(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.update_latches(addr);

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
        self.update_latches(addr);
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        10
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
        if self.has_chr_ram {
            self.chr_ram.clone()
        } else {
            Vec::new()
        }
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        if self.has_chr_ram && !data.is_empty() {
            let to_copy = data.len().min(self.chr_ram.len());
            self.chr_ram[..to_copy].copy_from_slice(&data[..to_copy]);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize MMC4 internal registers:
        // [0]: prg_bank_16k
        // [1]: chr_bank_0_fd
        // [2]: chr_bank_0_fe
        // [3]: chr_bank_1_fd
        // [4]: chr_bank_1_fe
        // [5]: latches (bit 0 = latch0_is_fd, bit 1 = latch1_is_fd)
        // [6]: mirroring
        vec![
            self.prg_bank_16k,
            self.chr_bank_0_fd,
            self.chr_bank_0_fe,
            self.chr_bank_1_fd,
            self.chr_bank_1_fe,
            (self.latch0_is_fd.get() as u8) | ((self.latch1_is_fd.get() as u8) << 1),
            self.mirroring as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 7 {
            self.prg_bank_16k = data[0];
            self.chr_bank_0_fd = data[1];
            self.chr_bank_0_fe = data[2];
            self.chr_bank_1_fd = data[3];
            self.chr_bank_1_fe = data[4];
            self.latch0_is_fd.set((data[5] & 1) != 0);
            self.latch1_is_fd.set((data[5] & 2) != 0);
            self.mirroring = match data[6] {
                0 => MirroringMode::Vertical,
                1 => MirroringMode::Horizontal,
                _ => MirroringMode::Horizontal,
            };
        }
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
    fn test_mmc4_prg_bank_8000_is_switchable_and_upper_bank_fixed() {
        let prg_banks = 4;
        let prg_rom = filled_banks(MMC4Mapper::PRG_BANK_SIZE, prg_banks);
        let chr_rom = filled_banks(MMC4Mapper::CHR_BANK_SIZE, 8);

        let mut mapper = MMC4Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Power-on: bank 0 at $8000.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Fixed region should map to last bank.
        assert_eq!(mapper.read_prg(0xC000), (prg_banks - 1) as u8);
        assert_eq!(mapper.read_prg(0xFFFF), (prg_banks - 1) as u8);

        // Switch $8000-$BFFF bank via $A000.
        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0xA999, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn test_mmc4_chr_latches_select_between_fd_and_fe_banks() {
        let chr_rom = filled_banks(MMC4Mapper::CHR_BANK_SIZE, 8);
        let prg_rom = filled_banks(MMC4Mapper::PRG_BANK_SIZE, 4);

        let mut mapper = MMC4Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

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

    #[test]
    fn test_mmc4_registers_snapshot_preserves_latches_and_mirroring() {
        let prg_rom = filled_banks(MMC4Mapper::PRG_BANK_SIZE, 4);
        let chr_rom = filled_banks(MMC4Mapper::CHR_BANK_SIZE, 8);

        let mut mapper = MMC4Mapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Vertical);

        mapper.write_prg(0xA000, 0x03); // PRG bank
        mapper.write_prg(0xB000, 0x01); // CHR 0 FD
        mapper.write_prg(0xC000, 0x02); // CHR 0 FE
        mapper.write_prg(0xD000, 0x03); // CHR 1 FD
        mapper.write_prg(0xE000, 0x04); // CHR 1 FE
        mapper.write_prg(0xF000, 0x01); // Mirroring horizontal

        mapper.ppu_address_changed(0x0FD8); // latch0 FD
        mapper.ppu_address_changed(0x1FE8); // latch1 FE

        let saved = mapper.registers_snapshot();

        let mut restored = MMC4Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);
        restored.restore_registers(&saved);

        assert_eq!(restored.get_mirroring(), MirroringMode::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 1);
        assert_eq!(restored.read_chr(0x1000), 4);
    }
}
