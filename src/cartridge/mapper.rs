use crate::cartridge::MirroringMode;
use std::io;

// Memory size constants
const CHR_RAM_SIZE: usize = 8192; // 8KB
const PRG_RAM_SIZE: usize = 8192; // 8KB
const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const PRG_BANK_SIZE_32K: usize = 0x8000; // 32KB (for AxROM)
const CHR_BANK_SIZE_4K: usize = 0x1000; // 4KB (for MMC1, MMC3)
const CHR_BANK_SIZE_8K: usize = 0x2000; // 8KB
const CHR_MASK: u16 = 0x1FFF; // 8KB mask

// MMC1 specific constants
const MMC1_SHIFT_REGISTER_RESET: u8 = 0x80; // Bit 7 set triggers reset
const MMC1_WRITE_COUNT_MAX: u8 = 5; // Number of writes to load a register
const MMC1_DEFAULT_CONTROL: u8 = 0x0C; // PRG mode 3, CHR mode 0

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
    /// Read a byte from PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, battery-backed on some cartridges)
    /// - $8000-$FFFF: PRG-ROM (with bank switching on advanced mappers)
    /// Returns the byte at the given address after bank translation
    fn read_prg(&self, addr: u16) -> u8;

    /// Write a byte to PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, writable)
    /// - $8000-$FFFF: Mapper control registers (PRG-ROM is read-only)
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

/// UxROM mapper (Mapper 2)
///
/// PRG banking mapper with switchable lower bank and fixed upper bank.
/// Supports:
/// - 16KB switchable PRG bank at $8000-$BFFF
/// - 16KB fixed PRG bank at $C000-$FFFF (always last bank)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 8KB CHR-RAM (no CHR ROM banking)
/// - Bank select register at $8000-$FFFF (any write)
///
/// Common in games like Mega Man, Castlevania, Contra, Duck Tales, Metal Gear.
pub struct UxROMMapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_ram: Vec<u8>,
    mirroring: MirroringMode,
    bank_select: u8,
}

impl UxROMMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // UxROM uses CHR-RAM, ignore chr_rom parameter
        Self {
            prg_rom,
            prg_ram: vec![0; PRG_RAM_SIZE],
            chr_ram: vec![0; CHR_RAM_SIZE],
            mirroring,
            bank_select: 0,
        }
    }

    fn get_last_bank_offset(&self) -> usize {
        self.prg_rom.len().saturating_sub(PRG_BANK_SIZE)
    }
}

impl Mapper for UxROMMapper {
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

                if addr < 0xC000 {
                    // $8000-$BFFF: Switchable 16KB bank
                    let bank_offset = (self.bank_select as usize) * PRG_BANK_SIZE;
                    let index = bank_offset + offset;
                    self.prg_rom.get(index).copied().unwrap_or(0)
                } else {
                    // $C000-$FFFF: Fixed to last 16KB bank
                    let last_bank_offset = self.get_last_bank_offset();
                    let index = last_bank_offset + (offset - PRG_BANK_SIZE);
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
            // Any write to $8000-$FFFF sets the bank register
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
        // UxROM doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }
}

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

/// Create a mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        0 => Ok(Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring))),
        1 => Ok(Box::new(MMC1Mapper::new(prg_rom, chr_rom, mirroring))),
        2 => Ok(Box::new(UxROMMapper::new(prg_rom, chr_rom, mirroring))),
        3 => Ok(Box::new(CNROMMapper::new(prg_rom, chr_rom, mirroring))),
        7 => Ok(Box::new(AxROMMapper::new(prg_rom, chr_rom, mirroring))),
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

    // UxROM (Mapper 2) Tests

    #[test]
    fn test_uxrom_128kb_prg_bank_switching() {
        // Create 128KB (8 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];

        // Fill each bank with its bank number
        for bank in 0..8 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Initially bank 0 should be at $8000-$BFFF
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Last bank (7) should always be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_prg(0xFFFF), 7);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 3);

        // Last bank should remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 7);

        // Switch to bank 5
        mapper.write_prg(0xFFFF, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);

        // Last bank still fixed
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_uxrom_256kb_prg_bank_switching() {
        // Create 256KB (16 banks of 16KB each) PRG ROM
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Vertical);

        // Last bank (15) should be at $C000-$FFFF
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 10
        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 15);

        // Switch to bank 0
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn test_uxrom_chr_ram() {
        // UxROM uses 8KB CHR-RAM
        let mut mapper = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_uxrom_mirroring() {
        let mapper_h = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = UxROMMapper::new(vec![0; 128 * 1024], vec![], MirroringMode::Vertical);
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_uxrom_bank_register_mask() {
        // Test that all 8 bits of the bank register work
        let mut prg_rom = vec![0; 256 * 1024]; // 16 banks

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Test writing different bit patterns
        mapper.write_prg(0x8000, 0b0000_0000); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 0b0000_0111); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 70);

        mapper.write_prg(0x8000, 0b0000_1111); // Bank 15
        assert_eq!(mapper.read_prg(0x8000), 150);
    }

    #[test]
    fn test_uxrom_fixed_last_bank() {
        // Verify that $C000-$FFFF is always the last bank regardless of switches
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..16 {
            let start = bank * 16 * 1024;
            let end = start + 16 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = UxROMMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // Last bank should always read 115 (bank 15 + 100)
        assert_eq!(mapper.read_prg(0xC000), 115);
        assert_eq!(mapper.read_prg(0xFFFF), 115);

        // Switch banks several times
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0xC000), 115);

        mapper.write_prg(0x8000, 10);
        assert_eq!(mapper.read_prg(0xC000), 115);
    }

    // CNROM (Mapper 3) Tests

    #[test]
    fn test_cnrom_32kb_prg_no_banking() {
        // CNROM has 32KB PRG ROM with no banking (like NROM)
        let mut prg_rom = vec![0; 32 * 1024];

        // Fill with pattern - each 1KB block gets a unique value
        for (i, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (i / 1024) as u8;
        }

        let mapper = CNROMMapper::new(prg_rom, vec![0; 32 * 1024], MirroringMode::Horizontal);

        // PRG ROM should be accessible at $8000-$FFFF
        assert_eq!(mapper.read_prg(0x8000), 0); // First byte of first 1KB block
        assert_eq!(mapper.read_prg(0x9000), 4); // $9000 = $8000 + $1000 = 4KB offset = block 4
        assert_eq!(mapper.read_prg(0xC000), 16); // $C000 = $8000 + $4000 = 16KB offset = block 16
        assert_eq!(mapper.read_prg(0xFFFF), 31); // $FFFF = last byte of block 31
    }

    #[test]
    fn test_cnrom_chr_bank_switching_4_banks() {
        // 32KB CHR ROM = 4 banks of 8KB
        let mut chr_rom = vec![0; 32 * 1024];

        // Fill each 8KB bank with its bank number
        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 10);
        assert_eq!(mapper.read_chr(0x1FFF), 10);

        // Switch to bank 2
        mapper.write_prg(0x8000, 0b0000_0010);
        assert_eq!(mapper.read_chr(0x0000), 20);
        assert_eq!(mapper.read_chr(0x1FFF), 20);

        // Switch to bank 3
        mapper.write_prg(0x8000, 0b0000_0011);
        assert_eq!(mapper.read_chr(0x0000), 30);
        assert_eq!(mapper.read_chr(0x1FFF), 30);

        // Switch back to bank 0
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn test_cnrom_chr_bank_switching_2_banks() {
        // 16KB CHR ROM = 2 banks of 8KB
        let mut chr_rom = vec![0; 16 * 1024];

        for bank in 0..2 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 50) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Vertical);

        // Initially bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 50);

        // Writing higher bits should wrap (only 2 banks available)
        mapper.write_prg(0x8000, 0b0000_0011); // Bank 3 wraps to bank 1
        assert_eq!(mapper.read_chr(0x0000), 50);
    }

    #[test]
    fn test_cnrom_chr_read_only() {
        // CNROM uses CHR-ROM, not CHR-RAM - writes should be ignored
        let chr_rom = vec![0xAA; 32 * 1024];
        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Try to write to CHR
        mapper.write_chr(0x0000, 0x55);

        // Should still read original ROM value
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let mapper_h = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Horizontal,
        );
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = CNROMMapper::new(
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            MirroringMode::Vertical,
        );
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_cnrom_bank_select_any_address() {
        // CNROM responds to writes anywhere in $8000-$FFFF
        let mut chr_rom = vec![0; 32 * 1024];

        for bank in 0..4 {
            let start = bank * 8 * 1024;
            let end = start + 8 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = CNROMMapper::new(vec![0; 32 * 1024], chr_rom, MirroringMode::Horizontal);

        // Write to different addresses in PRG space
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x0000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_chr(0x0000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_chr(0x0000), 103);
    }

    // AxROM (Mapper 7) Tests

    #[test]
    fn test_axrom_256kb_prg_bank_switching() {
        // AxROM with 256KB (8 banks × 32KB)
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each 32KB bank with its bank number
        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Default bank should be 0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_axrom_bank_select_bits_0_2() {
        // Test that bits 0-2 select the bank (3-bit bank select = 8 banks max)
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to $8000 with different bank values
        mapper.write_prg(0x8000, 0x00); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 0x01); // Bank 1
        assert_eq!(mapper.read_prg(0x8000), 101);

        mapper.write_prg(0x8000, 0x07); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 107);

        // Test that upper bits are ignored (only bits 0-2 matter for bank)
        mapper.write_prg(0x8000, 0xF2); // 0b11110010 -> bank 2
        assert_eq!(mapper.read_prg(0x8000), 102);
    }

    #[test]
    fn test_axrom_chr_ram() {
        // AxROM uses 8KB CHR-RAM (no CHR ROM)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        // Read back
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_lower() {
        // Bit 4 = 0 selects lower nametable (single-screen A)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x8000, 0x00); // Bits: 0000 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);

        // Write with bit 4 = 0 but other bits set
        mapper.write_prg(0x8000, 0x07); // Bits: 0000 0111
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_upper() {
        // Bit 4 = 1 selects upper nametable (single-screen B)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x8000, 0x10); // Bits: 0001 0000
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreen);
    }

    #[test]
    fn test_axrom_128kb_rom_4_banks() {
        // Test with 128KB ROM (4 banks × 32KB)
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Select each of the 4 banks
        for bank in 0..4 {
            mapper.write_prg(0x8000, bank as u8);
            assert_eq!(mapper.read_prg(0x8000), (bank + 50) as u8);
        }

        // Bank numbers wrap (bank 7 % 4 = 3)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 53); // Bank 3
    }

    #[test]
    fn test_axrom_register_write_any_address() {
        // Writes anywhere in $8000-$FFFF should change the bank
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to different addresses in PRG ROM space
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 11);

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 12);
    }

    #[test]
    fn test_axrom_prg_ram_support() {
        // AxROM should support PRG-RAM at $6000-$7FFF
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(7, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("Failed to create AxROM mapper");

        // Write to PRG-RAM
        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        // Read back
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
    }

    // MMC1 (Mapper 1) Tests

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
