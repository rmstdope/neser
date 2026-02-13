use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{BankedRom, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE: usize = 0x2000; // 8KB

/// Mapper 78 - Irem Holy Diver / Jaleco JF-16
///
/// Hardware: Two different board types sharing the same mapper number
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_078>
/// - Irem boards: <https://www.nesdev.org/wiki/INES_Mapper_078#Irem_boards>
/// - Jaleco boards: <https://www.nesdev.org/wiki/INES_Mapper_078#Jaleco_boards>
/// - PRG-ROM: Up to 128KB (8 16KB banks)
/// - PRG-RAM: None
/// - CHR-ROM: Up to 128KB (16 8KB banks)
/// - Mirroring: Programmable (horizontal or vertical via register)
///
/// Common boards: Irem 74HC161/32, Jaleco JF-16
///
/// Register at $8000-$FFFF (any write):
/// - Bits 0-2: Select 16KB PRG bank at $8000-$BFFF
/// - Bit 3: Mirroring (0 = vertical, 1 = horizontal)
/// - Bits 4-7: Select 8KB CHR bank
///
/// Notes:
/// - Last 16KB PRG bank always fixed at $C000-$FFFF
/// - Used in Tengen unlicensed games (due to similar design to NINA-03/06)
/// - Games: Holy Diver (Irem), Uchuusen: Cosmo Carrier (Irem)
/// - Also used by Tengen: Pac-Man, RBI Baseball, Tetris (unlicensed)
pub struct NinaTengenMapper {
    prg_rom: BankedRom,
    prg_ram: PrgRam,
    chr_rom: BankedRom,
    prg_bank_select: u8,
    chr_bank_select: u8,
    mirroring: MirroringMode,
}

impl NinaTengenMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        Self {
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_rom: BankedRom::new(chr_rom, CHR_BANK_SIZE),
            prg_bank_select: 0,
            chr_bank_select: 0,
            mirroring,
        }
    }

    fn get_last_prg_bank(&self) -> usize {
        // Get the last bank number (num_banks - 1)
        let num_banks = self.prg_rom.num_banks();
        if num_banks == 0 { 0 } else { num_banks - 1 }
    }
}

impl Mapper for NinaTengenMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF
        match addr {
            0x8000..=0xBFFF => {
                // Switchable 16KB bank
                self.prg_rom
                    .read_with_base(self.prg_bank_select as usize, 0x8000, addr)
            }
            0xC000..=0xFFFF => {
                // Fixed to last 16KB bank
                let last_bank = self.get_last_prg_bank();
                self.prg_rom.read_with_base(last_bank, 0xC000, addr)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // Any write to $8000-$FFFF sets the bank register
        if (0x8000..=0xFFFF).contains(&addr) {
            // Bits 0-2: PRG bank select
            self.prg_bank_select = value & 0x07;

            // Bit 3: Mirroring (0=vertical, 1=horizontal)
            self.mirroring = if (value & 0x08) != 0 {
                MirroringMode::Horizontal
            } else {
                MirroringMode::Vertical
            };

            // Bits 4-7: CHR bank select
            self.chr_bank_select = (value >> 4) & 0x0F;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_rom
            .read_with_base(self.chr_bank_select as usize, 0x0000, addr)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // Mapper 78 uses CHR-ROM, writes are ignored
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        78
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
        let mirroring = match self.mirroring {
            MirroringMode::Horizontal => 0,
            MirroringMode::Vertical => 1,
            MirroringMode::SingleScreen | MirroringMode::SingleScreenLower => 2,
            MirroringMode::SingleScreenUpper => 3,
            MirroringMode::FourScreen => 4,
        };
        vec![self.prg_bank_select, self.chr_bank_select, mirroring]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank_select = data[0];
            self.chr_bank_select = data[1];
            self.mirroring = match data[2] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreen,
                3 => MirroringMode::SingleScreenUpper,
                4 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nina_tengen_prg_bank_switching() {
        // Create 128KB (8 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(prg_rom, vec![0; 128 * 1024], MirroringMode::Horizontal);

        // Initially bank 0 at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xBFFF), 0);

        // Last bank (7) always at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 70);
        assert_eq!(mapper.read_prg(0xFFFF), 70);

        // Switch to bank 1 (bits 0-2)
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xBFFF), 10);

        // Switch to bank 5 (bits 0-2)
        mapper.write_prg(0x8000, 0b0000_0101);
        assert_eq!(mapper.read_prg(0x8000), 50);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 70);
    }

    #[test]
    fn test_nina_tengen_chr_bank_switching() {
        // Create 128KB (16 banks of 8KB) CHR ROM
        let mut chr_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 15) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(vec![0; 128 * 1024], chr_rom, MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);

        // Switch to bank 1 (bits 4-7)
        mapper.write_prg(0x8000, 0b0001_0000);
        assert_eq!(mapper.read_chr(0x0000), 15);

        // Switch to bank 5 (bits 4-7)
        mapper.write_prg(0x8000, 0b0101_0000);
        assert_eq!(mapper.read_chr(0x0000), 75);

        // Switch to bank 15 (bits 4-7)
        mapper.write_prg(0x8000, 0b1111_0000);
        assert_eq!(mapper.read_chr(0x0000), 225);
    }

    #[test]
    fn test_nina_tengen_mirroring_control() {
        let mut mapper = NinaTengenMapper::new(
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            MirroringMode::Horizontal,
        );

        // Initially horizontal (from constructor)
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Bit 3 = 0: vertical mirroring
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Bit 3 = 1: horizontal mirroring
        mapper.write_prg(0x8000, 0b0000_1000);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Test with other bits set
        mapper.write_prg(0x8000, 0b1111_0111); // Bit 3 = 0
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        mapper.write_prg(0x8000, 0b1111_1111); // Bit 3 = 1
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_nina_tengen_combined_register() {
        // Test that all register bits work together
        let mut prg_rom = vec![0; 128 * 1024];
        let mut chr_rom = vec![0; 128 * 1024];

        // Fill PRG banks
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        // Fill CHR banks
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 200) as u8;
            }
        }

        let mut mapper = NinaTengenMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Write combined register: PRG=3, Mirroring=Vertical, CHR=7
        // Binary: 0111_0011 (CHR=7, Mir=0, PRG=3)
        mapper.write_prg(0x8000, 0b0111_0011);

        assert_eq!(mapper.read_prg(0x8000), 103); // PRG bank 3
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical); // Bit 3 = 0
        assert_eq!(mapper.read_chr(0x0000), 207); // CHR bank 7

        // Write another combined register: PRG=5, Mirroring=Horizontal, CHR=10
        // Binary: 1010_1101 (CHR=10, Mir=1, PRG=5)
        mapper.write_prg(0x8000, 0b1010_1101);

        assert_eq!(mapper.read_prg(0x8000), 105); // PRG bank 5
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal); // Bit 3 = 1
        assert_eq!(mapper.read_chr(0x0000), 210); // CHR bank 10
    }

    #[test]
    fn test_nina_tengen_prg_bank_mask() {
        // Test that only bits 0-2 affect PRG banking
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 25) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(prg_rom, vec![0; 8 * 1024], MirroringMode::Horizontal);

        // Write with upper bits set - should only use lower 3 bits
        mapper.write_prg(0x8000, 0b1111_1111); // PRG bank = 7
        assert_eq!(mapper.read_prg(0x8000), 175); // Bank 7

        mapper.write_prg(0x8000, 0b1111_1000); // PRG bank = 0
        assert_eq!(mapper.read_prg(0x8000), 0); // Bank 0
    }

    #[test]
    fn test_nina_tengen_chr_bank_mask() {
        // Test that only bits 4-7 affect CHR banking
        let mut chr_rom = vec![0; 128 * 1024];
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 12) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Write with lower bits set - should only use bits 4-7
        mapper.write_prg(0x8000, 0b1111_0000); // CHR bank = 15
        assert_eq!(mapper.read_chr(0x0000), 180); // Bank 15

        mapper.write_prg(0x8000, 0b0000_0000); // CHR bank = 0
        assert_eq!(mapper.read_chr(0x0000), 0); // Bank 0
    }

    #[test]
    fn test_nina_tengen_fixed_last_prg_bank() {
        // Verify that $C000-$FFFF is always the last bank
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(prg_rom, vec![0; 8 * 1024], MirroringMode::Horizontal);

        // Last bank should always read 57 (bank 7 + 50)
        assert_eq!(mapper.read_prg(0xC000), 57);

        // Switch banks several times
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0xC000), 57);

        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0xC000), 57);

        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0xC000), 57);
    }

    #[test]
    fn test_nina_tengen_chr_rom_read_only() {
        // CHR ROM should not be writable
        let chr_rom = vec![0xAA; 8 * 1024];
        let mut mapper =
            NinaTengenMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Try to write to CHR
        mapper.write_chr(0x0000, 0x55);

        // Should still read original ROM value
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_nina_tengen_registers_snapshot_restores_banks_and_mirroring() {
        let mut prg_rom = vec![0; 128 * 1024];
        let mut chr_rom = vec![0; 128 * 1024];

        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 20) as u8;
            }
        }

        let mut mapper =
            NinaTengenMapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal);

        // PRG=5, Mirroring=Horizontal, CHR=9
        mapper.write_prg(0x8000, 0b1001_1101);

        let snapshot = mapper.registers_snapshot();

        let mut restored = NinaTengenMapper::new(prg_rom, chr_rom, MirroringMode::Vertical);
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 15);
        assert_eq!(restored.get_mirroring(), MirroringMode::Horizontal);
        assert_eq!(restored.read_chr(0x0000), 29);
    }

    #[test]
    fn test_nina_tengen_banked_rom_replacement() {
        use crate::cartridge::common::BankedRom;
        use crate::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 0x4000; // 16KB
        const CHR_BANK_SIZE: usize = 0x2000; // 8KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 8);
        let chr_rom = banked_data(CHR_BANK_SIZE, 16);

        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom, CHR_BANK_SIZE);

        // Test PRG bank reading
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(7, 0), 7);

        // Test CHR bank reading
        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(15, 0), 15);

        // Test last bank wrapping
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to 0
        assert_eq!(chr_banked.read(16, 0), 0); // wraps to 0
    }
}
