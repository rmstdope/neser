//! Mapper 246 - Fong Shen Bang
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_246>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 246 - Fong Shen Bang
///
/// Hardware: Register-based PRG and CHR bank switching.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_246>
/// - PRG-ROM: Up to 512KB (8KB banks)
/// - CHR-ROM: Up to 512KB (2KB banks)
/// - Mirroring: Fixed from header
///
/// Registers at $6000-$6007 (write only, within PRG-RAM range):
/// - $6000: PRG bank 0 ($8000-$9FFF, 8KB)
/// - $6001: PRG bank 1 ($A000-$BFFF, 8KB)
/// - $6002: PRG bank 2 ($C000-$DFFF, 8KB)
/// - $6003: PRG bank 3 ($E000-$FFFF, 8KB)
/// - $6004: CHR bank 0 ($0000-$07FF, 2KB)
/// - $6005: CHR bank 1 ($0800-$0FFF, 2KB)
/// - $6006: CHR bank 2 ($1000-$17FF, 2KB)
/// - $6007: CHR bank 3 ($1800-$1FFF, 2KB)
///
/// PRG-RAM at $6800-$7FFF (6KB usable, excluding register space)
pub struct Mapper246 {
    base: BaseMapper,
    prg_banks: [u8; 4],
    chr_banks: [u8; 4],
}

impl Mapper246 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            ..Default::default()
        };
        let num_prg_banks = ctx.prg_rom.len() / (8 * 1024);
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as u8
        } else {
            0
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(2 * 1024);
        // Initial PRG mapping: banks 0, 1, last-1, last
        let prg_banks = [0, 1, last_bank.saturating_sub(1), last_bank];
        for (slot, &bank) in prg_banks.iter().enumerate() {
            base.select_prg_page(slot, bank as i16);
        }
        Self {
            base,
            prg_banks,
            chr_banks: [0, 0, 0, 0],
        }
    }
}

impl Mapper for Mapper246 {
    fn base(&self) -> Option<&BaseMapper> {
        Some(&self.base)
    }

    fn base_mut(&mut self) -> Option<&mut BaseMapper> {
        Some(&mut self.base)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x67FF => 0, // Register space - reads return open bus
            0x6800..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x6007 => {
                let reg = (addr - 0x6000) as usize;
                if reg < 4 {
                    self.prg_banks[reg] = value;
                    self.base.select_prg_page(reg, value as i16);
                } else {
                    self.chr_banks[reg - 4] = value;
                    self.base.select_chr_page(reg - 4, value as i16);
                }
            }
            0x6800..=0x7FFF => {
                self.base.try_write_prg_ram(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base.read_chr_banked(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base.write_chr_banked(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut regs = Vec::with_capacity(8);
        regs.extend_from_slice(&self.prg_banks);
        regs.extend_from_slice(&self.chr_banks);
        regs
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.prg_banks.copy_from_slice(&data[0..4]);
            for (slot, &bank) in self.prg_banks.iter().enumerate() {
                self.base.select_prg_page(slot, bank as i16);
            }
        }
        if data.len() >= 8 {
            self.chr_banks.copy_from_slice(&data[4..8]);
            for (slot, &bank) in self.chr_banks.iter().enumerate() {
                self.base.select_chr_page(slot, bank as i16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper246(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            246, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_246() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 246 should be creatable via factory");
    }

    #[test]
    fn test_prg_bank_switching_8kb() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set all 4 PRG banks
        mapper.write_prg(0x6000, 5); // $8000-$9FFF = bank 5
        mapper.write_prg(0x6001, 10); // $A000-$BFFF = bank 10
        mapper.write_prg(0x6002, 3); // $C000-$DFFF = bank 3
        mapper.write_prg(0x6003, 7); // $E000-$FFFF = bank 7

        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xA000), 10);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_chr_bank_switching_2kb() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set 4 CHR banks (2KB each covering $0000-$1FFF)
        mapper.write_prg(0x6004, 3); // $0000-$07FF = bank 3
        mapper.write_prg(0x6005, 7); // $0800-$0FFF = bank 7
        mapper.write_prg(0x6006, 1); // $1000-$17FF = bank 1
        mapper.write_prg(0x6007, 12); // $1800-$1FFF = bank 12

        assert_eq!(mapper.read_chr(0x0000), 3);
        assert_eq!(mapper.read_chr(0x0800), 7);
        assert_eq!(mapper.read_chr(0x1000), 1);
        assert_eq!(mapper.read_chr(0x1800), 12);
    }

    #[test]
    fn test_prg_ram_at_6800_7fff() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // PRG-RAM should work at $6800-$7FFF
        mapper.write_prg(0x6800, 0x42);
        mapper.write_prg(0x7FFF, 0xAB);
        assert_eq!(mapper.read_prg(0x6800), 0x42);
        assert_eq!(mapper.read_prg(0x7FFF), 0xAB);
    }

    #[test]
    fn test_register_space_does_not_return_ram() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write to register space
        mapper.write_prg(0x6000, 0xFF);
        // Read from register space should return 0 (open bus), not the written value
        assert_eq!(mapper.read_prg(0x6000), 0);
    }

    #[test]
    fn test_mirroring_is_fixed() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 4);
        let mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Horizontal).unwrap();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_prg_bank_wrapping() {
        let prg_rom = banked_data(8 * 1024, 8); // 8 banks
        let chr_rom = banked_data(2 * 1024, 4);
        let mut mapper = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Bank 10 should wrap to bank 2 (10 % 8)
        mapper.write_prg(0x6000, 10);
        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper =
            create_mapper246(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 5);
        mapper.write_prg(0x6001, 10);
        mapper.write_prg(0x6002, 3);
        mapper.write_prg(0x6003, 7);
        mapper.write_prg(0x6004, 2);
        mapper.write_prg(0x6005, 8);
        mapper.write_prg(0x6006, 1);
        mapper.write_prg(0x6007, 14);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper246(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_prg(0xA000), 10);
        assert_eq!(restored.read_prg(0xC000), 3);
        assert_eq!(restored.read_prg(0xE000), 7);
        assert_eq!(restored.read_chr(0x0000), 2);
        assert_eq!(restored.read_chr(0x0800), 8);
        assert_eq!(restored.read_chr(0x1000), 1);
        assert_eq!(restored.read_chr(0x1800), 14);
    }
}
