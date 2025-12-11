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
