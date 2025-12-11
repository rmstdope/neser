use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// NROM mapper (Mapper 0)
///
/// The simplest mapper with no bank switching.
/// Supports:
/// - 16KB or 32KB PRG ROM (16KB is mirrored at $C000)
/// - 8KB PRG-RAM at $6000-$7FFF (battery-backed on some cartridges)
/// - 8KB CHR ROM or CHR-RAM
/// - Fixed nametable mirroring
///
/// This is the baseline mapper implementation that all other mappers build upon.
pub struct NROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
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
            prg_ram: vec![0; PRG_RAM_SIZE], // 8KB PRG-RAM initialized to 0
            chr_memory,
            mirroring,
            has_chr_ram,
        }
    }
}

impl Mapper for NROMMapper {
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
            // Writes to PRG ROM are ignored (no mapper registers in NROM)
            0x8000..=0xFFFF => {}
            _ => {}
        }
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
