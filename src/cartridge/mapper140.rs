//! Mapper 140 - Jaleco JF-11/JF-14
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 140 - Jaleco JF-11/JF-14
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_140>
/// - GNROM-like register format: `[..PP CCCC]`
/// - Register write port: `$6000-$7FFF`
/// - PRG: 32KB bank at `$8000-$FFFF` selected by bits 4-5
/// - CHR: 8KB bank at `$0000-$1FFF` selected by bits 0-3
/// - No PRG-RAM (registers occupy `$6000-$7FFF`)
/// - Mirroring: fixed from iNES header
pub struct Mapper140 {
    base: BaseMapper,
    register: u8,
}

impl Mapper140 {
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

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.base.select_prg_page(0, ((value >> 4) & 0b11) as i16);
        self.base.select_chr_page(0, (value & 0b1111) as i16);
    }
}

impl Mapper for Mapper140 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.apply_register(value);
        }
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

    fn create_mapper140(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            140,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn test_mapper140_register_at_6000_7fff_selects_prg_and_chr() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 16);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x6000, 0x12); // PRG=1, CHR=2
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.write_prg(0x7FFF, 0x3F); // PRG=3, CHR=15
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_chr(0x0000), 15);
    }

    #[test]
    fn test_mapper140_ignores_writes_outside_6000_7fff() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x8000, 0x31);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn test_mapper140_has_no_prg_ram() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper =
            create_mapper140(prg_rom, chr_rom).expect("mapper 140 should be implemented");

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0);
        assert_eq!(mapper.wram_size(), 0);
        assert!(mapper.wram_snapshot().is_empty());
    }
}
