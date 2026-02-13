use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};

// Memory size constants
const PRG_BANK_SIZE: usize = 32 * 1024; // 32KB
const CHR_BANK_SIZE: usize = 8 * 1024; // 8KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

/// Mapper 11 - Color Dreams
///
/// Hardware: Simple unlicensed mapper with combined PRG/CHR banking
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/Color_Dreams>
/// - PRG-ROM: Up to 128KB (4 32KB banks)
/// - PRG-RAM: None
/// - CHR-ROM: Up to 128KB (16 8KB banks)
/// - Mirroring: Fixed horizontal or vertical
///
/// Common boards: Unlicensed Color Dreams boards
///
/// Notes:
/// - Single register at any write to $8000-$FFFF
/// - Bits 0-3: Select 8KB CHR bank
/// - Bits 4-7: Select 32KB PRG bank
/// - Used in unlicensed games like Crystal Mines, Bible Adventures
/// - Some variants support different bank counts
pub struct ColorDreamsMapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
    prg_bank_select: u8,
    chr_bank_select: u8,
}

impl ColorDreamsMapper {
    /// Create a new ColorDreams mapper.
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
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

impl Mapper for ColorDreamsMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
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
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            // Register format:
            // - Bits 0-1: CHR bank
            // - Bits 4-7: PRG bank
            self.chr_bank_select = value & 0b0000_0011;
            self.prg_bank_select = (value >> 4) & 0b0000_1111;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank_offset();
        let index = bank_offset + (addr & CHR_MASK) as usize;
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // ColorDreams uses CHR-ROM, writes are ignored.
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        11
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

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank_select, self.chr_bank_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank_select = data[0];
            self.chr_bank_select = data[1];
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::MirroringMode;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    fn create_colordreams_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(11, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_colordreams_prg_and_chr_bank_selected_by_single_write() {
        // Mapper 11 (ColorDreams):
        // - PRG: 32KB banks selected by bits 4-7
        // - CHR: 8KB banks selected by bits 0-1

        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Initial banks should be 0.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Select PRG bank 1 (bits 4-7) and CHR bank 2 (bits 0-1): 0b0001_0010 = 0x12
        mapper.write_prg(0x8000, 0x12);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);

        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn test_colordreams_prg_uses_4_bits() {
        // ColorDreams uses bits 4-7 for PRG bank selection (16 banks possible)
        // This differs from GxROM which only uses bits 4-5 (4 banks)

        let prg_rom = banked_data(32 * 1024, 16); // 16 banks of 32KB
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Test bank 8 (0b1000_0000 = 0x80)
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.read_prg(0x8000), 8);

        // Test bank 15 (0b1111_0000 = 0xF0)
        mapper.write_prg(0x8000, 0xF0);
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_colordreams_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 2);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Vertical)
            .expect("ColorDreams (mapper 11) should be implemented");

        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Bank select write should not affect mirroring.
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_colordreams_bank_wrapping() {
        // Test that bank selection wraps when ROM is smaller than max banks

        let prg_rom = banked_data(32 * 1024, 4); // Only 4 PRG banks
        let chr_rom = banked_data(8 * 1024, 2); // Only 2 CHR banks

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Select bank 5, should wrap to bank 1 (5 % 4 = 1)
        mapper.write_prg(0x8000, 0x50); // 0b0101_0000
        assert_eq!(mapper.read_prg(0x8000), 1);

        // Select CHR bank 3, should wrap to bank 1 (3 % 2 = 1)
        mapper.write_prg(0x8000, 0x03); // 0b0000_0011
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn test_colordreams_registers_snapshot_restores_banks() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper =
            create_colordreams_mapper(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal)
                .expect("ColorDreams (mapper 11) should be implemented");

        // Select PRG bank 2 and CHR bank 3 (0b0010_0011 = 0x23)
        mapper.write_prg(0x8000, 0x23);

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_colordreams_mapper(prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_chr(0x0000), 3);
    }
}
