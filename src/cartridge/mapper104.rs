use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 104 - Pegasus 5-in-1
///
/// Spec: <https://www.nesdev.org/wiki/INES_Mapper_104>
pub struct Mapper104 {
    base: BaseMapper,
    register: u8,
}

impl Mapper104 {
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
        self.base.select_prg_page(0, ((value >> 4) & 0x0F) as i16);
        self.base.select_chr_page(0, (value & 0x0F) as i16);
    }
}

impl Mapper for Mapper104 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
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
    use super::Mapper104;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper104(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper104 {
        Mapper104::new(MapperContext::new_for_test(
            104,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_104_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            104,
            banked_data(32 * 1024, 3),
            banked_data(8 * 1024, 5),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 104 should be supported");
    }

    #[test]
    fn write_8000_ffff_selects_32kb_prg_bank() {
        let mut mapper = create_mapper104(banked_data(32 * 1024, 3), banked_data(8 * 1024, 5));
        mapper.write_prg(0x8000, 0x10); // PRG=1, CHR=0
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xFFFF), 1);
    }

    #[test]
    fn write_8000_ffff_selects_8kb_chr_bank() {
        let mut mapper = create_mapper104(banked_data(32 * 1024, 3), banked_data(8 * 1024, 5));
        mapper.write_prg(0xFFFF, 0x02); // PRG=0, CHR=2
        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = create_mapper104(banked_data(32 * 1024, 3), banked_data(8 * 1024, 5));
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x3F);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
