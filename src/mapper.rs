use crate::cartridge::MirroringMode;
use std::io;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// Trait for NES cartridge mappers
///
/// Mappers handle bank switching and address translation for PRG ROM/RAM and CHR ROM/RAM.
/// Different mapper implementations (NROM, MMC1, MMC3, etc.) provide different banking
/// capabilities and features.
///
/// # Example Implementation
///
/// ```ignore
/// struct MyMapper {
///     prg_rom: Vec<u8>,
///     chr_rom: Vec<u8>,
///     // Add banking registers, etc.
/// }
///
/// impl Mapper for MyMapper {
///     fn read_prg(&self, addr: u16) -> u8 {
///         // Translate address through banking logic
///         let bank = self.current_prg_bank();
///         let offset = addr - 0x8000;
///         self.prg_rom[bank * 0x4000 + offset as usize]
///     }
///     // Implement other methods...
/// }
/// ```
pub trait Mapper {
    /// Read a byte from PRG address space (CPU $8000-$FFFF)
    /// Returns the byte at the given address after bank translation
    fn read_prg(&self, addr: u16) -> u8;

    /// Write a byte to PRG address space (CPU $8000-$FFFF)
    /// Used for mapper control registers and PRG-RAM
    fn write_prg(&mut self, addr: u16, value: u8);

    /// Read a byte from CHR address space (PPU $0000-$1FFF)
    /// Returns the byte at the given address after bank translation
    fn read_chr(&self, addr: u16) -> u8;

    /// Write a byte to CHR address space (PPU $0000-$1FFF)
    /// Only works for CHR-RAM, CHR-ROM is read-only
    fn write_chr(&mut self, addr: u16, value: u8);

    /// Notify mapper of PPU address bus changes
    /// Used for detecting A12 rising edges (for MMC3 IRQ)
    fn ppu_address_changed(&mut self, addr: u16);

    /// Get the current nametable mirroring mode
    /// Some mappers can change mirroring dynamically
    fn get_mirroring(&self) -> MirroringMode;
}

/// NROM mapper (Mapper 0)
///
/// The simplest mapper with no bank switching.
/// Supports:
/// - 16KB or 32KB PRG ROM (16KB is mirrored at $C000)
/// - 8KB CHR ROM or CHR-RAM
/// - Fixed nametable mirroring
///
/// This is the baseline mapper implementation that all other mappers build upon.
pub struct NROMMapper {
    prg_rom: Vec<u8>,
    chr_memory: Vec<u8>,
    mirroring: MirroringMode,
    has_chr_ram: bool,
}

impl NROMMapper {
    /// Create a new NROM mapper
    /// If chr_rom is empty, 8KB of CHR-RAM is allocated
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let has_chr_ram = chr_rom.is_empty();
        let chr_memory = if has_chr_ram {
            vec![0; CHR_RAM_SIZE]
        } else {
            chr_rom
        };

        Self {
            prg_rom,
            chr_memory,
            mirroring,
            has_chr_ram,
        }
    }
}

impl Mapper for NROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG ROM is mapped to $8000-$FFFF
        let offset = (addr - 0x8000) as usize;

        // Handle 16KB vs 32KB PRG ROM
        if self.prg_rom.len() == PRG_BANK_SIZE {
            // 16KB ROM: mirror at $C000
            // $8000-$BFFF maps to ROM, $C000-$FFFF mirrors to same ROM
            let index = offset % PRG_BANK_SIZE;
            self.prg_rom.get(index).copied().unwrap_or(0)
        } else {
            // 32KB or larger ROM: direct mapping
            let index = offset % self.prg_rom.len();
            self.prg_rom.get(index).copied().unwrap_or(0)
        }
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // NROM has no PRG-RAM or mapper registers, writes are ignored
    }

    fn read_chr(&self, addr: u16) -> u8 {
        // CHR memory is mapped to $0000-$1FFF (8KB)
        let index = (addr & CHR_MASK) as usize;
        self.chr_memory.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        // Only write to CHR-RAM, CHR-ROM is read-only
        if self.has_chr_ram {
            let index = (addr & CHR_MASK) as usize;
            if index < self.chr_memory.len() {
                self.chr_memory[index] = value;
            }
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // NROM doesn't care about PPU address changes
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

/// UxROM mapper (Mapper 2)
///
/// PRG banking mapper with switchable lower bank and fixed upper bank.
/// Supports:
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - 16KB fixed PRG bank at $C000-$FFFF (always last bank)
/// - 8KB CHR-RAM (no CHR ROM banking)
/// - Bank select register at $8000-$FFFF (any write)
///
/// Common in games like Mega Man, Castlevania, Contra, Duck Tales, Metal Gear.
pub struct UxROMMapper {
    prg_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    mirroring: MirroringMode,
    bank_select: u8,
}

impl UxROMMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // UxROM uses CHR-RAM, ignore chr_rom parameter
        Self {
            prg_rom,
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

    fn write_prg(&mut self, _addr: u16, value: u8) {
        // Any write to $8000-$FFFF sets the bank register
        self.bank_select = value;
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

/// Create a mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        0 => Ok(Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring))),
        2 => Ok(Box::new(UxROMMapper::new(prg_rom, chr_rom, mirroring))),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Mapper {} not implemented", mapper_number),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nrom_32kb_prg_rom_read() {
        // Create a 32KB PRG ROM
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0x0000] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xBB; // First byte at $C000
        prg_rom[0x7FFF] = 0xCC; // Last byte at $FFFF

        let mapper = NROMMapper::new(prg_rom, vec![0; 8192], MirroringMode::Horizontal);

        // Test reading from different PRG addresses
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xC000), 0xBB);
        assert_eq!(mapper.read_prg(0xFFFF), 0xCC);
    }

    #[test]
    fn test_nrom_16kb_prg_rom_mirroring() {
        // Create a 16KB PRG ROM
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0x0000] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte

        let mapper = NROMMapper::new(prg_rom, vec![0; 8192], MirroringMode::Horizontal);

        // Test reading from $8000-$BFFF (first 16KB)
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xBFFF), 0xBB);

        // Test reading from $C000-$FFFF (mirrored second 16KB)
        assert_eq!(mapper.read_prg(0xC000), 0xAA); // Should mirror to $8000
        assert_eq!(mapper.read_prg(0xFFFF), 0xBB); // Should mirror to $BFFF
    }

    #[test]
    fn test_nrom_chr_rom_read() {
        // Create 8KB CHR ROM
        let mut chr_rom = vec![0; 8192];
        chr_rom[0x0000] = 0x11;
        chr_rom[0x0FFF] = 0x22;
        chr_rom[0x1000] = 0x33;
        chr_rom[0x1FFF] = 0x44;

        let mapper = NROMMapper::new(vec![0; 0x8000], chr_rom, MirroringMode::Horizontal);

        // Test reading from CHR ROM
        assert_eq!(mapper.read_chr(0x0000), 0x11);
        assert_eq!(mapper.read_chr(0x0FFF), 0x22);
        assert_eq!(mapper.read_chr(0x1000), 0x33);
        assert_eq!(mapper.read_chr(0x1FFF), 0x44);
    }

    #[test]
    fn test_nrom_chr_ram_write_and_read() {
        // Create mapper with CHR-RAM (empty CHR ROM)
        let mut mapper = NROMMapper::new(vec![0; 0x8000], vec![], MirroringMode::Horizontal);

        // Initially should read 0
        assert_eq!(mapper.read_chr(0x0000), 0x00);

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        // Read back the values
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_nrom_chr_rom_write_ignored() {
        // Create mapper with CHR ROM (not RAM)
        let chr_rom = vec![0x55; 8192];
        let mut mapper = NROMMapper::new(vec![0; 0x8000], chr_rom, MirroringMode::Horizontal);

        // Try to write to CHR ROM (should be ignored)
        mapper.write_chr(0x0000, 0xAA);

        // Should still read original value
        assert_eq!(mapper.read_chr(0x0000), 0x55);
    }

    #[test]
    fn test_nrom_prg_write_ignored() {
        // NROM has no PRG-RAM or mapper registers
        let prg_rom = vec![0xAA; 0x8000];
        let mut mapper = NROMMapper::new(prg_rom, vec![0; 8192], MirroringMode::Horizontal);

        // Try to write to PRG space (should be ignored)
        mapper.write_prg(0x8000, 0xBB);

        // Should still read original value
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
    }

    #[test]
    fn test_nrom_mirroring_modes() {
        let mapper_h = NROMMapper::new(vec![0; 0x8000], vec![0; 8192], MirroringMode::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = NROMMapper::new(vec![0; 0x8000], vec![0; 8192], MirroringMode::Vertical);
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);

        let mapper_4 = NROMMapper::new(vec![0; 0x8000], vec![0; 8192], MirroringMode::FourScreen);
        assert_eq!(mapper_4.get_mirroring(), MirroringMode::FourScreen);
    }

    #[test]
    fn test_nrom_ppu_address_changed_noop() {
        // NROM doesn't care about PPU address changes (no IRQ, no banking)
        let mut mapper = NROMMapper::new(vec![0; 0x8000], vec![0; 8192], MirroringMode::Horizontal);

        // Should not panic or change behavior
        mapper.ppu_address_changed(0x0000);
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_address_changed(0x1FFF);
    }

    // UxROM (Mapper 2) Tests

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
