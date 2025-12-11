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
