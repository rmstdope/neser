use crate::cartridge::{Mapper, MirroringMode};

// Memory size constants
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 32 * 1024; // 32KB
const CHR_BANK_SIZE: usize = 8 * 1024; // 8KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// GxROM/GNROM mapper (Mapper 66)
///
/// Simple mapper selecting both PRG and CHR banks with a single write.
///
/// - PRG: 32KB banks mapped at $8000-$FFFF (bits 4-5)
/// - CHR: 8KB banks mapped at $0000-$1FFF (bits 0-1)
/// - Mirroring: fixed from iNES header
pub struct GxROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
    prg_bank_select: u8,
    chr_bank_select: u8,
}

impl GxROMMapper {
    /// Create a new GxROM mapper.
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_rom,
            mirroring,
            prg_bank_select: 0,
            chr_bank_select: 0,
        }
    }

    fn prg_bank_offset(&self) -> usize {
        let num_banks = (self.prg_rom.len() / PRG_BANK_SIZE).max(1);
        let bank = (self.prg_bank_select as usize) % num_banks;
        bank * PRG_BANK_SIZE
    }

    fn chr_bank_offset(&self) -> usize {
        let num_banks = (self.chr_rom.len() / CHR_BANK_SIZE).max(1);
        let bank = (self.chr_bank_select as usize) % num_banks;
        bank * CHR_BANK_SIZE
    }
}

impl Mapper for GxROMMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let bank_offset = self.prg_bank_offset();
                let offset = (addr - 0x8000) as usize;
                let index = bank_offset + offset;
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if offset < self.prg_ram.len() {
                    self.prg_ram[offset] = value;
                }
            }
            0x8000..=0xFFFF => {
                // Register format:
                // - Bits 0-1: CHR bank
                // - Bits 4-5: PRG bank
                self.chr_bank_select = value & 0b0000_0011;
                self.prg_bank_select = (value >> 4) & 0b0000_0011;
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank_offset();
        let index = bank_offset + (addr & CHR_MASK) as usize;
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // GxROM uses CHR-ROM, writes are ignored.
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // No IRQ functionality.
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::mapper::create_mapper;
    use crate::cartridge::MirroringMode;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    #[test]
    fn test_gxrom_prg_and_chr_bank_selected_by_single_write() {
        // Mapper 66 (GxROM/GNROM):
        // - PRG: 32KB banks selected by bits 4-5
        // - CHR: 8KB banks selected by bits 0-1

        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_mapper(66, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("GxROM (mapper 66) should be implemented");

        // Initial banks should be 0.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Select PRG bank 1 (bits 4-5) and CHR bank 2 (bits 0-1): 0b0001_0010 = 0x12
        mapper.write_prg(0x8000, 0x12);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);

        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn test_gxrom_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 2);

        let mut mapper = create_mapper(66, prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("GxROM (mapper 66) should be implemented");

        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Bank select write should not affect mirroring.
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);
    }
}
