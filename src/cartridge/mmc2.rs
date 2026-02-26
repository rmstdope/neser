//! Mapper 9 - MMC2
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::ines::ConsoleType;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

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
    prg_ram: Option<PrgRam>,

    chr_memory: ChrMemory,

    mirroring: NametableLayout,

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

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_ram = matches!(ctx.console_type, ConsoleType::Playchoice10)
            .then(|| PrgRam::new(DEFAULT_PRG_RAM_SIZE));
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            prg_rom,
            prg_ram,
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

    fn read_prg_window_8k(&self, addr: u16, window_start: u16, bank_index: usize) -> u8 {
        let offset = (addr - window_start) as usize;
        self.read_prg_rom_bank(bank_index, offset)
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
            0x0FD8 => self.latch0_is_fd = true,
            0x0FE8 => self.latch0_is_fd = false,
            0x1FD8..=0x1FDF => self.latch1_is_fd = true,
            0x1FE8..=0x1FEF => self.latch1_is_fd = false,
            _ => {}
        }
    }
}

impl Mapper for MMC2Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(prg_ram) = &self.prg_ram
            && let Some(value) = prg_ram.try_read(addr)
        {
            return value;
        }

        match addr {
            0x8000..=0x9FFF => {
                let bank = self.clamp_prg_bank_8k(self.prg_bank_8k);
                self.read_prg_window_8k(addr, 0x8000, bank)
            }
            0xA000..=0xBFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(3);
                self.read_prg_window_8k(addr, 0xA000, bank)
            }
            0xC000..=0xDFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(2);
                self.read_prg_window_8k(addr, 0xC000, bank)
            }
            0xE000..=0xFFFF => {
                let count = self.prg_bank_count_8k();
                let bank = count.saturating_sub(1);
                self.read_prg_window_8k(addr, 0xE000, bank)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if let Some(prg_ram) = &mut self.prg_ram
            && prg_ram.try_write(addr, value)
        {
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
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                };
            }

            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        let value = self.read_chr_bank_4k(bank, offset);
        self.update_latches_for_chr_read(addr);
        value
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & (Self::CHR_BANK_SIZE - 1);
        let index = bank * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.write_at_index(index, value);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        let _ = addr;
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        9
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
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
                NametableLayout::Vertical => 0,
                NametableLayout::Horizontal => 1,
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
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                _ => NametableLayout::Horizontal,
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: if self.prg_ram.is_some() { 8 } else { 0 },
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;

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

        let mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

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

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xA000, 0x03); // PRG bank
        mapper.write_prg(0xB000, 0x01); // CHR 0 FD
        mapper.write_prg(0xC000, 0x02); // CHR 0 FE
        mapper.write_prg(0xD000, 0x03); // CHR 1 FD
        mapper.write_prg(0xE000, 0x04); // CHR 1 FE
        mapper.write_prg(0xF000, 0x01); // Mirroring horizontal

        mapper.read_chr(0x0FD8); // latch0 FD after read
        mapper.read_chr(0x1FE8); // latch1 FE after read

        let saved = mapper.registers_snapshot();

        let mut restored = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&saved);

        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 1);
        assert_eq!(restored.read_chr(0x1000), 4);
    }

    #[test]
    fn test_mmc2_chr_latches_select_between_fd_and_fe_banks() {
        // Provide at least 6 4KB banks.
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Configure banks.
        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE
        mapper.write_prg(0xD000, 3); // high FD
        mapper.write_prg(0xE000, 4); // high FE

        // Triggering read uses old bank for that fetch, then switches latch.
        assert_eq!(mapper.read_chr(0x0FD8), 2);
        assert_eq!(mapper.read_chr(0x0000), 1);

        assert_eq!(mapper.read_chr(0x0FE8), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);

        assert_eq!(mapper.read_chr(0x1FD8), 4);
        assert_eq!(mapper.read_chr(0x1000), 3);

        assert_eq!(mapper.read_chr(0x1FE8), 3);
        assert_eq!(mapper.read_chr(0x1000), 4);
    }

    #[test]
    fn test_mmc2_latch0_only_switches_on_exact_addresses() {
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1); // low FD
        mapper.write_prg(0xC000, 2); // low FE

        // Power-on FE bank selected.
        assert_eq!(mapper.read_chr(0x0000), 2);

        // Neighbor of FD trigger must not switch latch0.
        assert_eq!(mapper.read_chr(0x0FDF), 2);
        assert_eq!(mapper.read_chr(0x0000), 2);

        // Exact FD trigger should switch to FD for subsequent reads.
        assert_eq!(mapper.read_chr(0x0FD8), 2);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Neighbor of FE trigger must not switch latch0.
        assert_eq!(mapper.read_chr(0x0FEF), 1);
        assert_eq!(mapper.read_chr(0x0000), 1);

        // Exact FE trigger should switch to FE for subsequent reads.
        assert_eq!(mapper.read_chr(0x0FE8), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn test_mmc2_ppu_address_changed_does_not_switch_latches() {
        let chr_rom = filled_banks(MMC2Mapper::CHR_BANK_SIZE, 8);
        let prg_rom = filled_banks(MMC2Mapper::PRG_BANK_SIZE, 8);

        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xB000, 1);
        mapper.write_prg(0xC000, 2);

        // Power-on FE bank selected.
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.ppu_address_changed(0x0FD8);

        // Still FE because address bus activity alone must not switch latch.
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn test_mmc2_open_bus() {
        let mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x11), 0x11);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x22), 0x22);
    }

    #[test]
    fn test_mmc2_standard_board_has_no_prg_ram_window() {
        let mut mapper = MMC2Mapper::new(MapperContext::new_for_test(
            9,
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x6000, 0xA5);

        assert_eq!(mapper.wram_size(), 0);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0x3C), 0x3C);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, 0x7E), 0x7E);
    }
}
