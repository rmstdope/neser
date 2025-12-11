use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// UxROM mapper (Mapper 2)
///
/// PRG banking mapper with switchable lower bank and fixed upper bank.
/// Supports:
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - 16KB fixed PRG bank at $C000-$FFFF (always last bank)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 8KB CHR-RAM (no CHR ROM banking)
/// - Bank select register at $8000-$FFFF (any write)
///
/// Common in games like Mega Man, Castlevania, Contra, Duck Tales, Metal Gear.
pub struct UxROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_ram: Vec<u8>,
    mirroring: MirroringMode,
    bank_select: u8,
}

impl UxROMMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // UxROM uses CHR-RAM, ignore chr_rom parameter
        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_ram: vec![0; CHR_RAM_SIZE],
            mirroring,
            bank_select: 0,
        }
    }

    fn get_last_bank_offset(&self) -> usize {
        self.prg_rom.len().saturating_sub(PRG_BANK_SIZE)
    }
}

impl Mapper for UxROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            // PRG-RAM at $6000-$7FFF (8KB)
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            // PRG ROM at $8000-$FFFF
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;

                if addr < 0xC000 {
                    // $8000-$BFFF: Switchable 16KB bank
                    let bank_offset = (self.bank_select as usize) * PRG_BANK_SIZE;
                    let index = bank_offset + offset;
                    self.prg_rom.get(index).copied().unwrap_or(0)
                } else {
                    // $C000-$FFFF: Fixed to last 16KB bank
                    let last_bank_offset = self.get_last_bank_offset();
                    let index = last_bank_offset + (offset - PRG_BANK_SIZE);
                    self.prg_rom.get(index).copied().unwrap_or(0)
                }
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            // PRG-RAM at $6000-$7FFF (8KB)
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if offset < self.prg_ram.len() {
                    self.prg_ram[offset] = value;
                }
            }
            // Any write to $8000-$FFFF sets the bank register
            0x8000..=0xFFFF => {
                self.bank_select = value;
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let index = (addr & CHR_MASK) as usize;
        self.chr_ram.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let index = (addr & CHR_MASK) as usize;
        if index < self.chr_ram.len() {
            self.chr_ram[index] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // UxROM doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uxrom_128kb_prg_bank_switching() {
        // Create 128KB (8 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Initially bank 0 should be at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Last bank (7) should always be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_prg(0xFFFF), 7);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 3);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 7);

        // Switch to bank 5
        mapper.write_prg(0xFFFF, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);

        // Last bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_uxrom_256kb_prg_bank_switching() {
        // Create 256KB (16 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Vertical);

        // Last bank (15) should be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 10
        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 0
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn test_uxrom_chr_ram() {
        // UxROM uses 8KB CHR-RAM
        let mut mapper = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_uxrom_mirroring() {
        let mapper_h = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Vertical);
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_uxrom_bank_register_mask() {
        // Test that all 8 bits of the bank register work
        let mut prg_rom = vec![0; 256 * 1024]; // 16 banks

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Test writing different bit patterns
        mapper.write_prg(0x8000, 0b0000_0000); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 0b0000_0111); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 70);

        mapper.write_prg(0x8000, 0b0000_1111); // Bank 15
        assert_eq!(mapper.read_prg(0x8000), 150);
    }

    #[test]
    fn test_uxrom_fixed_last_bank() {
        // Verify that $C000-$FFFF is always the last bank regardless of switches
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

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
}
