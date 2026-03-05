//! Mapper 133 - Sachen 72008 / UNL-SA-72008
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_133>
//! - Implemented variant: 72-pin Sachen 72008 behavior.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

pub struct Mapper133 {
    base: BaseMapper,
    register: u8,
}

impl Mapper133 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);

        Self { base, register: 0 }
    }

    fn is_4100_masked_window(addr: u16) -> bool {
        addr & 0xE100 == 0x4100
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.base.select_prg_page(0, ((value >> 2) & 0x01) as i16);
        self.base.select_chr_page(0, (value & 0x03) as i16);
    }
}

impl Mapper for Mapper133 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_4100_masked_window(addr) {
            self.apply_register(value);
        }
    }

    fn reset(&mut self) {
        self.apply_register(0);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper133(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            133,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_133_is_registered() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);

        let mapper = create_mapper133(prg_rom, chr_rom).expect("mapper 133 should be implemented");
        assert_eq!(mapper.mapper_number(), 133);
    }

    #[test]
    fn mapper_133_write_4100_mask_selects_prg_and_chr_banks_from_pcc() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper133(prg_rom, chr_rom).expect("mapper 133 should be implemented");

        mapper.write_prg(0x4100, 0b0000_0110);

        assert_eq!(mapper.read_prg(0x8000), 1, "P bit selects 32 KiB PRG bank");
        assert_eq!(mapper.read_chr(0x0000), 2, "CC bits select 8 KiB CHR bank");
    }

    #[test]
    fn mapper_133_ignores_writes_outside_e100_4100_decode() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper133(prg_rom, chr_rom).expect("mapper 133 should be implemented");

        mapper.write_prg(0x4100, 0b0000_0011);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 3);

        mapper.write_prg(0x4200, 0b0000_0100);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "non-decoded writes must be ignored"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "non-decoded writes must be ignored"
        );

        mapper.write_prg(0x5100, 0b0000_0100);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "decoded aliases must update the register"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "decoded aliases must update the register"
        );
    }

    #[test]
    fn mapper_133_reset_restores_default_bank_state() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper133(prg_rom, chr_rom).expect("mapper 133 should be implemented");

        mapper.write_prg(0x4100, 0b0000_0111);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 3);

        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn mapper_133_restores_register_state_from_snapshot() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper133(prg_rom, chr_rom).expect("mapper 133 should be implemented");

        mapper.write_prg(0x4100, 0b0000_0110);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);

        let snapshot = mapper.registers_snapshot();
        mapper.write_prg(0x4100, 0b0000_0001);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 1);

        mapper.restore_registers(&snapshot);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);
    }
}
