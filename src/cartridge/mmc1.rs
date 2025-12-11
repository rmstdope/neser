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
