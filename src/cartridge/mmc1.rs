use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;
use crate::trace_mapper;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE_4K: usize = 0x1000; // 4KB (for MMC1, MMC3)
const CHR_BANK_SIZE_8K: usize = 0x2000; // 8KB
const MMC1_SHIFT_REGISTER_RESET: u8 = 0x80; // Bit 7 set triggers reset
const MMC1_WRITE_COUNT_MAX: u8 = 5; // Number of writes to load a register
const MMC1_DEFAULT_CONTROL: u8 = 0x0C; // PRG mode 3, CHR mode 0

/// MMC1 ASIC revision variants
///
/// Different MMC1 hardware revisions have different PRG-RAM enable behavior:
/// - MMC1A: PRG-RAM always enabled (bit 4 of PRG bank register ignored)
/// - MMC1B: PRG-RAM can be disabled via bit 4 (starts enabled by default)
///
/// See: https://www.nesdev.org/wiki/MMC1#ASIC_Revisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mmc1Revision {
    /// MMC1A: PRG-RAM always enabled, bit 4 ignored
    #[cfg(test)]
    Mmc1A,
    /// MMC1B/C: PRG-RAM enable controlled by bit 4 of PRG bank register
    Mmc1B,
}

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
/// - $E000-$FFFF: PRG bank (16KB switchable), bit 4 controls PRG-RAM enable
///
/// PRG-RAM Enable (Revision-Specific):
/// - MMC1A: PRG-RAM is always enabled, bit 4 of PRG bank register is ignored
/// - MMC1B/C: Bit 4 controls PRG-RAM (0 = enabled, 1 = disabled)
///
/// See: https://www.nesdev.org/wiki/MMC1#ASIC_Revisions
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

    // Hardware revision
    revision: Mmc1Revision, // MMC1A vs MMC1B behavior

    // Cycle tracking for consecutive-write ignore behavior
    cpu_cycle_count: u64,  // Current CPU cycle count
    last_write_cycle: u64, // CPU cycle of last write to shift register
}

impl MMC1Mapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, _mirroring: MirroringMode) -> Self {
        // Default to MMC1B for backward compatibility and broader game support
        Self::new_with_revision(prg_rom, chr_rom, _mirroring, Mmc1Revision::Mmc1B)
    }

    pub fn new_with_revision(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        _mirroring: MirroringMode,
        revision: Mmc1Revision,
    ) -> Self {
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
            shift_register: 0x10, // Power-on state: bit 4 set
            write_count: 0,
            control: MMC1_DEFAULT_CONTROL, // Default: PRG mode 3 (fix last bank), CHR mode 0
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            revision,
            cpu_cycle_count: 0,
            last_write_cycle: 0,
        }
    }

    fn reset_shift_register(&mut self) {
        self.shift_register = 0x10; // Reset to power-on state: bit 4 set
        self.write_count = 0;
        self.control |= MMC1_DEFAULT_CONTROL; // Set PRG mode to 3 (fix last bank)
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        // Check for reset (bit 7 set) - reset writes are NEVER ignored
        if value & MMC1_SHIFT_REGISTER_RESET != 0 {
            self.reset_shift_register();
            self.last_write_cycle = self.cpu_cycle_count;
            return;
        }

        // MMC1 ignores consecutive-cycle writes (except reset writes above)
        // This prevents RMW instructions from shifting two bits
        // Only apply this filtering if cpu_cycle() has been called (cpu_cycle_count > 0)
        // This allows tests without cpu_cycle() calls to work as before
        if self.cpu_cycle_count > 0 && self.cpu_cycle_count == self.last_write_cycle {
            // Consecutive write detected - ignore it
            return;
        }

        // Update last write cycle
        self.last_write_cycle = self.cpu_cycle_count;

        // Shift in bit 0
        self.shift_register >>= 1;
        self.shift_register |= (value & 0x01) << 4;
        self.write_count += 1;

        // After 5 writes, load the register
        if self.write_count == MMC1_WRITE_COUNT_MAX {
            let register_value = self.shift_register;

            // Determine which register to load based on address
            match addr {
                0x8000..=0x9FFF => {
                    trace_mapper!(1; "MMC1 control=${:02X} (mirroring={}, PRG_mode={}, CHR_mode={})",
                        register_value & 0x1F,
                        register_value & 0x03,
                        (register_value >> 2) & 0x03,
                        (register_value >> 4) & 0x01
                    );
                    self.control = register_value & 0x1F;
                }
                0xA000..=0xBFFF => {
                    trace_mapper!(1; "MMC1 CHR_bank_0=${:02X}", register_value & 0x1F);
                    self.chr_bank_0 = register_value & 0x1F;
                }
                0xC000..=0xDFFF => {
                    trace_mapper!(1; "MMC1 CHR_bank_1=${:02X}", register_value & 0x1F);
                    self.chr_bank_1 = register_value & 0x1F;
                }
                0xE000..=0xFFFF => {
                    trace_mapper!(1; "MMC1 PRG_bank=${:02X}", register_value & 0x1F);
                    self.prg_bank = register_value & 0x1F;
                }
                _ => {}
            }

            // Reset shift register for next write sequence
            self.shift_register = 0x10; // Reset to power-on state
            self.write_count = 0;
        }
    }

    fn get_prg_mode(&self) -> u8 {
        (self.control >> 2) & 0x03
    }

    fn get_chr_mode(&self) -> u8 {
        (self.control >> 4) & 0x01
    }

    fn is_wram_enabled(&self) -> bool {
        match self.revision {
            #[cfg(test)]
            Mmc1Revision::Mmc1A => {
                // MMC1A always has PRG-RAM enabled, bit 4 is ignored
                true
            }
            Mmc1Revision::Mmc1B => {
                // MMC1B/C: Bit 4 of prg_bank register controls WRAM
                // 0 = enabled, 1 = disabled
                (self.prg_bank & 0x10) == 0
            }
        }
    }

    fn get_mirroring_mode(&self) -> MirroringMode {
        match self.control & 0x03 {
            0 => MirroringMode::SingleScreenLower, // One-screen, lower bank
            1 => MirroringMode::SingleScreenUpper, // One-screen, upper bank
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
                // Check if WRAM is enabled
                if !self.is_wram_enabled() {
                    return 0; // Return 0 when WRAM is disabled (open bus behavior)
                }
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

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if !self.is_wram_enabled() {
                    return open_bus;
                }
                self.read_prg(addr)
            }
            _ => {
                if addr < 0x6000 {
                    open_bus
                } else {
                    self.read_prg(addr)
                }
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // Only allow writes if WRAM is enabled
                if !self.is_wram_enabled() {
                    return; // Ignore writes when WRAM is disabled
                }
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

    fn mapper_number(&self) -> u8 {
        1
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        if self.has_chr_ram {
            self.chr_memory.clone()
        } else {
            Vec::new()
        }
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        if self.has_chr_ram && !data.is_empty() {
            let to_copy = data.len().min(self.chr_memory.len());
            self.chr_memory[..to_copy].copy_from_slice(&data[..to_copy]);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize MMC1 internal registers:
        // [0]: shift_register
        // [1]: write_count
        // [2]: control
        // [3]: chr_bank_0
        // [4]: chr_bank_1
        // [5]: prg_bank
        // [6..=13]: cpu_cycle_count (u64 LE)
        // [14..=21]: last_write_cycle (u64 LE)
        vec![
            self.shift_register,
            self.write_count,
            self.control,
            self.chr_bank_0,
            self.chr_bank_1,
            self.prg_bank,
            (self.cpu_cycle_count & 0xFF) as u8,
            ((self.cpu_cycle_count >> 8) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 16) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 24) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 32) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 40) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 48) & 0xFF) as u8,
            ((self.cpu_cycle_count >> 56) & 0xFF) as u8,
            (self.last_write_cycle & 0xFF) as u8,
            ((self.last_write_cycle >> 8) & 0xFF) as u8,
            ((self.last_write_cycle >> 16) & 0xFF) as u8,
            ((self.last_write_cycle >> 24) & 0xFF) as u8,
            ((self.last_write_cycle >> 32) & 0xFF) as u8,
            ((self.last_write_cycle >> 40) & 0xFF) as u8,
            ((self.last_write_cycle >> 48) & 0xFF) as u8,
            ((self.last_write_cycle >> 56) & 0xFF) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.shift_register = data[0];
            self.write_count = data[1];
            self.control = data[2];
            self.chr_bank_0 = data[3];
            self.chr_bank_1 = data[4];
            self.prg_bank = data[5];
        }

        if data.len() >= 22 {
            self.cpu_cycle_count = u64::from_le_bytes([
                data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
            ]);
            self.last_write_cycle = u64::from_le_bytes([
                data[14], data[15], data[16], data[17], data[18], data[19], data[20], data[21],
            ]);
        }
    }

    fn cpu_cycle(&mut self) {
        // Increment CPU cycle counter for consecutive-write detection
        self.cpu_cycle_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    /// Helper function to write a 5-bit value to a register using the MMC1 shift mechanism
    fn write_register(mapper: &mut MMC1Mapper, addr: u16, value: u8) {
        for i in 0..5 {
            mapper.write_prg(addr, (value >> i) & 0x01);
        }
    }

    fn create_mmc1_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new(1, prg_rom, chr_rom, mirroring))
            .expect("MMC1 (mapper 1) should be implemented")
    }

    #[test]
    fn test_mmc1_shift_register_load() {
        // MMC1 requires 5 sequential writes to load a register
        // Each write shifts bit 0 into the shift register
        // Writing with bit 7 set resets the shift register and control register

        let prg_rom = vec![0; 128 * 1024]; // 128KB = 8 banks of 16KB
        let chr_rom = vec![0; 32 * 1024]; // 32KB = 8 banks of 4KB
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
    fn test_mmc1_registers_snapshot_preserves_write_ignore_timing() {
        let prg_rom = vec![0; PRG_BANK_SIZE * 2];
        let chr_rom = vec![];

        let mut mapper =
            MMC1Mapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal);

        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01);

        let saved = mapper.registers_snapshot();

        let mut restored = MMC1Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
        restored.restore_registers(&saved);

        let before = restored.registers_snapshot();
        restored.write_prg(0x8000, 0x00);
        let after = restored.registers_snapshot();

        assert_eq!(
            after, before,
            "consecutive write should be ignored when cpu_cycle_count matches last_write_cycle"
        );
    }

    #[test]
    fn test_mmc1_shift_register_reset() {
        // Writing with bit 7 set should reset the shift register
        let prg_rom = vec![0; 256 * 1024];
        let chr_rom = vec![0; 128 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Start loading a value
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);

        // Reset the shift register (bit 7 set)
        mapper.write_prg(0x8000, 0b10000000);

        // Control register should be reset to default: PRG mode 3 (fix last bank)
        // Start a new load with value 0b00000 (mirroring mode 0 = one screen lower)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);
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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Load 0b00000 (mirroring = 0 = SingleScreenLower)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        // Load 0b00001 (mirroring = 1 = SingleScreenUpper)
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..4 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);

        // Load 0b00010 (mirroring = 2 = Vertical)
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Load 0b00011 (mirroring = 3 = Horizontal)
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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, vec![], MirroringMode::Horizontal);

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
    fn test_mmc1_shift_register_power_on_state() {
        // MMC1 hardware shift register should start at 0x10 (bit 4 set)
        // This means the first write will shift bit 4 right and OR in bit 0
        // After 5 writes, the shift register should contain the 5-bit value
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Write sequence: 1, 0, 1, 0, 1 should result in value 0b10101
        // With proper power-on state (0x10), after 5 writes we should see the bit pattern
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1
        mapper.write_prg(0x8000, 0b00000000); // Write bit 0 = 0
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1
        mapper.write_prg(0x8000, 0b00000000); // Write bit 0 = 0
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1 (5th write, should load)

        // The control register should now contain 0b10101 (mirroring = 0b01 = SingleScreenUpper)
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
    }

    #[test]
    fn test_mmc1_shift_register_reset_clears_to_power_on_state() {
        // After reset, the shift register should go back to 0x10
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Do some writes
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);

        // Reset with bit 7 set
        mapper.write_prg(0x8000, 0b10000000);

        // Now write a sequence and verify it works correctly
        // Write 5 ones: should result in 0b11111 after loading
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000001);
        }

        // Control register should be 0b11111 (mirroring = 0b11 = Horizontal)
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_mmc1_wram_enable_disable() {
        // PRG bank register bit 4 controls WRAM enable/disable
        // When bit 4 is set (1), WRAM is disabled
        // When bit 4 is clear (0), WRAM is enabled
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Initially WRAM should be enabled (prg_bank defaults to 0)
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable WRAM by setting bit 4 of prg_bank register ($E000-$FFFF)
        // To load 0b10000 into the shift register, write the bit sequence: 0,0,0,0,1
        // The shift register starts at 0x10, shifts right, and ORs each bit at position 4
        // After 5 writes, this produces: 0b10000 (bit 4 set = WRAM disabled)
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000001);

        // With WRAM disabled, reads should return 0 (open bus behavior)
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Writes should be ignored
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0x00); // Still reads 0, not 0xBB

        // Re-enable WRAM by clearing bit 4
        // Write 0b00000 to prg_bank register
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0b00000000);
        }

        // With WRAM enabled again, writes should work
        mapper.write_prg(0x6000, 0xCC);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);

        // Previous write while disabled should not have affected memory
        // (we wrote 0xBB at 0x6000 while disabled, but it was ignored)
        mapper.write_prg(0x6001, 0xDD);
        assert_eq!(mapper.read_prg(0x6001), 0xDD);
    }

    #[test]
    fn test_mmc1_wram_disable_multiple_addresses() {
        // Verify WRAM disable affects the entire WRAM range ($6000-$7FFF)
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Write to various WRAM addresses while enabled
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x7000, 0x22);
        mapper.write_prg(0x7FFF, 0x33);

        // Verify writes worked
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0x7000), 0x22);
        assert_eq!(mapper.read_prg(0x7FFF), 0x33);

        // Disable WRAM (set bit 4 of prg_bank)
        // Write sequence: 0,0,0,0,1 to load 0b10000
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000000);
        mapper.write_prg(0xE000, 0b00000001); // Loads 0b10000 (bit 4 set)

        // All WRAM reads should return 0
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x7000), 0x00);
        assert_eq!(mapper.read_prg(0x7FFF), 0x00);

        // All WRAM writes should be ignored
        mapper.write_prg(0x6000, 0x44);
        mapper.write_prg(0x7000, 0x55);
        mapper.write_prg(0x7FFF, 0x66);
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x7000), 0x00);
        assert_eq!(mapper.read_prg(0x7FFF), 0x00);
    }

    #[test]
    fn test_mmc1a_wram_always_enabled() {
        // MMC1A revision: PRG-RAM is always enabled, bit 4 is ignored
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new_with_revision(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            Mmc1Revision::Mmc1A,
        );

        // Write to WRAM while enabled
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7000, 0xBB);
        mapper.write_prg(0x7FFF, 0xCC);

        // Verify writes worked
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7000), 0xBB);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCC);

        // Try to disable WRAM by setting bit 4 of prg_bank register ($E000-$FFFF)
        // Load 0b10000 (bit 4 set)
        write_register(&mut mapper, 0xE000, 0b10000);

        // On MMC1A, WRAM should still be enabled (bit 4 is ignored)
        // Reads should return the previously written values
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7000), 0xBB);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCC);

        // Writes should still work
        mapper.write_prg(0x6000, 0xDD);
        mapper.write_prg(0x7000, 0xEE);
        mapper.write_prg(0x7FFF, 0xFF);

        assert_eq!(mapper.read_prg(0x6000), 0xDD);
        assert_eq!(mapper.read_prg(0x7000), 0xEE);
        assert_eq!(mapper.read_prg(0x7FFF), 0xFF);
    }

    #[test]
    fn test_mmc1b_wram_enable_disable() {
        // MMC1B revision: PRG-RAM can be disabled via bit 4
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new_with_revision(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            Mmc1Revision::Mmc1B,
        );

        // Initially WRAM should be enabled (prg_bank defaults to 0)
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable WRAM by setting bit 4 of prg_bank register
        write_register(&mut mapper, 0xE000, 0b10000);

        // With WRAM disabled, reads should return 0
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Writes should be ignored
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0x00); // Still reads 0, not 0xBB

        // Re-enable WRAM by clearing bit 4
        write_register(&mut mapper, 0xE000, 0b00000);

        // With WRAM enabled again, writes should work
        mapper.write_prg(0x6000, 0xCC);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);

        // Previous write while disabled should not have affected memory
        mapper.write_prg(0x6001, 0xDD);
        assert_eq!(mapper.read_prg(0x6001), 0xDD);
    }

    #[test]
    fn test_mmc1_default_revision_is_mmc1b() {
        // Default constructor should use MMC1B for backward compatibility
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Write to WRAM
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable WRAM (should work on MMC1B)
        write_register(&mut mapper, 0xE000, 0b10000);

        // Should be disabled (reads 0)
        assert_eq!(mapper.read_prg(0x6000), 0x00);
    }

    #[test]
    fn test_mmc1_consecutive_write_ignore() {
        // MMC1 should ignore consecutive-cycle writes to prevent RMW instructions
        // from shifting two bits. Reset writes (bit 7 set) are never ignored.
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Start with a clean shift register (reset it first)
        mapper.write_prg(0x8000, 0x80); // Reset, cycle 0

        // Advance to cycle 1 for first real write
        mapper.cpu_cycle(); // cycle = 1

        // First write on cycle 1: shift in bit 1 (write_count = 1)
        mapper.write_prg(0x8000, 0x01); // last_write = 1, write_count = 1

        // Immediately write again on same cycle (simulates RMW) - should be IGNORED
        mapper.write_prg(0x8000, 0x01); // still cycle 1, ignored

        // Advance to cycle 2
        mapper.cpu_cycle(); // cycle = 2

        // Second accepted write on cycle 2 - bit 0 (write_count = 2)
        mapper.write_prg(0x8000, 0x00); // last_write = 2, write_count = 2

        // Consecutive write again - should be IGNORED
        mapper.write_prg(0x8000, 0x01); // still cycle 2, ignored

        // Advance to cycle 3
        mapper.cpu_cycle(); // cycle = 3

        // Third accepted write - bit 1 (write_count = 3)
        mapper.write_prg(0x8000, 0x01); // last_write = 3, write_count = 3

        // Advance to cycle 4
        mapper.cpu_cycle(); // cycle = 4

        // Fourth accepted write - bit 1 (write_count = 4)
        mapper.write_prg(0x8000, 0x01); // last_write = 4, write_count = 4

        // Advance to cycle 5
        mapper.cpu_cycle(); // cycle = 5

        // Fifth accepted write - bit 0 (write_count = 5, triggers load)
        mapper.write_prg(0x8000, 0x00); // last_write = 5, write_count = 0 (reset after load)

        // We should have shifted in: 1, 0, 1, 1, 0 (in LSB-first order)
        // This gives us 0b01101 in the register
        // Bits 0-1 = 01 = SingleScreenUpper
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
    }

    #[test]
    fn test_mmc1_consecutive_reset_not_ignored() {
        // Reset writes (bit 7 set) should never be ignored, even if consecutive
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Start loading a value
        mapper.write_prg(0x8000, 0x01);
        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01);
        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01);

        // Consecutive reset write - should NOT be ignored
        mapper.write_prg(0x8000, 0x80); // Reset

        // The shift register should be reset
        // Load a new value: 0b00000 (all zeros)
        for _ in 0..5 {
            mapper.cpu_cycle();
            mapper.write_prg(0x8000, 0x00);
        }

        // Should have mirroring mode 0 = SingleScreenLower
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);
    }
}
