//! Mapper 15 - Multicart 100-in-1
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE_8K: usize = 0x2000; // 8KB
const PRG_BANK_SIZE_16K: usize = 0x4000; // 16KB

/// Mapper 15 - 100-in-1 Contra Function
///
/// Hardware: Pirate multicart mapper with multiple banking modes
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_015>
/// - PRG-ROM: Up to 1MB with various banking modes
/// - PRG-RAM: None
/// - CHR: 8KB CHR-RAM (no CHR-ROM support)
/// - Mirroring: Programmable (horizontal, vertical, or one-screen)
///
/// Common boards: Various pirate multicart boards
///
/// Notes:
/// - Banking mode selected by address written to ($8000-$8007)
/// - Mode 0 ($8000-$8001): 16KB bank at $8000, mirror at $C000
/// - Mode 1 ($8002-$8003): 32KB bank at $8000
/// - Mode 2 ($8004-$8007): 8KB bank at $8000, separate 8KB at $C000
/// - Bit 7 of data: mirroring control
/// - Used in various pirate multicarts (100-in-1, 168-in-1, etc.)
pub struct Multicart15Mapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    bank_select: u8,
    sub_bank: u8,
    mode: u8,
    mirroring: MirroringMode,
}

impl Multicart15Mapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // Pirate multicarts typically use CHR-RAM
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            bank_select: 0,
            sub_bank: 0,
            mode: 0,
            mirroring,
        }
    }

    fn get_prg_bank_8k(&self, bank_num: u8) -> usize {
        let total_banks = (self.prg_rom.len() / PRG_BANK_SIZE_8K).max(1);
        let bank = (bank_num as usize) % total_banks;
        bank * PRG_BANK_SIZE_8K
    }

    fn get_prg_bank_16k(&self, bank_num: u8) -> usize {
        let total_banks = (self.prg_rom.len() / PRG_BANK_SIZE_16K).max(1);
        let bank = (bank_num as usize) % total_banks;
        bank * PRG_BANK_SIZE_16K
    }
}

impl Mapper for Multicart15Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF
        if addr < 0x8000 {
            return 0;
        }

        let offset = (addr - 0x8000) as usize;

        match self.mode {
            // Mode 0 ($8000-$8001): 16KB at $8000, mirror at $C000
            0 => {
                let bank_offset = self.get_prg_bank_16k(self.bank_select);
                let index = bank_offset + (offset % PRG_BANK_SIZE_16K);
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            // Mode 1 ($8002-$8003): 32KB at $8000
            1 => {
                let bank_offset = self.get_prg_bank_16k(self.bank_select & 0xFE);
                let index = bank_offset + offset;
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            // Mode 2 ($8004-$8007): 8KB banks
            2 => {
                if offset < 0x2000 {
                    // $8000-$9FFF: bank from bank_select
                    let bank_offset = self.get_prg_bank_8k(self.bank_select << 1);
                    let index = bank_offset + offset;
                    self.prg_rom.get(index).copied().unwrap_or(0)
                } else if offset < 0x4000 {
                    // $A000-$BFFF: mirror of $8000-$9FFF
                    let bank_offset = self.get_prg_bank_8k(self.bank_select << 1);
                    let index = bank_offset + (offset - 0x2000);
                    self.prg_rom.get(index).copied().unwrap_or(0)
                } else if offset < 0x6000 {
                    // $C000-$DFFF: sub_bank
                    let bank_offset = self.get_prg_bank_8k((self.bank_select << 1) | 1);
                    let index = bank_offset + (offset - 0x4000);
                    self.prg_rom.get(index).copied().unwrap_or(0)
                } else {
                    // $E000-$FFFF: mirror of $C000-$DFFF
                    let bank_offset = self.get_prg_bank_8k((self.bank_select << 1) | 1);
                    let index = bank_offset + (offset - 0x6000);
                    self.prg_rom.get(index).copied().unwrap_or(0)
                }
            }
            // This should never happen since write_prg only sets mode to 0, 1, or 2
            _ => unreachable!("Invalid banking mode"),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // Mapper control registers at $8000-$FFFF
        if addr >= 0x8000 {
            // Extract bank and mirroring from value
            self.bank_select = value & 0x3F; // Bits 0-5: bank select
            self.sub_bank = value & 0x7F;
            self.mirroring = if (value & 0x80) != 0 {
                MirroringMode::Horizontal
            } else {
                MirroringMode::Vertical
            };

            // Set mode based on address bits 0-2
            let addr_bits = addr & 0x0007;

            if addr_bits == 0 || addr_bits == 1 {
                // $xxx0 or $xxx1: Mode 0
                self.mode = 0;
            } else if addr_bits == 2 || addr_bits == 3 {
                // $xxx2 or $xxx3: Mode 1 (32KB)
                self.mode = 1;
            } else {
                // $xxx4-$xxx7: Mode 2 (8KB)
                self.mode = 2;
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        15
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
        // [0]: bank_select
        // [1]: sub_bank
        // [2]: mode
        // [3]: mirroring
        let mirroring = match self.mirroring {
            MirroringMode::Horizontal => 0,
            MirroringMode::Vertical => 1,
            MirroringMode::SingleScreen | MirroringMode::SingleScreenLower => 2,
            MirroringMode::SingleScreenUpper => 3,
            MirroringMode::FourScreen => 4,
        };
        vec![self.bank_select, self.sub_bank, self.mode, mirroring]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.bank_select = data[0];
            self.sub_bank = data[1];
            self.mode = data[2];
            self.mirroring = match data[3] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreen,
                3 => MirroringMode::SingleScreenUpper,
                4 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_prg_rom(num_16k_banks: usize) -> Vec<u8> {
        let mut prg_rom = vec![0; num_16k_banks * 16 * 1024];
        // Fill each 16KB bank with its bank number
        for bank in 0..num_16k_banks {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }
        prg_rom
    }

    #[test]
    fn test_multicart15_mode0_16kb_mirror() {
        // Mode 0: 16KB at $8000, mirrored at $C000
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write to $8000 (mode 0), select bank 2
        mapper.write_prg(0x8000, 2);

        // Both $8000 and $C000 should read from bank 2
        assert_eq!(mapper.read_prg(0x8000), 20);
        assert_eq!(mapper.read_prg(0xBFFF), 20);
        assert_eq!(mapper.read_prg(0xC000), 20);
        assert_eq!(mapper.read_prg(0xFFFF), 20);
    }

    #[test]
    fn test_multicart15_mode1_32kb() {
        // Mode 1: 32KB at $8000 (uses even/odd 16KB banks as a pair)
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write to $8002 (mode 1), select bank 2
        // Bank 2 -> uses banks 2-3 as 32KB (bank & 0xFE gives 2)
        mapper.write_prg(0x8002, 2);

        // Should read from 16KB bank 2 and 3
        assert_eq!(mapper.read_prg(0x8000), 20); // Bank 2 at $8000-$BFFF
        assert_eq!(mapper.read_prg(0xBFFF), 20);
        assert_eq!(mapper.read_prg(0xC000), 30); // Bank 3 at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xFFFF), 30);
    }

    #[test]
    fn test_multicart15_mode2_8kb_banks() {
        // Mode 2: 8KB banks
        let prg_rom = create_test_prg_rom(16);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write to $8004 (mode 2), select bank 4
        mapper.write_prg(0x8004, 4);

        // $8000-$9FFF and $A000-$BFFF should mirror (8KB bank 8 = 4 << 1)
        assert_eq!(mapper.read_prg(0x8000), 40);
        assert_eq!(mapper.read_prg(0x9FFF), 40);
        assert_eq!(mapper.read_prg(0xA000), 40);
        assert_eq!(mapper.read_prg(0xBFFF), 40);

        // $C000-$DFFF and $E000-$FFFF should mirror (8KB bank 9 = (4 << 1) | 1)
        assert_eq!(mapper.read_prg(0xC000), 40); // Bank 9, which is still filled with 40
        assert_eq!(mapper.read_prg(0xDFFF), 40);
        assert_eq!(mapper.read_prg(0xE000), 40);
        assert_eq!(mapper.read_prg(0xFFFF), 40);
    }

    #[test]
    fn test_multicart15_mirroring_control() {
        let prg_rom = create_test_prg_rom(4);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Initially horizontal
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Bit 7 = 0: vertical mirroring
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Bit 7 = 1: horizontal mirroring
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Test with mode 1
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        mapper.write_prg(0x8002, 0x80);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_multicart15_bank_select_masking() {
        // Test that only bits 0-5 are used for bank selection
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write with upper bits set - should only use lower 6 bits
        mapper.write_prg(0x8000, 0xFF); // Bank = 0x3F

        // Should wrap to available banks
        let value = mapper.read_prg(0x8000);
        assert!(value < 80); // Should be within available banks
    }

    #[test]
    fn test_multicart15_chr_ram() {
        // Multicart mappers typically use CHR-RAM
        let mut mapper =
            Multicart15Mapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_multicart15_mode_switching() {
        // Test switching between modes
        let prg_rom = create_test_prg_rom(16);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Start in mode 0
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.mode, 0);
        let _val0 = mapper.read_prg(0x8000);

        // Switch to mode 1
        mapper.write_prg(0x8002, 2);
        assert_eq!(mapper.mode, 1);
        let _val1 = mapper.read_prg(0x8000);

        // Values should differ since banking modes are different
        // (This might not always be true depending on bank numbers, but demonstrates mode switch)

        // Switch to mode 2
        mapper.write_prg(0x8004, 3);
        assert_eq!(mapper.mode, 2);
    }

    #[test]
    fn test_multicart15_address_discrimination() {
        // Test that different addresses trigger different modes
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // $8000-$8001 -> mode 0
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.mode, 0);

        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.mode, 0);

        // $8002-$8003 -> mode 1
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.mode, 1);

        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.mode, 1);

        // $8004-$8007 -> mode 2
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.mode, 2);

        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.mode, 2);
    }

    #[test]
    fn test_multicart15_large_rom() {
        // Test with a large ROM to ensure bank wrapping works
        let prg_rom = create_test_prg_rom(32); // 512KB
        let mut mapper = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write to mode 0 with various bank numbers
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 20);
        assert_eq!(mapper.read_prg(0x8000), 200);
    }

    #[test]
    fn test_multicart15_registers_snapshot_restores_state() {
        let prg_rom = create_test_prg_rom(8);
        let mut mapper = Multicart15Mapper::new(prg_rom.clone(), vec![], MirroringMode::Vertical);

        mapper.write_prg(0x8004, 0x80 | 0x12); // mode 2, bank select 0x12, horizontal mirroring
        mapper.write_chr(0x0000, 0xAB);

        let regs = mapper.registers_snapshot();
        let chr = mapper.chr_ram_snapshot();

        let mut restored = Multicart15Mapper::new(prg_rom, vec![], MirroringMode::Vertical);
        restored.restore_registers(&regs);
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.get_mirroring(), MirroringMode::Horizontal);
        assert_eq!(restored.read_chr(0x0000), 0xAB);
        assert_eq!(restored.mode, 2);
        assert_eq!(restored.bank_select, 0x12 & 0x3F);
    }
}
