use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE: usize = 0x8000; // 32KB
const CHR_BANK_SIZE: usize = 0x2000; // 8KB

/// Mapper 34 (BNROM/NINA-001)
///
/// Two different hardware types:
/// - BNROM: Simple PRG switching only, CHR-RAM
/// - NINA-001: PRG + CHR switching
///
/// Detection: Games with CHR ROM use NINA-001, games with CHR-RAM use BNROM
///
/// BNROM:
/// - 32KB switchable PRG banks (up to 128KB total = 4 banks)
/// - 8KB CHR-RAM
/// - Bank select at $8000-$FFFF (any write)
/// - Used by: Deadly Towers, various others
///
/// NINA-001:
/// - 32KB switchable PRG banks (up to 128KB total = 4 banks)
/// - 8KB switchable CHR banks (up to 64KB total = 8 banks)
/// - PRG bank select at $7FFD and $7FFF
/// - CHR bank select at $7FFE
pub struct BnromNinaMapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    mirroring: MirroringMode,
    prg_bank_select: u8,
    chr_bank_select: u8,
    is_nina: bool, // true for NINA-001, false for BNROM
}

impl BnromNinaMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // Detect variant: NINA-001 has CHR ROM, BNROM uses CHR-RAM
        let is_nina = !chr_rom.is_empty();

        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            prg_bank_select: 0,
            chr_bank_select: 0,
            is_nina,
        }
    }

    fn get_prg_bank_offset(&self) -> usize {
        let num_banks = (self.prg_rom.len() / PRG_BANK_SIZE).max(1);
        let bank = (self.prg_bank_select as usize) % num_banks;
        bank * PRG_BANK_SIZE
    }

    fn get_chr_bank_offset(&self) -> usize {
        let chr_size = if self.chr_memory.is_ram() {
            8192 // CHR-RAM size
        } else {
            self.chr_memory.size()
        };
        let num_banks = (chr_size / CHR_BANK_SIZE).max(1);
        let bank = (self.chr_bank_select as usize) % num_banks;
        bank * CHR_BANK_SIZE
    }
}

impl Mapper for BnromNinaMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF (32KB switchable bank)
        match addr {
            0x8000..=0xFFFF => {
                let bank_offset = self.get_prg_bank_offset();
                let offset = (addr - 0x8000) as usize;
                let index = bank_offset + offset;
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (but check for NINA-001 registers first)
        if self.is_nina && (0x7FFD..=0x7FFF).contains(&addr) {
            // NINA-001 registers at $7FFD-$7FFF
            match addr {
                0x7FFD | 0x7FFF => {
                    // PRG bank select
                    self.prg_bank_select = value;
                }
                0x7FFE => {
                    // CHR bank select
                    self.chr_bank_select = value;
                }
                _ => {}
            }
            return;
        }

        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // BNROM: Any write to $8000-$FFFF sets PRG bank
        // NINA-001: Writes to $8000-$FFFF are ignored (uses $7FFD-$7FFF instead)
        if !self.is_nina && (0x8000..=0xFFFF).contains(&addr) {
            self.prg_bank_select = value;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        if self.is_nina {
            // NINA-001: banked CHR
            let bank_offset = self.get_chr_bank_offset();
            let offset = (addr & 0x1FFF) as usize;
            let index = bank_offset + offset;
            self.chr_memory.read_at_index(index)
        } else {
            // BNROM: simple CHR-RAM
            self.chr_memory.read(addr)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.is_nina {
            // NINA-001: banked CHR
            let bank_offset = self.get_chr_bank_offset();
            let offset = (addr & 0x1FFF) as usize;
            let index = bank_offset + offset;
            self.chr_memory.write_at_index(index, value);
        } else {
            // BNROM: simple CHR-RAM
            self.chr_memory.write(addr, value);
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // No IRQ support
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

    // BNROM tests (CHR-RAM)
    #[test]
    fn test_bnrom_prg_bank_switching() {
        // Create 128KB (4 banks of 32KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        // Empty CHR ROM = BNROM variant
        let mut mapper = BnromNinaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xFFFF), 10);

        // Switch to bank 2
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 20);
        assert_eq!(mapper.read_prg(0xFFFF), 20);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 30);
        assert_eq!(mapper.read_prg(0xFFFF), 30);
    }

    #[test]
    fn test_bnrom_chr_ram() {
        // BNROM uses CHR-RAM
        let mut mapper =
            BnromNinaMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_bnrom_bank_select_anywhere() {
        // BNROM responds to any write in $8000-$FFFF
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_prg(0x8000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_prg(0x8000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_prg(0x8000), 103);
    }

    // NINA-001 tests (CHR ROM)
    #[test]
    fn test_nina001_prg_bank_switching() {
        // Create 128KB PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        // Non-empty CHR ROM = NINA-001 variant
        let mut mapper =
            BnromNinaMapper::new(prg_rom, vec![0; 64 * 1024], MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        // NINA-001 uses $7FFD/$7FFF for PRG bank select
        mapper.write_prg(0x7FFD, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0x7FFF, 2);
        assert_eq!(mapper.read_prg(0x8000), 20);

        mapper.write_prg(0x7FFD, 3);
        assert_eq!(mapper.read_prg(0x8000), 30);
    }

    #[test]
    fn test_nina001_chr_bank_switching() {
        // Create 64KB CHR ROM (8 banks of 8KB)
        let mut chr_rom = vec![0; 64 * 1024];
        for bank in 0..8 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 20) as u8;
            }
        }

        let mut mapper =
            BnromNinaMapper::new(vec![0; 128 * 1024], chr_rom, MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Switch to bank 1
        mapper.write_prg(0x7FFE, 1);
        assert_eq!(mapper.read_chr(0x0000), 20);

        // Switch to bank 2
        mapper.write_prg(0x7FFE, 2);
        assert_eq!(mapper.read_chr(0x0000), 40);

        // Switch to bank 7
        mapper.write_prg(0x7FFE, 7);
        assert_eq!(mapper.read_chr(0x0000), 140);
    }

    #[test]
    fn test_nina001_ignores_8000_writes() {
        // NINA-001 should ignore writes to $8000-$FFFF (not a bank select region)
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper =
            BnromNinaMapper::new(prg_rom, vec![0; 8 * 1024], MirroringMode::Horizontal);

        // Set bank via proper register
        mapper.write_prg(0x7FFD, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);

        // Write to $8000 should not change bank
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 10); // Still bank 1

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_prg(0x8000), 10); // Still bank 1
    }

    #[test]
    fn test_bnrom_detection() {
        // Empty CHR ROM = BNROM
        let mapper_bnrom =
            BnromNinaMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);
        assert!(!mapper_bnrom.is_nina);

        // Non-empty CHR ROM = NINA-001
        let mapper_nina = BnromNinaMapper::new(
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            MirroringMode::Horizontal,
        );
        assert!(mapper_nina.is_nina);
    }

    #[test]
    fn test_bnrom_mirroring() {
        let mapper_h = BnromNinaMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = BnromNinaMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Vertical);
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_nina001_mirroring() {
        let mapper_h = BnromNinaMapper::new(
            vec![0; 128 * 1024],
            vec![0; 8 * 1024],
            MirroringMode::Horizontal,
        );
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = BnromNinaMapper::new(
            vec![0; 128 * 1024],
            vec![0; 8 * 1024],
            MirroringMode::Vertical,
        );
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }
}
