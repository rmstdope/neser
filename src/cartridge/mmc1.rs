use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE_4K: usize = 0x1000; // 4KB (for MMC1, MMC3)
const CHR_BANK_SIZE_8K: usize = 0x2000; // 8KB
const MMC1_SHIFT_REGISTER_RESET: u8 = 0x80; // Bit 7 set triggers reset
const MMC1_WRITE_COUNT_MAX: u8 = 5; // Number of writes to load a register
const MMC1_DEFAULT_CONTROL: u8 = 0x0C; // PRG mode 3, CHR mode 0

/// MMC1 mapper (Mapper 1)
///
/// One of the most common NES mappers with sophisticated banking capabilities.
/// Supports:
/// - PRG ROM: Switchable 16KB or 32KB banks
/// - PRG RAM: 8KB at $6000-$7FFF (optional battery-backed)
/// - CHR: Switchable 4KB or 8KB banks (or CHR-RAM if no CHR ROM)
/// - Mirroring: Programmable (horizontal, vertical, one-screen)
/// - Serial shift register: 5-bit values loaded via sequential writes
///
/// Register loading mechanism:
/// - Write to $8000-$FFFF with bit 0 containing the next bit
/// - After 5 writes, the 5-bit value is loaded into the target register
/// - Writing with bit 7 set resets the shift register and sets control to mode 3
///
/// Registers (selected by address):
/// - $8000-$9FFF: Control (mirroring, PRG mode, CHR mode)
/// - $A000-$BFFF: CHR bank 0 (4KB at $0000 or 8KB at $0000)
/// - $C000-$DFFF: CHR bank 1 (4KB at $1000)
/// - $E000-$FFFF: PRG bank (16KB switchable)
///
/// Used in games like The Legend of Zelda, Metroid, Mega Man 2, Final Fantasy.
pub struct MMC1Mapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_memory: Vec<u8>,
    has_chr_ram: bool,

    // Shift register state
    shift_register: u8, // 5-bit shift register
    write_count: u8,    // Number of writes (0-4)

    // Internal registers (5 bits each)
    control: u8,    // Mirroring and banking mode control
    chr_bank_0: u8, // CHR bank 0 select
    chr_bank_1: u8, // CHR bank 1 select
    prg_bank: u8,   // PRG bank select
}

impl MMC1Mapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, _mirroring: MirroringMode) -> Self {
        let has_chr_ram = chr_rom.is_empty();
        let chr_memory = if has_chr_ram {
            vec![0; CHR_RAM_SIZE]
        } else {
            chr_rom
        };

        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_memory,
            has_chr_ram,
            shift_register: 0,
            write_count: 0,
            control: MMC1_DEFAULT_CONTROL, // Default: PRG mode 3 (fix last bank), CHR mode 0
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    fn reset_shift_register(&mut self) {
        self.shift_register = 0;
        self.write_count = 0;
        self.control |= MMC1_DEFAULT_CONTROL; // Set PRG mode to 3 (fix last bank)
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        // Check for reset (bit 7 set)
        if value & MMC1_SHIFT_REGISTER_RESET != 0 {
            self.reset_shift_register();
            return;
        }

        // Shift in bit 0
        self.shift_register >>= 1;
        self.shift_register |= (value & 0x01) << 4;
        self.write_count += 1;

        // After 5 writes, load the register
        if self.write_count == MMC1_WRITE_COUNT_MAX {
            let register_value = self.shift_register;

            // Determine which register to load based on address
            match addr {
                0x8000..=0x9FFF => self.control = register_value & 0x1F,
                0xA000..=0xBFFF => self.chr_bank_0 = register_value & 0x1F,
                0xC000..=0xDFFF => self.chr_bank_1 = register_value & 0x1F,
                0xE000..=0xFFFF => self.prg_bank = register_value & 0x0F,
                _ => {}
            }

            // Reset shift register for next write sequence
            self.shift_register = 0;
            self.write_count = 0;
        }
    }

    fn get_prg_mode(&self) -> u8 {
        (self.control >> 2) & 0x03
    }

    fn get_chr_mode(&self) -> u8 {
        (self.control >> 4) & 0x01
    }

    fn get_mirroring_mode(&self) -> MirroringMode {
        match self.control & 0x03 {
            0 | 1 => MirroringMode::SingleScreen, // 0 and 1 are both single-screen modes
            2 => MirroringMode::Vertical,
            3 => MirroringMode::Horizontal,
            _ => unreachable!(),
        }
    }

    fn get_prg_bank_offset(&self, addr: u16) -> usize {
        let prg_mode = self.get_prg_mode();
        let num_banks = self.prg_rom.len() / PRG_BANK_SIZE;
        let last_bank = num_banks.saturating_sub(1);

        match prg_mode {
            0 | 1 => {
                // 32KB mode: switch entire $8000-$FFFF, ignore low bit of bank number
                let bank = ((self.prg_bank & 0x0E) >> 1) as usize;
                let bank = bank % (num_banks / 2).max(1);
                bank * PRG_BANK_SIZE * 2
            }
            2 => {
                // Fix first bank at $8000, switch 16KB bank at $C000
                if addr < 0xC000 {
                    0 // First bank fixed
                } else {
                    let bank = (self.prg_bank & 0x0F) as usize;
                    let bank = bank % num_banks.max(1);
                    bank * PRG_BANK_SIZE
                }
            }
            3 => {
                // Switch 16KB bank at $8000, fix last bank at $C000
                if addr < 0xC000 {
                    let bank = (self.prg_bank & 0x0F) as usize;
                    let bank = bank % num_banks.max(1);
                    bank * PRG_BANK_SIZE
                } else {
                    last_bank * PRG_BANK_SIZE
                }
            }
            _ => unreachable!(),
        }
    }

    fn get_chr_bank_offset(&self, addr: u16) -> usize {
        let chr_mode = self.get_chr_mode();
        let num_4kb_banks = self.chr_memory.len() / CHR_BANK_SIZE_4K;

        if chr_mode == 0 {
            // 8KB mode: switch entire $0000-$1FFF, ignore low bit
            let bank = ((self.chr_bank_0 & 0x1E) >> 1) as usize;
            let bank = bank % (num_4kb_banks / 2).max(1);
            bank * CHR_BANK_SIZE_8K
        } else {
            // 4KB mode: two separate 4KB banks
            if addr < 0x1000 {
                let bank = (self.chr_bank_0 & 0x1F) as usize;
                let bank = bank % num_4kb_banks.max(1);
                bank * CHR_BANK_SIZE_4K
            } else {
                let bank = (self.chr_bank_1 & 0x1F) as usize;
                let bank = bank % num_4kb_banks.max(1);
                bank * CHR_BANK_SIZE_4K
            }
        }
    }
}

impl Mapper for MMC1Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let bank_offset = self.get_prg_bank_offset(addr);
                let offset = if self.get_prg_mode() <= 1 {
                    // 32KB mode
                    (addr - 0x8000) as usize
                } else {
                    // 16KB mode
                    (addr & 0x3FFF) as usize
                };
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
                self.write_register(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = if self.get_chr_mode() == 0 {
            // 8KB mode
            (addr & 0x1FFF) as usize
        } else {
            // 4KB mode
            (addr & 0x0FFF) as usize
        };
        let index = bank_offset + offset;
        self.chr_memory.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram {
            return; // CHR ROM is read-only
        }

        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = if self.get_chr_mode() == 0 {
            // 8KB mode
            (addr & 0x1FFF) as usize
        } else {
            // 4KB mode
            (addr & 0x0FFF) as usize
        };
        let index = bank_offset + offset;
        if index < self.chr_memory.len() {
            self.chr_memory[index] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // MMC1 doesn't use PPU address changes
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.get_mirroring_mode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::create_mapper;

    #[test]
    fn test_mmc1_shift_register_load() {
        // MMC1 requires 5 sequential writes to load a register
        // Each write shifts bit 0 into the shift register
        // Writing with bit 7 set resets the shift register and control register

        let prg_rom = vec![0; 128 * 1024]; // 128KB = 8 banks of 16KB
        let chr_rom = vec![0; 32 * 1024]; // 32KB = 8 banks of 4KB
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Load value 0b00011 (3) into control register at $8000-$9FFF
        // This requires 5 writes, each with bit 0 containing the next bit of the value
        mapper.write_prg(0x8000, 0b00000001); // bit 0
        mapper.write_prg(0x8000, 0b00000001); // bit 1
        mapper.write_prg(0x8000, 0b00000000); // bit 2
        mapper.write_prg(0x8000, 0b00000000); // bit 3
        mapper.write_prg(0x8000, 0b00000000); // bit 4 (5th write triggers load)

        // After loading 0b00011 into control register:
        // Bits 0-1: Mirroring = 0b11 = Horizontal
        // Bits 2-3: PRG ROM bank mode = 0b00
        // Bit 4: CHR ROM bank mode = 0
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_mmc1_shift_register_reset() {
        // Writing with bit 7 set should reset the shift register
        let prg_rom = vec![0; 256 * 1024];
        let chr_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Start loading a value
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);

        // Reset the shift register (bit 7 set)
        mapper.write_prg(0x8000, 0b10000000);

        // Control register should be reset to default: PRG mode 3 (fix last bank)
        // Start a new load with value 0b00000 (mirroring mode 0 = one screen)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_mmc1_control_register_mirroring() {
        // Control register bits 0-1 control mirroring:
        // 0: one-screen, lower bank
        // 1: one-screen, upper bank
        // 2: vertical
        // 3: horizontal
        let prg_rom = vec![0; 256 * 1024];
        let chr_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Load 0b00000 (mirroring = 0)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);

        // Load 0b00001 (mirroring = 1)
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..4 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);

        // Load 0b00010 (mirroring = 2)
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Load 0b00011 (mirroring = 3)
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_mmc1_prg_bank_mode_0_32kb() {
        // PRG ROM bank mode 0 or 1: switch 32 KB at $8000, ignoring low bit of bank number
        let mut prg_rom = vec![0; 256 * 1024]; // 256KB = 16 banks of 16KB = 8 banks of 32KB

        // Fill each 32KB bank with a unique value
        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Set control register to PRG mode 0 (bits 2-3 = 0b00) and mirroring
        // Value: 0b00000 (mirroring=0, prg_mode=0, chr_mode=0)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }

        // Select 32KB bank 0 via PRG bank register (address $E000-$FFFF)
        // Load value 0b00000 (bank 0)
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0b00000000);
        }
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 10);

        // Select 32KB bank 1 (write 0b00010 = 2, but low bit ignored, so bank 1)
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0xE000, 0b00000000);
        }
        assert_eq!(mapper.read_prg(0x8000), 11);
        assert_eq!(mapper.read_prg(0xC000), 11);
    }

    #[test]
    fn test_mmc1_prg_bank_mode_2_fix_first() {
        // PRG ROM bank mode 2: fix first bank at $8000 and switch 16 KB bank at $C000
        let mut prg_rom = vec![0; 256 * 1024]; // 256KB = 16 banks of 16KB

        // Fill each 16KB bank with a unique value
        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 20) as u8;
            }
        }

        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Set control register to PRG mode 2 (bits 2-3 = 0b10)
        // Value: 0b01000 (mirroring=0, prg_mode=2, chr_mode=0)
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000000);

        // First bank at $8000 should be fixed to bank 0
        assert_eq!(mapper.read_prg(0x8000), 20);

        // Select bank 3 at $C000
        mapper.write_prg(0xE000, 0b00000001);
        mapper.write_prg(0xE000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0xE000, 0b00000000);
        }
        assert_eq!(mapper.read_prg(0x8000), 20); // First bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 23); // Bank 3 at $C000
    }

    #[test]
    fn test_mmc1_prg_bank_mode_3_fix_last() {
        // PRG ROM bank mode 3: fix last bank at $C000 and switch 16 KB bank at $8000
        let mut prg_rom = vec![0; 256 * 1024]; // 256KB = 16 banks of 16KB

        // Fill each 16KB bank with a unique value
        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 30) as u8;
            }
        }

        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Set control register to PRG mode 3 (bits 2-3 = 0b11) - this is the default
        // Value: 0b01100 (mirroring=0, prg_mode=3, chr_mode=0)
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000000);

        // Last bank at $C000 should be fixed to bank 15 (last bank)
        assert_eq!(mapper.read_prg(0xC000), 45); // Bank 15 = 30 + 15

        // Select bank 2 at $8000
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0xE000, 0b00000000);
        }
        assert_eq!(mapper.read_prg(0x8000), 32); // Bank 2 at $8000
        assert_eq!(mapper.read_prg(0xC000), 45); // Last bank still fixed
    }

    #[test]
    fn test_mmc1_chr_bank_mode_0_8kb() {
        // CHR ROM bank mode 0: switch 8 KB at a time
        let mut chr_rom = vec![0; 128 * 1024]; // 128KB = 16 banks of 8KB

        // Fill each 8KB bank with a unique value
        for bank in 0..16 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 40) as u8;
            }
        }

        let prg_rom = vec![0; 32 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Set control register to CHR mode 0 (bit 4 = 0)
        // Value: 0b00000 (mirroring=0, prg_mode=0, chr_mode=0)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }

        // Select 8KB bank 2 via CHR bank 0 register (address $A000-$BFFF)
        // In 8KB mode, only CHR bank 0 matters, and low bit is ignored
        // Load value 0b00100 (4, but low bit ignored = bank 2)
        mapper.write_prg(0xA000, 0b00000000);
        mapper.write_prg(0xA000, 0b00000000);
        mapper.write_prg(0xA000, 0b00000001);
        for _ in 0..2 {
            mapper.write_prg(0xA000, 0b00000000);
        }
        assert_eq!(mapper.read_chr(0x0000), 42); // Bank 2
        assert_eq!(mapper.read_chr(0x1000), 42); // Still bank 2
    }

    #[test]
    fn test_mmc1_chr_bank_mode_1_4kb() {
        // CHR ROM bank mode 1: switch two separate 4 KB banks
        let mut chr_rom = vec![0; 128 * 1024]; // 128KB = 32 banks of 4KB

        // Fill each 4KB bank with a unique value
        for bank in 0..32 {
            let start = bank * 4 * 1024;
            let end = start + 4 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let prg_rom = vec![0; 32 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Set control register to CHR mode 1 (bit 4 = 1)
        // Value: 0b10000 (mirroring=0, prg_mode=0, chr_mode=1)
        mapper.write_prg(0x8000, 0b00000000);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        mapper.write_prg(0x8000, 0b00000001);

        // Select 4KB bank 3 at $0000 via CHR bank 0 register
        mapper.write_prg(0xA000, 0b00000001);
        mapper.write_prg(0xA000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0xA000, 0b00000000);
        }
        assert_eq!(mapper.read_chr(0x0000), 53); // Bank 3 at $0000

        // Select 4KB bank 5 at $1000 via CHR bank 1 register
        mapper.write_prg(0xC000, 0b00000001);
        mapper.write_prg(0xC000, 0b00000000);
        mapper.write_prg(0xC000, 0b00000001);
        for _ in 0..2 {
            mapper.write_prg(0xC000, 0b00000000);
        }
        assert_eq!(mapper.read_chr(0x0000), 53); // Bank 3 still at $0000
        assert_eq!(mapper.read_chr(0x1000), 55); // Bank 5 at $1000
    }

    #[test]
    fn test_mmc1_prg_ram_support() {
        // MMC1 should support 8KB PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mapper(1, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7000, 0xBB);
        mapper.write_prg(0x7FFF, 0xCC);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7000), 0xBB);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCC);
    }

    #[test]
    fn test_mmc1_chr_ram_when_no_chr_rom() {
        // If CHR ROM is empty, MMC1 should use CHR-RAM
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(1, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create MMC1 mapper");

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
}
