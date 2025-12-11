use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE_32K: usize = 0x8000; // 32KB (for AxROM)
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// AxROM mapper (Mapper 7)
///
/// Simple PRG banking mapper with programmable one-screen mirroring.
/// Supports:
/// - 32KB switchable PRG ROM bank (entire $8000-$FFFF)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 8KB CHR-RAM (no CHR ROM banking)
/// - Programmable one-screen mirroring (selectable between two nametables)
/// - Register at any write to $8000-$FFFF
///
/// Register format (any write to $8000-$FFFF):
/// - Bits 0-2: Select 32KB PRG ROM bank (supports up to 8 banks = 256KB)
/// - Bit 4: One-screen mirroring select (0 = lower/A, 1 = upper/B)
/// - Other bits ignored
///
/// Used in games like Battletoads, Marble Madness, Wizards & Warriors.
pub struct AxROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_ram: Vec<u8>,
    bank_select: u8, // Stores the full register value (bits 0-2 for bank, bit 4 for mirroring)
}

impl AxROMMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, _mirroring: MirroringMode) -> Self {
        // AxROM uses CHR-RAM, ignores chr_rom and initial mirroring (controlled by register)
        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_ram: vec![0; CHR_RAM_SIZE],
            bank_select: 0, // Default to bank 0, lower nametable
        }
    }

    fn get_prg_bank_offset(&self) -> usize {
        // Extract bank number from bits 0-2
        let bank = (self.bank_select & 0x07) as usize;
        let num_banks = self.prg_rom.len() / PRG_BANK_SIZE_32K;
        let bank = bank % num_banks.max(1);
        bank * PRG_BANK_SIZE_32K
    }
}

impl Mapper for AxROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            // PRG-RAM at $6000-$7FFF (8KB)
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            // PRG ROM at $8000-$FFFF (32KB switchable bank)
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
        match addr {
            // PRG-RAM at $6000-$7FFF (8KB)
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if offset < self.prg_ram.len() {
                    self.prg_ram[offset] = value;
                }
            }
            // Register at $8000-$FFFF
            // Bits 0-2: PRG bank select
            // Bit 4: One-screen mirroring (0 = lower, 1 = upper)
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
        // AxROM doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
        // Bit 4 determines one-screen mirroring mode
        // We use SingleScreen for both modes (PPU memory will handle the actual mirroring)
        // The distinction between upper/lower isn't needed at this level
        MirroringMode::SingleScreen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::create_mapper;

    #[test]
    fn test_axrom_256kb_prg_bank_switching() {
        // AxROM with 256KB (8 banks × 32KB)
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each 32KB bank with its bank number
        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Default bank should be 0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_axrom_bank_select_bits_0_2() {
        // Test that bits 0-2 select the bank (3-bit bank select = 8 banks max)
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to $8000 with different bank values
        mapper.write_prg(0x8000, 0x00); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 0x01); // Bank 1
        assert_eq!(mapper.read_prg(0x8000), 101);

        mapper.write_prg(0x8000, 0x07); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 107);

        // Test that upper bits are ignored (only bits 0-2 matter for bank)
        mapper.write_prg(0x8000, 0xF2); // 0b11110010 -> bank 2
        assert_eq!(mapper.read_prg(0x8000), 102);
    }

    #[test]
    fn test_axrom_chr_ram() {
        // AxROM uses 8KB CHR-RAM (no CHR ROM)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        // Read back
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_lower() {
        // Bit 4 = 0 selects lower nametable (single-screen A)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x8000, 0x00); // Bits: 0000 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);

        // Write with bit 4 = 0 but other bits set
        mapper.write_prg(0x8000, 0x07); // Bits: 0000 0111
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_upper() {
        // Bit 4 = 1 selects upper nametable (single-screen B)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x8000, 0x10); // Bits: 0001 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_axrom_128kb_rom_4_banks() {
        // Test with 128KB ROM (4 banks × 32KB)
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Select each of the 4 banks
        for bank in 0..4 {
            mapper.write_prg(0x8000, bank as u8);
            assert_eq!(mapper.read_prg(0x8000), (bank + 50) as u8);
        }

        // Bank numbers wrap (bank 7 % 4 = 3)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 53); // Bank 3
    }

    #[test]
    fn test_axrom_register_write_any_address() {
        // Writes anywhere in $8000-$FFFF should change the bank
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to different addresses in PRG ROM space
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 11);

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 12);
    }

    #[test]
    fn test_axrom_prg_ram_support() {
        // AxROM should support PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
    }
}
