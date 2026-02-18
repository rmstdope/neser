//! Mapper 9 - MMC2
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MapperCapabilities, MirroringMode};

/// Mapper 9 - MMC2 (PNROM boards)
///
/// Hardware: Nintendo's PPU-triggered CHR banking used exclusively by Punch-Out!!
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC2>
/// - Latch behavior: <https://www.nesdev.org/wiki/MMC2#Latch_Behavior>
/// - PRG-ROM: 128KB (8KB switchable + 24KB fixed)
/// - PRG-RAM: None
/// - CHR-ROM: 128KB with two 4KB regions controlled by PPU address latches
/// - Mirroring: Programmable (horizontal or vertical)
///
/// Common boards: NES-PNROM
///
/// Notes:
/// - Two independent CHR latches triggered by PPU reads:
///   - Latch 0: $0FD8-$0FDF (FD) or $0FE8-$0FEF (FE) switches $0000-$0FFF
///   - Latch 1: $1FD8-$1FDF (FD) or $1FE8-$1FEF (FE) switches $1000-$1FFF
/// - Latch state determines which of two 4KB CHR banks is active per region
/// - Used exclusively in (Mike Tyson's) Punch-Out!!
pub struct MMC2Mapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,

    chr_memory: ChrMemory,

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
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
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
        self.chr_memory.size() / Self::CHR_BANK_SIZE
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
        self.chr_memory.read_at_index(addr)
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
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        let index = bank * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.write_at_index(index, value);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        // MMC2 latches are clocked by reads in the pattern tables. We approximate
        // this by updating latches on address bus activity.
        self.update_latches_for_chr_read(addr);
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        9
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
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize MMC2 internal registers:
        // [0]: prg_bank_8k
        // [1]: chr_bank_0_fd
        // [2]: chr_bank_0_fe
        // [3]: chr_bank_1_fd
        // [4]: chr_bank_1_fe
        // [5]: latches (bit 0 = latch0_is_fd, bit 1 = latch1_is_fd)
        // [6]: mirroring
        vec![
            self.prg_bank_8k,
            self.chr_bank_0_fd,
            self.chr_bank_0_fe,
            self.chr_bank_1_fd,
            self.chr_bank_1_fe,
            (self.latch0_is_fd as u8) | ((self.latch1_is_fd as u8) << 1),
            match self.mirroring {
                MirroringMode::Vertical => 0,
                MirroringMode::Horizontal => 1,
                _ => 1,
            },
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 7 {
            self.prg_bank_8k = data[0];
            self.chr_bank_0_fd = data[1];
            self.chr_bank_0_fe = data[2];
            self.chr_bank_1_fd = data[3];
            self.chr_bank_1_fe = data[4];
            self.latch0_is_fd = (data[5] & 1) != 0;
            self.latch1_is_fd = (data[5] & 2) != 0;
            self.mirroring = match data[6] {
                0 => MirroringMode::Vertical,
                1 => MirroringMode::Horizontal,
                _ => MirroringMode::Horizontal,
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
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
    fn test_mmc2_prg_bank_8000_is_switchable_and_upper_banks_are_fixed() {
        let prg_banks = 8;
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, prg_banks);
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);

        let mapper = MMC2Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Power-on: bank 0 at $8000.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Fixed region should map to last 3 banks.
        assert_eq!(mapper.read_prg(0xA000), (prg_banks - 3) as u8);
        assert_eq!(mapper.read_prg(0xC000), (prg_banks - 2) as u8);
        assert_eq!(mapper.read_prg(0xE000), (prg_banks - 1) as u8);
    }

    #[test]
    fn test_mmc2_registers_snapshot_preserves_latches_and_mirroring() {
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, 4);
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Vertical);

        mapper.write_prg(0xA000, 0x03); // PRG bank
        mapper.write_prg(0xB000, 0x01); // CHR 0 FD
        mapper.write_prg(0xC000, 0x02); // CHR 0 FE
        mapper.write_prg(0xD000, 0x03); // CHR 1 FD
        mapper.write_prg(0xE000, 0x04); // CHR 1 FE
        mapper.write_prg(0xF000, 0x01); // Mirroring horizontal

        mapper.ppu_address_changed(0x0FD8); // latch0 FD
        mapper.ppu_address_changed(0x1FE8); // latch1 FE

        let saved = mapper.registers_snapshot();

        let mut restored = MMC2Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);
        restored.restore_registers(&saved);

        assert_eq!(restored.get_mirroring(), MirroringMode::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 1);
        assert_eq!(restored.read_chr(0x1000), 4);
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

    #[test]
    fn test_mmc2_open_bus() {
        let mapper = MMC2Mapper::new(
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            MirroringMode::Horizontal,
        );

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x11), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x22), 0x22);
    }
}
