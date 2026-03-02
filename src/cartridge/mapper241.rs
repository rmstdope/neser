//! Mapper 241 - BxROM variant (150-in-1)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_241>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 241 - BxROM variant (150-in-1)
///
/// Hardware: Simple 32KB PRG bank switching with CHR-RAM.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_241>
/// - PRG-ROM: Up to 512KB (16 x 32KB banks)
/// - CHR: 8KB CHR-RAM
/// - Mirroring: Fixed from header
///
/// Register: Any write to $8000-$FFFF selects 32KB PRG bank from data value.
pub struct Mapper241 {
    base: BaseMapper,
}

impl Mapper241 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        Self { base }
    }
}

impl Mapper for Mapper241 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.base.select_prg_page(0, value as i16);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base().read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base_mut().write_chr(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.base.prg_page(0) as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.base.select_prg_page(0, value as i16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper241(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            241, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_241() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal);
        assert!(mapper.is_ok(), "Mapper 241 should be creatable via factory");
    }

    #[test]
    fn test_initial_prg_bank_is_zero() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_prg_bank_switching_via_data_write() {
        let prg_rom = banked_data(32 * 1024, 8);
        let mut mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();

        // Select bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xFFFF), 3);

        // Select bank 7
        mapper.write_prg(0xFFFF, 7);
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.read_prg(0xFFFF), 7);
    }

    #[test]
    fn test_prg_bank_wrapping() {
        let prg_rom = banked_data(32 * 1024, 4); // 4 banks
        let mut mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();

        // Bank 5 should wrap to bank 1 (5 % 4 = 1)
        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0x8000), 1);
    }

    #[test]
    fn test_chr_ram_read_write() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1FFF, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper241(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Write should not change mirroring
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(32 * 1024, 8);
        let mut mapper =
            create_mapper241(prg_rom.clone(), vec![], NametableLayout::Horizontal).unwrap();

        mapper.write_prg(0x8000, 5);
        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();
        restored.restore_registers(&regs);
        assert_eq!(restored.read_prg(0x8000), 5);
    }

    #[test]
    fn test_prg_ram_at_6000_7fff() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper241(prg_rom, vec![], NametableLayout::Horizontal).unwrap();

        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x7FFF, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
        assert_eq!(mapper.read_prg(0x7FFF), 0xAB);
    }
}
