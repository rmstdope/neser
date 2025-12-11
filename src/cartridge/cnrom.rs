use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// CNROM mapper (Mapper 3)
///
/// Simple CHR banking mapper with fixed PRG ROM.
/// Supports:
/// - 32KB fixed PRG ROM (no PRG banking)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 8KB switchable CHR ROM window (up to 4 banks = 32KB typical)
/// - CHR bank select via writes to $8000-$FFFF
/// - Fixed horizontal or vertical mirroring
///
/// Used in many early NES games.
pub struct CNROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
    chr_bank_select: u8,
}

impl CNROMMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_rom,
            mirroring,
            chr_bank_select: 0,
        }
    }

    fn get_chr_bank_offset(&self) -> usize {
        let num_banks = (self.chr_rom.len() / CHR_RAM_SIZE).max(1);
        let bank = (self.chr_bank_select as usize) % num_banks;
        bank * CHR_RAM_SIZE
    }
}

impl Mapper for CNROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            // PRG-RAM at $6000-$7FFF (8KB)
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            // PRG ROM is fixed at $8000-$FFFF (32KB or 16KB)
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                let index = offset % self.prg_rom.len();
                self.prg_rom.get(index).copied().unwrap_or(0)
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
            // Any write to $8000-$FFFF sets the CHR bank select
            0x8000..=0xFFFF => {
                self.chr_bank_select = value;
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.get_chr_bank_offset();
        let index = bank_offset + (addr & CHR_MASK) as usize;
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CNROM uses CHR-ROM, writes are ignored
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // CNROM doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cnrom_32kb_prg_no_banking() {
        // CNROM has 32KB PRG ROM with no banking (like NROM)
        let mut prg_rom = vec![0; 32 * 1024];

        // Fill with pattern - each 1KB block gets a unique value
        for (i, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (i / 1024) as u8;
        }

        let mapper = CNROMMapper::new(prg_rom, vec![0; 32 * 1024], MirroringMode::Horizontal);

        // PRG ROM should be accessible at $8000-$FFFF
        assert_eq!(mapper.read_prg(0x8000), 0); // First byte of first 1KB block
        assert_eq!(mapper.read_prg(0x9000), 4); // $9000 = $8000 + $1000 = 4KB offset = block 4
        assert_eq!(mapper.read_prg(0xC000), 16); // $C000 = $8000 + $4000 = 16KB offset = block 16
        assert_eq!(mapper.read_prg(0xFFFF), 31); // $FFFF = last byte of block 31
    }

    #[test]
    fn test_cnrom_chr_bank_switching_4_banks() {
        // 32KB CHR ROM = 4 banks of 8KB
        let mut chr_rom = vec![0; 32 * 1024];

        // Fill each 8KB bank with its bank number
        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 10);
        assert_eq!(mapper.read_chr(0x1FFF), 10);

        // Switch to bank 2
        mapper.write_prg(0x8000, 0b0000_0010);
        assert_eq!(mapper.read_chr(0x0000), 20);
        assert_eq!(mapper.read_chr(0x1FFF), 20);

        // Switch to bank 3
        mapper.write_prg(0x8000, 0b0000_0011);
        assert_eq!(mapper.read_chr(0x0000), 30);
        assert_eq!(mapper.read_chr(0x1FFF), 30);

        // Switch back to bank 0
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn test_cnrom_chr_bank_switching_2_banks() {
        // 16KB CHR ROM = 2 banks of 8KB
        let mut chr_rom = vec![0; 16 * 1024];

        for bank in 0..2 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 50) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Vertical);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 50);

        // Writing higher bits should wrap (only 2 banks available)
        mapper.write_prg(0x8000, 0b0000_0011); // Bank 3 wraps to bank 1
        assert_eq!(mapper.read_chr(0x0000), 50);
    }

    #[test]
    fn test_cnrom_chr_read_only() {
        // CNROM uses CHR-ROM, not CHR-RAM - writes should be ignored
        let chr_rom = vec![0xAA; 32 * 1024];
        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Try to write to CHR
        mapper.write_chr(0x0000, 0x55);

        // Should still read original ROM value
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let mapper_h = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Horizontal,
        );
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Vertical,
        );
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_cnrom_bank_select_any_address() {
        // CNROM responds to writes anywhere in $8000-$FFFF
        let mut chr_rom = vec![0; 32 * 1024];

        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Write to different addresses in PRG space
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x0000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_chr(0x0000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_chr(0x0000), 103);
    }
}
