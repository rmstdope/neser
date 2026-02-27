//! Mapper 1 - MMC1 (SxROM)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::trace_mapper;

// Memory size constants
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

/// Mapper 1 - MMC1 (SxROM boards)
///
/// Hardware: Nintendo's most common mapper with serial shift register interface
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/MMC1>
/// - Revisions: <https://www.nesdev.org/wiki/MMC1#ASIC_Revisions>
/// - Board variants: <https://www.nesdev.org/wiki/SxROM>
/// - PRG-ROM: Up to 512KB (32 16KB banks)
/// - PRG-RAM: 8KB or 32KB (depends on board)
/// - CHR: Up to 128KB (32 4KB banks or 16 8KB banks, ROM or RAM)
/// - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
///
/// Common boards: NES-SLROM, NES-SNROM, NES-SGROM, NES-SKROM, NES-SUROM
///
/// Notes:
/// - Shift register interface (5-bit serial loading)
/// - Bit 7 write triggers reset, sets control to mode 3 (PRG fixed at $C000)
/// - MMC1A revision: PRG-RAM always enabled
/// - MMC1B/C revisions: PRG-RAM controllable via register bit
/// - Used in The Legend of Zelda, Metroid, Mega Man 2, Final Fantasy
///
/// PRG-RAM Enable (Revision-Specific):
/// - MMC1A: PRG-RAM is always enabled, bit 4 of PRG bank register is ignored
/// - MMC1B/C: Bit 4 controls PRG-RAM (0 = enabled, 1 = disabled)
///
/// SxROM Board Variant Support:
/// - SUROM (512KB PRG-ROM): CHR bank 0 bit 4 selects 256KB outer PRG-ROM bank
/// - SOROM/SXROM (>8KB PRG-RAM): CHR bank 0 bit 3 selects active 8KB PRG-RAM bank
///
/// See: https://www.nesdev.org/wiki/MMC1#ASIC_Revisions
///      https://www.nesdev.org/wiki/SxROM
///
/// Used in games like The Legend of Zelda, Metroid, Mega Man 2, Final Fantasy.
pub struct MMC1Mapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_memory: ChrMemory,

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

    // SxROM board variant flags (detected from ROM metadata at construction)
    surom: bool, // SUROM: 512KB PRG-ROM; chr_bank_0[4] selects 256KB outer bank
    sorom: bool, // SOROM/SXROM: >8KB PRG-RAM; chr_bank_0[3] selects 8KB PRG-RAM bank

    // Cycle tracking for consecutive-write ignore behavior
    cpu_cycle_count: u64,  // Current CPU cycle count
    last_write_cycle: u64, // CPU cycle of last write to shift register
}

impl MMC1Mapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_ram_size = (ctx.prg_ram_banks_8k as usize) * 8192;
        let surom = ctx.prg_rom.len() > 256 * 1024;
        let sorom = ctx.prg_ram_banks_8k >= 2;
        Self {
            prg_rom: ctx.prg_rom,
            prg_ram: vec![0; prg_ram_size],
            chr_memory: ChrMemory::new(ctx.chr_rom),
            shift_register: 0x10,
            write_count: 0,
            control: MMC1_DEFAULT_CONTROL,
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            revision: Mmc1Revision::Mmc1B,
            surom,
            sorom,
            cpu_cycle_count: 0,
            last_write_cycle: 0,
        }
    }

    #[cfg(test)]
    pub fn new_with_revision(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        _mirroring: NametableLayout,
        revision: Mmc1Revision,
    ) -> Self {
        let surom = prg_rom.len() > 256 * 1024;
        Self {
            prg_rom,
            prg_ram: vec![0; 8192],
            chr_memory: ChrMemory::new(chr_rom),
            shift_register: 0x10, // Power-on state: bit 4 set
            write_count: 0,
            control: MMC1_DEFAULT_CONTROL, // Default: PRG mode 3 (fix last bank), CHR mode 0
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            revision,
            surom,
            sorom: false,
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

    fn is_mmc1a_a17_bypass_active(&self) -> bool {
        match self.revision {
            #[cfg(test)]
            Mmc1Revision::Mmc1A => (self.prg_bank & 0x10) != 0,
            Mmc1Revision::Mmc1B => false,
        }
    }

    fn mmc1a_a17_bit(&self) -> usize {
        ((self.prg_bank >> 3) & 0x01) as usize
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

    fn get_mirroring_mode(&self) -> NametableLayout {
        match self.control & 0x03 {
            0 => NametableLayout::SingleScreenLower, // One-screen, lower bank
            1 => NametableLayout::SingleScreenUpper, // One-screen, upper bank
            2 => NametableLayout::Vertical,
            3 => NametableLayout::Horizontal,
            _ => unreachable!(),
        }
    }

    fn get_prg_bank_offset(&self, addr: u16) -> usize {
        let prg_mode = self.get_prg_mode();
        let num_banks = self.prg_rom.len() / PRG_BANK_SIZE;
        let mmc1a_a17_bypass = self.is_mmc1a_a17_bypass_active();

        // SUROM: chr_bank_0 bit 4 selects which 256KB outer half of 512KB PRG-ROM
        let outer_bank = if self.surom {
            ((self.chr_bank_0 >> 4) & 1) as usize
        } else {
            0
        };

        match prg_mode {
            0 | 1 => {
                // 32KB mode: switch entire $8000-$FFFF, ignore low bit of bank number
                let inner = ((self.prg_bank & 0x0E) >> 1) as usize;
                let bank = (outer_bank << 3) | inner;
                let bank = bank % (num_banks / 2).max(1);
                bank * PRG_BANK_SIZE * 2
            }
            2 => {
                // Fix first bank at $8000, switch 16KB bank at $C000
                if addr < 0xC000 {
                    let bank = if mmc1a_a17_bypass {
                        self.mmc1a_a17_bit() << 3
                    } else {
                        outer_bank << 4 // first bank of current outer half
                    };
                    (bank % num_banks.max(1)) * PRG_BANK_SIZE
                } else {
                    let bank = (outer_bank << 4) | (self.prg_bank & 0x0F) as usize;
                    let bank = bank % num_banks.max(1);
                    bank * PRG_BANK_SIZE
                }
            }
            3 => {
                // Switch 16KB bank at $8000, fix last bank at $C000
                if addr < 0xC000 {
                    let bank = (outer_bank << 4) | (self.prg_bank & 0x0F) as usize;
                    let bank = bank % num_banks.max(1);
                    bank * PRG_BANK_SIZE
                } else {
                    let bank = if mmc1a_a17_bypass {
                        (self.mmc1a_a17_bit() << 3) | 0x07
                    } else {
                        (outer_bank << 4) | 0x0F // last bank of current outer half
                    };
                    (bank % num_banks.max(1)) * PRG_BANK_SIZE
                }
            }
            _ => unreachable!(),
        }
    }

    fn prg_ram_bank_offset(&self, addr: u16) -> usize {
        let base = (addr - 0x6000) as usize;
        if self.sorom {
            let bank = ((self.chr_bank_0 >> 3) & 1) as usize;
            bank * 8192 + base
        } else {
            base
        }
    }

    fn get_chr_bank_offset(&self, addr: u16) -> usize {
        let chr_mode = self.get_chr_mode();
        let num_4kb_banks = self.chr_memory.size() / CHR_BANK_SIZE_4K;

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
                if !self.is_wram_enabled() {
                    return 0; // Return 0 when WRAM is disabled (open bus behavior)
                }
                let offset = self.prg_ram_bank_offset(addr);
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
                if !self.is_wram_enabled() {
                    return; // Ignore writes when WRAM is disabled
                }
                let offset = self.prg_ram_bank_offset(addr);
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

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = if self.get_chr_mode() == 0 {
            // 8KB mode
            (addr & 0x1FFF) as usize
        } else {
            // 4KB mode
            (addr & 0x0FFF) as usize
        };
        let index = bank_offset + offset;
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = if self.get_chr_mode() == 0 {
            // 8KB mode
            (addr & 0x1FFF) as usize
        } else {
            // 4KB mode
            (addr & 0x0FFF) as usize
        };
        let index = bank_offset + offset;
        self.chr_memory.write_at_index(index, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
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
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        crate::console::initialize_ram(&mut self.prg_ram, mode);
        self.chr_memory.initialize(mode);
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

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: (self.prg_ram.len() / 1024).max(8),
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 4,
            trainer_jsr: false,
            ..Default::default()
        }
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
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(1, prg_rom, chr_rom, mirroring))
            .expect("MMC1 (mapper 1) should be implemented")
    }

    #[test]
    fn test_mmc1_shift_register_load() {
        // MMC1 requires 5 sequential writes to load a register
        // Each write shifts bit 0 into the shift register
        // Writing with bit 7 set resets the shift register and control register

        let prg_rom = vec![0; 128 * 1024]; // 128KB = 8 banks of 16KB
        let chr_rom = vec![0; 32 * 1024]; // 32KB = 8 banks of 4KB
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mmc1_registers_snapshot_preserves_write_ignore_timing() {
        let prg_rom = vec![0; PRG_BANK_SIZE * 2];
        let chr_rom = vec![];

        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01);

        let saved = mapper.registers_snapshot();

        let mut restored = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Load 0b00000 (mirroring = 0 = SingleScreenLower)
        for _ in 0..5 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Load 0b00001 (mirroring = 1 = SingleScreenUpper)
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..4 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        // Load 0b00010 (mirroring = 2 = Vertical)
        mapper.write_prg(0x8000, 0b00000000);
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Load 0b00011 (mirroring = 3 = Horizontal)
        mapper.write_prg(0x8000, 0b00000001);
        mapper.write_prg(0x8000, 0b00000001);
        for _ in 0..3 {
            mapper.write_prg(0x8000, 0b00000000);
        }
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, vec![], NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write sequence: 1, 0, 1, 0, 1 should result in value 0b10101
        // With proper power-on state (0x10), after 5 writes we should see the bit pattern
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1
        mapper.write_prg(0x8000, 0b00000000); // Write bit 0 = 0
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1
        mapper.write_prg(0x8000, 0b00000000); // Write bit 0 = 0
        mapper.write_prg(0x8000, 0b00000001); // Write bit 0 = 1 (5th write, should load)

        // The control register should now contain 0b10101 (mirroring = 0b01 = SingleScreenUpper)
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_mmc1_shift_register_reset_clears_to_power_on_state() {
        // After reset, the shift register should go back to 0x10
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mmc1_wram_enable_disable() {
        // PRG bank register bit 4 controls WRAM enable/disable
        // When bit 4 is set (1), WRAM is disabled
        // When bit 4 is clear (0), WRAM is enabled
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
        let mut mapper = create_mmc1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal);

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
            NametableLayout::Horizontal,
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
            NametableLayout::Horizontal,
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
        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Write to WRAM
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable WRAM (should work on MMC1B)
        write_register(&mut mapper, 0xE000, 0b10000);

        // Should be disabled (reads 0)
        assert_eq!(mapper.read_prg(0x6000), 0x00);
    }

    #[test]
    fn test_mmc1a_prg_bit4_enables_a17_bypass_for_fixed_last_bank() {
        let mut prg_rom = vec![0; 16 * PRG_BANK_SIZE];
        let chr_rom = vec![0; 8 * 1024];

        for bank in 0..16 {
            let start = bank * PRG_BANK_SIZE;
            let end = start + PRG_BANK_SIZE;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = MMC1Mapper::new_with_revision(
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
            Mmc1Revision::Mmc1A,
        );

        // PRG mode 3 (switch $8000, fixed $C000)
        write_register(&mut mapper, 0x8000, 0b01100);
        // bit4=1 enables MMC1A bypass, bit3=0 drives A17 low
        write_register(&mut mapper, 0xE000, 0b10000);

        // MMC1A bypass applies A17 from bit3 to fixed-bank path, selecting bank 7
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_mmc1b_prg_bit4_does_not_change_fixed_last_bank() {
        let mut prg_rom = vec![0; 16 * PRG_BANK_SIZE];
        let chr_rom = vec![0; 8 * 1024];

        for bank in 0..16 {
            let start = bank * PRG_BANK_SIZE;
            let end = start + PRG_BANK_SIZE;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = MMC1Mapper::new_with_revision(
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
            Mmc1Revision::Mmc1B,
        );

        // PRG mode 3 (switch $8000, fixed $C000)
        write_register(&mut mapper, 0x8000, 0b01100);
        // On MMC1B, bit4 is WRAM control only; fixed bank remains last bank
        write_register(&mut mapper, 0xE000, 0b10000);

        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn test_mmc1_consecutive_write_ignore() {
        // MMC1 should ignore consecutive-cycle writes to prevent RMW instructions
        // from shifting two bits. Reset writes (bit 7 set) are never ignored.
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_mmc1_consecutive_reset_not_ignored() {
        // Reset writes (bit 7 set) should never be ignored, even if consecutive
        let prg_rom = vec![0; 128 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_mmc1_comprehensive_state_roundtrip() {
        // Test round-trip of complete MMC1 state including all registers and banking modes
        let mut prg_rom = vec![0; 256 * 1024]; // 256KB = 16 banks
        let mut chr_rom = vec![0; 128 * 1024]; // 128KB = 32 4KB banks

        // Fill PRG ROM with bank-specific data
        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        // Fill CHR ROM with bank-specific data
        for bank in 0..32 {
            let start = bank * 4 * 1024;
            let end = start + 4 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // Configure complex state with all registers
        // Control register: CHR mode 1 (two 4KB banks), PRG mode 3 (switch at $8000, fixed last at $C000), horizontal mirroring
        // Control value: bits 0-1 = 11 (horizontal), bits 2-3 = 11 (PRG mode 3), bit 4 = 1 (CHR mode 1)
        // = 0b11111 = 31
        for i in 0..5 {
            mapper.cpu_cycle();
            mapper.write_prg(0x8000, (31 >> i) & 0x01); // Control = 31
        }

        // CHR bank 0: Select bank 5
        for i in 0..5 {
            mapper.cpu_cycle();
            mapper.write_prg(0xA000, (5 >> i) & 0x01);
        }

        // CHR bank 1: Select bank 10
        for i in 0..5 {
            mapper.cpu_cycle();
            mapper.write_prg(0xC000, (10 >> i) & 0x01);
        }

        // PRG bank: Select bank 7
        for i in 0..5 {
            mapper.cpu_cycle();
            mapper.write_prg(0xE000, (7 >> i) & 0x01);
        }

        // Write to PRG-RAM
        for i in 0..256 {
            mapper.write_prg(0x6000 + i, (i & 0xFF) as u8);
        }

        // Verify state before snapshot
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_chr(0x0000), 105); // CHR bank 5
        assert_eq!(mapper.read_chr(0x1000), 110); // CHR bank 10
        assert_eq!(mapper.read_prg(0x8000), 7); // Switchable bank 7 (PRG mode 3)
        assert_eq!(mapper.read_prg(0xC000), 15); // Fixed last bank (bank 15 = last of 16)
        assert_eq!(mapper.read_prg(0x6000), 0); // PRG-RAM

        // Take snapshot
        let registers = mapper.registers_snapshot();
        let prg_ram = mapper.wram_snapshot();
        let chr_ram = mapper.chr_ram_snapshot();

        // Create fresh mapper and restore
        let mut restored = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&registers);
        restored.load_wram_snapshot(&prg_ram);
        restored.restore_chr_ram(&chr_ram);

        // Verify all state is restored correctly
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_chr(0x0000), 105); // CHR bank 5
        assert_eq!(restored.read_chr(0x1000), 110); // CHR bank 10
        assert_eq!(restored.read_prg(0x8000), 7); // Switchable bank 7
        assert_eq!(restored.read_prg(0xC000), 15); // Fixed last bank
        assert_eq!(restored.read_prg(0x6000), 0); // PRG-RAM restored
        assert_eq!(restored.read_prg(0x60FF), 0xFF); // PRG-RAM restored
    }

    #[test]
    fn test_mmc1_shift_register_mid_write_roundtrip() {
        // Test that a partially-loaded shift register is preserved across save/load
        let prg_rom = vec![0; 256 * 1024];
        let chr_rom = vec![0; 128 * 1024];

        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // Start writing to control register but don't finish
        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01); // 1st bit
        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x01); // 2nd bit
        mapper.cpu_cycle();
        mapper.write_prg(0x8000, 0x00); // 3rd bit (now shift_register has 0b00000101, write_count=3)

        // Take snapshot mid-write
        let registers = mapper.registers_snapshot();

        // Restore to new mapper
        let mut restored = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&registers);

        // Complete the write sequence
        restored.cpu_cycle();
        restored.write_prg(0x8000, 0x01); // 4th bit
        restored.cpu_cycle();
        restored.write_prg(0x8000, 0x01); // 5th bit (completes write sequence)

        // The control register should now be set to 0b11011 = 27
        // Bits are loaded LSB first: 1, 1, 0, 1, 1
        // Mirroring mode (bits 0-1): 11 = Horizontal
        // PRG mode (bits 2-3): 01 = mode 1
        // CHR mode (bit 4): 1 = two 4KB banks
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }

    /// Test MMC1 disabled WRAM returns open-bus
    ///
    /// MMC1B/C can disable WRAM via bit 4 of the PRG bank register ($E000-$FFFF).
    /// When disabled, reads from $6000-$7FFF should return open-bus.
    #[test]
    fn test_mmc1_disabled_wram_returns_open_bus() {
        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            vec![0; 256 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        // First, enable WRAM and write some data
        // Write to $E000-$FFFF controls PRG banking and WRAM enable
        // We need to do 5 consecutive writes to load the shift register

        // Reset shift register
        mapper.write_prg(0x8000, 0x80);

        // Write pattern to enable WRAM: bit 4 = 0
        // We'll write 5 times with bit 0 = 0 each time to load 0x00 into register
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0x00);
        }

        // Write to WRAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7000, 0xBB);

        // Verify WRAM reads work when enabled
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7000), 0xBB);

        // Now disable WRAM by setting bit 4 = 1 in PRG bank register
        // Reset shift register
        mapper.write_prg(0x8000, 0x80);

        // Write pattern to disable WRAM: bit 4 = 1
        // We need to shift in 10000 binary = 0x10
        // Shift register is filled LSB first, so: bit0, bit1, bit2, bit3, bit4
        mapper.write_prg(0xE000, 0x00); // bit 0 = 0
        mapper.write_prg(0xE000, 0x00); // bit 1 = 0
        mapper.write_prg(0xE000, 0x00); // bit 2 = 0
        mapper.write_prg(0xE000, 0x00); // bit 3 = 0
        mapper.write_prg(0xE000, 0x01); // bit 4 = 1

        // read_prg should return 0 for backward compatibility
        assert_eq!(mapper.read_prg(0x6000), 0x00);
        assert_eq!(mapper.read_prg(0x7000), 0x00);

        // read_prg_open_bus should return the open-bus value
        let open_bus = 0x42;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $6000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7000, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $7000"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus),
            open_bus,
            "Disabled WRAM should return open-bus at $7FFF"
        );
    }

    // ===== SxROM Variant Tests (Issue #327) =====

    /// SUROM: 512KB PRG-ROM — CHR bank 0 bit 4 selects 256KB outer PRG-ROM bank.
    ///
    /// The 5-bit CHR bank register has bit 4 repurposed as PRG-ROM A18 on SUROM boards.
    /// This allows addressing 512KB of PRG-ROM even though the PRG bank register only holds
    /// the low 4 bits (0-15 within the active 256KB half).
    #[test]
    fn test_mmc1_surom_outer_prg_bank_selection() {
        // 512KB PRG-ROM = 32 × 16KB banks; fill each bank with its bank number
        let mut prg_rom = vec![0u8; 512 * 1024];
        for bank in 0..32usize {
            let offset = bank * PRG_BANK_SIZE;
            prg_rom[offset..offset + PRG_BANK_SIZE].fill(bank as u8);
        }

        let mut mapper = MMC1Mapper::new(
            MapperContext::new_for_test(1, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );

        // PRG mode 3 (default): $8000 switches 16KB, last bank fixed at $C000
        // Select PRG bank 0 in lower half (outer bank bit = 0)
        write_register(&mut mapper, 0xA000, 0x00); // chr_bank_0 = 0 (outer=0, bank 0)
        write_register(&mut mapper, 0xE000, 0x00); // prg_bank = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "With outer bank 0, bank 0 fills $8000"
        );

        // Now set chr_bank_0 bit 4 = 1 → switch to upper 256KB outer bank
        write_register(&mut mapper, 0xA000, 0x10); // chr_bank_0 = 0x10 (outer=1, bank 16)
        write_register(&mut mapper, 0xE000, 0x00); // prg_bank = 0 (within outer half)
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "With outer bank 1, prg_bank 0 maps to ROM bank 16"
        );

        // Verify outer bank bit also works for non-zero inner bank
        write_register(&mut mapper, 0xA000, 0x10); // outer=1
        write_register(&mut mapper, 0xE000, 0x02); // inner prg_bank = 2 → bank 18
        assert_eq!(
            mapper.read_prg(0x8000),
            18,
            "Outer bank 1 + inner bank 2 maps to ROM bank 18"
        );
    }

    /// SOROM: 16KB PRG-RAM (2×8KB) — CHR bank 0 bit 3 selects the active PRG-RAM bank.
    ///
    /// SOROM boards connect CHR A13 to the /CE of a second 8KB PRG-RAM chip, effectively
    /// doubling the accessible PRG-RAM to 16KB via software bank selection.
    #[test]
    fn test_mmc1_sorom_prg_ram_banking() {
        let prg_rom = vec![0u8; 256 * 1024];

        let mut mapper = MMC1Mapper::new(
            MapperContext::new_for_test(1, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(2), // 2 × 8KB = 16KB PRG-RAM → SOROM
        );

        // Ensure WRAM is enabled: set prg_bank bit 4 = 0
        write_register(&mut mapper, 0xE000, 0x00);

        // Write to PRG-RAM bank 0 (chr_bank_0 bit 3 = 0)
        write_register(&mut mapper, 0xA000, 0x00); // chr_bank_0 bit 3 = 0 → bank 0
        mapper.write_prg(0x6000, 0xAA);

        // Switch to PRG-RAM bank 1 (chr_bank_0 bit 3 = 1)
        write_register(&mut mapper, 0xA000, 0x08); // chr_bank_0 bit 3 = 1 → bank 1
        mapper.write_prg(0x6000, 0xBB);

        // Read back bank 0 — should still have 0xAA
        write_register(&mut mapper, 0xA000, 0x00); // back to bank 0
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAA,
            "PRG-RAM bank 0 should hold 0xAA independently of bank 1"
        );

        // Read back bank 1 — should still have 0xBB
        write_register(&mut mapper, 0xA000, 0x08); // bank 1
        assert_eq!(
            mapper.read_prg(0x6000),
            0xBB,
            "PRG-RAM bank 1 should hold 0xBB independently of bank 0"
        );
    }

    /// PRG-RAM size respected from metadata: mapper should allocate the correct amount.
    #[test]
    fn test_mmc1_prg_ram_size_from_metadata() {
        let prg_rom = vec![0u8; 256 * 1024];

        let mapper_1kb = MMC1Mapper::new(
            MapperContext::new_for_test(1, prg_rom.clone(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        assert_eq!(
            mapper_1kb.wram_size(),
            8 * 1024,
            "1 bank should allocate 8KB"
        );

        let mapper_2kb = MMC1Mapper::new(
            MapperContext::new_for_test(1, prg_rom.clone(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(2),
        );
        assert_eq!(
            mapper_2kb.wram_size(),
            16 * 1024,
            "2 banks should allocate 16KB"
        );

        let mapper_4kb = MMC1Mapper::new(
            MapperContext::new_for_test(1, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(4),
        );
        assert_eq!(
            mapper_4kb.wram_size(),
            32 * 1024,
            "4 banks should allocate 32KB (SXROM)"
        );
    }

    /// Test MMC1 enabled WRAM doesn't return open-bus
    #[test]
    fn test_mmc1_enabled_wram_returns_data() {
        let mut mapper = MMC1Mapper::new(MapperContext::new_for_test(
            1,
            vec![0; 256 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        // Reset and ensure WRAM is enabled (bit 4 = 0)
        mapper.write_prg(0x8000, 0x80);
        for _ in 0..5 {
            mapper.write_prg(0xE000, 0x00);
        }

        // Write to WRAM
        mapper.write_prg(0x6000, 0x55);

        let open_bus = 0xFF;
        let result = mapper.read_prg_open_bus(0x6000, open_bus);

        // Should return the written value, not open-bus
        assert_eq!(
            result, 0x55,
            "Enabled WRAM should return written data, not open-bus"
        );
    }
}
