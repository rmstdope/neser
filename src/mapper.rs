use crate::cartridge::MirroringMode;
use std::io;

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
            vec![0; 8192] // 8KB CHR-RAM
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
        if self.prg_rom.len() == 0x4000 {
            // 16KB ROM: mirror at $C000
            // $8000-$BFFF maps to ROM, $C000-$FFFF mirrors to same ROM
            let index = offset % 0x4000;
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
        let index = (addr & 0x1FFF) as usize;
        self.chr_memory.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        // Only write to CHR-RAM, CHR-ROM is read-only
        if self.has_chr_ram {
            let index = (addr & 0x1FFF) as usize;
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

/// Create a mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        0 => Ok(Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring))),
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
}
