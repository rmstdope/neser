use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE: usize = 0x4000; // 16KB

/// Mapper 71 - Camerica/Codemasters
///
/// Similar to UxROM (Mapper 2) but with programmable 1-screen mirroring.
/// Supports:
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - 16KB fixed PRG bank at $C000-$FFFF (always last bank)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 8KB CHR-RAM (no CHR ROM banking)
/// - Bank select register at $8000-$BFFF (any write)
/// - Mirroring control register at $C000-$FFFF (any write)
///
/// Register at $8000-$BFFF (PRG bank select):
/// - Bits 0-3: Select 16KB PRG ROM bank (supports up to 16 banks = 256KB)
/// - Other bits ignored
///
/// Register at $C000-$FFFF (mirroring control):
/// - Bit 4: One-screen mirroring select (0 = lower/A, 1 = upper/B)
/// - Other bits ignored
///
/// Used in games like Micro Machines, Fire Hawk, Dizzy series.
pub struct CamericaMapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    bank_select: u8,
    one_screen_upper: bool, // true = upper nametable, false = lower nametable
}

impl CamericaMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, _mirroring: MirroringMode) -> Self {
        // Mapper 71 uses CHR-RAM, ignores chr_rom and initial mirroring (controlled by register)
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new_ram(8192),
            bank_select: 0,
            one_screen_upper: false, // Default to lower nametable
        }
    }

    fn get_last_bank_offset(&self) -> usize {
        self.prg_rom.len().saturating_sub(PRG_BANK_SIZE)
    }
}

impl Mapper for CamericaMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF
        match addr {
            0x8000..=0xBFFF => {
                // Switchable 16KB bank
                let offset = (addr - 0x8000) as usize;
                let bank_offset = (self.bank_select as usize) * PRG_BANK_SIZE;
                let index = bank_offset + offset;
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            0xC000..=0xFFFF => {
                // Fixed to last 16KB bank
                let offset = (addr - 0xC000) as usize;
                let last_bank_offset = self.get_last_bank_offset();
                let index = last_bank_offset + offset;
                self.prg_rom.get(index).copied().unwrap_or(0)
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
            0x8000..=0xBFFF => {
                // PRG bank select (bits 0-3)
                self.bank_select = value & 0x0F;
            }
            0xC000..=0xFFFF => {
                // Mirroring control (bit 4)
                self.one_screen_upper = (value & 0x10) != 0;
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // Mapper 71 doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
        // Bit 4 of mirroring register determines one-screen mode
        if self.one_screen_upper {
            MirroringMode::SingleScreenUpper
        } else {
            MirroringMode::SingleScreenLower
        }
    }

    fn mapper_number(&self) -> u8 {
        71
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
    use crate::cartridge::mapper::create_mapper;

    #[test]
    fn test_mapper_71_is_wired_in_factory() {
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![];
        let mapper = create_mapper(71, prg_rom, chr_rom, MirroringMode::Horizontal);
        assert!(mapper.is_ok(), "Mapper 71 should be implemented");
    }

    #[test]
    fn test_mapper71_prg_bank_switching() {
        // Create 256KB (16 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each bank with its bank number
        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Initially bank 0 should be at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Last bank (15) should always be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 15);
        assert_eq!(mapper.read_prg(0xFFFF), 15);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 3);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 10
        mapper.write_prg(0x9000, 10);
        assert_eq!(mapper.read_prg(0x8000), 10);

        // Last bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn test_mapper71_bank_register_mask() {
        // Test that only bits 0-3 affect bank selection
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Test that upper bits are masked off
        mapper.write_prg(0x8000, 0b1111_0101); // Should select bank 5
        assert_eq!(mapper.read_prg(0x8000), 50);

        mapper.write_prg(0x8000, 0b0000_1111); // Bank 15
        assert_eq!(mapper.read_prg(0x8000), 150);
    }

    #[test]
    fn test_mapper71_one_screen_mirroring() {
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Default should be lower nametable
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Write to $C000-$FFFF with bit 4 = 0 (lower nametable)
        mapper.write_prg(0xC000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0xC000, 0b0001_0000);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);

        // Write with bit 4 = 0 again
        mapper.write_prg(0xD000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Test that other bits don't affect mirroring
        mapper.write_prg(0xE000, 0b0001_1111); // Bit 4 = 1 (0x1F)
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);

        mapper.write_prg(0xFFFF, 0b1110_1111); // Bit 4 = 0 (0xEF)
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);
    }

    #[test]
    fn test_mapper71_chr_ram() {
        // Mapper 71 uses 8KB CHR-RAM
        let mut mapper =
            CamericaMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_mapper71_fixed_last_bank() {
        // Verify that $C000-$FFFF is always the last bank regardless of switches
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Last bank should always read 115 (bank 15 + 100)
        assert_eq!(mapper.read_prg(0xC000), 115);
        assert_eq!(mapper.read_prg(0xFFFF), 115);

        // Switch banks several times
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0xC000), 115);
    }

    #[test]
    fn test_mapper71_separate_registers() {
        // Verify that bank select and mirroring control are separate registers
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 20) as u8;
            }
        }

        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Set bank to 5
        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0x8000), 25);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Set mirroring to upper
        mapper.write_prg(0xC000, 0x10);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
        // Bank should remain 5
        assert_eq!(mapper.read_prg(0x8000), 25);

        // Change bank to 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 23);
        // Mirroring should still be upper
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
    }

    #[test]
    fn test_mapper71_prg_ram_support() {
        // Mapper 71 should support PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = CamericaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
    }
}
