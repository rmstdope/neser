//! Mapper 255 - 110-in-1 Multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_255>
//!
//! Known Limitations:
//! - Sub-games that write to $8000-$FFFF (e.g., UxROM/CNROM games) will
//!   inadvertently change the multicart's address latch, switching to wrong
//!   banks. This is a hardware limitation of the multicart, not an emulator
//!   bug (confirmed identical behavior in FCEUX and Nestopia).

use crate::cartridge::NametableLayout;
use crate::cartridge::common::{BankedRom, ChrMemory};
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 255 - 110-in-1 Multicart
///
/// Hardware: Address-latch based bank switching (same as mapper 225 with
/// 4-byte protection RAM).
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_255>
/// - PRG-ROM: Up to 2MB (16KB banks)
/// - CHR-ROM: Up to 512KB (8KB banks)
/// - Mirroring: Software-controlled via address latch
///
/// Write to $8000-$FFFF (address latch):
/// - A14: High PRG chip select (extends PRG addressing)
/// - A13: Mirroring (0=Vertical, 1=Horizontal)
/// - A12: PRG mode (0=32KB, 1=16KB)
/// - A11-A6: PRG bank number
/// - A5-A0: CHR bank number
///
/// Protection RAM at $5800-$5FFF: 4 bytes, only low 4 bits retained.
pub struct Mapper255 {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    /// A14: High bit for PRG/CHR chip select
    chip_select: u8,
    /// A12: PRG mode (0=32KB, 1=16KB)
    prg_mode: u8,
    /// A11-A6: PRG bank
    prg_bank: u8,
    /// A5-A0: CHR bank
    chr_bank: u8,
    /// 4-byte protection RAM at $5800-$5FFF (low 4 bits only)
    protection_ram: [u8; 4],
}

impl Mapper255 {
    const MAPPER_NUMBER: u8 = 255;
    const PRG_BANK_SIZE: usize = 16 * 1024;
    const CHR_BANK_SIZE: usize = 8 * 1024;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            prg_rom: BankedRom::new(prg_rom, Self::PRG_BANK_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            chip_select: 0,
            prg_mode: 0,
            prg_bank: 0,
            chr_bank: 0,
            protection_ram: [0; 4],
        }
    }

    fn resolve_prg_bank(&self, bank: usize) -> usize {
        let num_banks = self.prg_rom.num_banks();
        if num_banks == 0 { 0 } else { bank % num_banks }
    }

    fn full_prg_bank(&self) -> usize {
        ((self.chip_select as usize) << 6) | (self.prg_bank as usize)
    }

    fn full_chr_bank(&self) -> usize {
        let bank = ((self.chip_select as usize) << 6) | (self.chr_bank as usize);
        let num_banks = self.chr_memory.size() / Self::CHR_BANK_SIZE;
        if num_banks == 0 { 0 } else { bank % num_banks }
    }

    fn decode_address_latch(&mut self, addr: u16) {
        self.chip_select = ((addr >> 14) & 1) as u8;
        self.prg_mode = ((addr >> 12) & 1) as u8;
        self.prg_bank = ((addr >> 6) & 0x3F) as u8;
        self.chr_bank = (addr & 0x3F) as u8;

        self.mirroring = if (addr >> 13) & 1 == 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
    }
}

impl Mapper for Mapper255 {
    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5800..=0x5FFF => self.read_prg(addr),
            _ if addr < 0x6000 => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5800..=0x5FFF => {
                let index = (addr - 0x5800) as usize & 0x03;
                self.protection_ram[index] & 0x0F
            }
            0x8000..=0xBFFF => {
                let bank = if self.prg_mode == 0 {
                    // 32KB mode: use bank with bit 0 cleared for low half
                    self.full_prg_bank() & !1
                } else {
                    // 16KB mode: same bank in both halves
                    self.full_prg_bank()
                };
                let bank = self.resolve_prg_bank(bank);
                let offset = (addr - 0x8000) as usize;
                self.prg_rom.read(bank, offset)
            }
            0xC000..=0xFFFF => {
                let bank = if self.prg_mode == 0 {
                    // 32KB mode: use bank with bit 0 set for high half
                    self.full_prg_bank() | 1
                } else {
                    // 16KB mode: same bank in both halves
                    self.full_prg_bank()
                };
                let bank = self.resolve_prg_bank(bank);
                let offset = (addr - 0xC000) as usize;
                self.prg_rom.read(bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5800..=0x5FFF => {
                let index = (addr - 0x5800) as usize & 0x03;
                self.protection_ram[index] = value & 0x0F;
            }
            0x8000..=0xFFFF => {
                self.decode_address_latch(addr);
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank = self.full_chr_bank();
        let offset = addr as usize & 0x1FFF;
        let index = bank * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut regs = Vec::with_capacity(9);
        regs.push(self.chip_select);
        regs.push(self.prg_mode);
        regs.push(self.prg_bank);
        regs.push(self.chr_bank);
        regs.extend_from_slice(&self.protection_ram);
        regs.push(match self.mirroring {
            NametableLayout::Horizontal => 1,
            _ => 0,
        });
        regs
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 8 {
            self.chip_select = data[0];
            self.prg_mode = data[1];
            self.prg_bank = data[2];
            self.chr_bank = data[3];
            self.protection_ram.copy_from_slice(&data[4..8]);
        }
        if data.len() >= 9 {
            self.mirroring = if data[8] == 1 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper255(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(255, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_factory_creates_mapper_255() {
        let prg_rom = banked_data(16 * 1024, 16);
        let chr_rom = banked_data(8 * 1024, 8);
        let mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    #[test]
    fn test_prg_32kb_mode() {
        let prg_rom = banked_data(16 * 1024, 16);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write to $8000 with A12=0 (32KB mode), A11-A6 = bank 4 (0b000100 << 6 = 0x0100)
        // Address: 0x8000 | (bank << 6) = 0x8000 | 0x0100 = 0x8100
        mapper.write_prg(0x8100, 0);

        // In 32KB mode, $8000 gets bank & ~1 = 4, $C000 gets bank | 1 = 5
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    #[test]
    fn test_prg_16kb_mode() {
        let prg_rom = banked_data(16 * 1024, 16);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write with A12=1 (16KB mode), A11-A6 = bank 3 (0b000011 << 6 = 0x00C0)
        // Address: 0x8000 | 0x1000 (A12) | 0x00C0 = 0x90C0
        mapper.write_prg(0x90C0, 0);

        // In 16KB mode, same bank in both $8000 and $C000
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    #[test]
    fn test_chr_bank_switching() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // CHR bank is in A5-A0 = 5
        // Address: 0x8000 | 5 = 0x8005
        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn test_mirroring_via_address_latch() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // A13=0 → Vertical
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // A13=1 → Horizontal (0x2000)
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_protection_ram_via_open_bus() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Protection RAM at $5800-$5FFF: write full byte, only low 4 bits retained.
        // Reads must go through read_prg_open_bus (as the real bus does),
        // NOT through read_prg directly, to ensure the override works.
        let open_bus = 0xAB_u8;

        mapper.write_prg(0x5800, 0xFF);
        assert_eq!(mapper.read_prg_open_bus(0x5800, open_bus), 0x0F);

        mapper.write_prg(0x5801, 0xA5);
        assert_eq!(mapper.read_prg_open_bus(0x5801, open_bus), 0x05);

        // Addresses wrap to 4 bytes
        mapper.write_prg(0x5804, 0x03);
        assert_eq!(mapper.read_prg_open_bus(0x5800, open_bus), 0x03);
    }

    #[test]
    fn test_reads_below_5800_return_open_bus() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Reads from $5000-$57FF should return open bus (not protection RAM)
        let open_bus = 0x77_u8;
        assert_eq!(mapper.read_prg_open_bus(0x5000, open_bus), open_bus);
        assert_eq!(mapper.read_prg_open_bus(0x57FF, open_bus), open_bus);
    }

    #[test]
    fn test_chip_select_extends_prg() {
        // Need enough banks for chip select to matter
        let prg_rom = banked_data(16 * 1024, 128); // 128 banks
        let chr_rom = banked_data(8 * 1024, 128);
        let mut mapper = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // A14=1 → chip_select=1, A11-A6=bank 2 → full bank = (1<<6)|2 = 66
        // A12=1 (16KB mode) → both halves = bank 66
        // Address: 0x8000 | 0x4000 (A14) | 0x1000 (A12) | (2 << 6) = 0x8000 | 0x4000 | 0x1000 | 0x0080 = 0xD080
        mapper.write_prg(0xD080, 0);
        assert_eq!(mapper.read_prg(0x8000), 66);
        assert_eq!(mapper.read_prg(0xC000), 66);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(16 * 1024, 16);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper =
            create_mapper255(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Set some state: A14=0, A13=1 (H mirror), A12=1 (16KB), bank=5, chr=3
        // Address: 0x8000 | 0x2000 (A13) | 0x1000 (A12) | (5<<6) | 3 = 0x8000 | 0x2000 | 0x1000 | 0x0140 | 0x03 = 0xB143
        mapper.write_prg(0xB143, 0);
        mapper.write_prg(0x5800, 0x0A);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper255(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_chr(0x0000), 3);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x5800), 0x0A);
    }
}
