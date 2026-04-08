use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 104 - Pegasus 5-in-1 / Golden Five
///
/// Spec: <https://www.nesdev.org/wiki/INES_Mapper_104>
pub struct Mapper104 {
    base: BaseMapper,
    register: u8,
}

impl Mapper104 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base, register: 0 };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.register as i16);
        self.base
            .select_prg_page(1, ((self.register & 0x70) | 0x0F) as i16);
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
        if addr >= 0xC000 {
            self.register = (self.register & 0x70) | (value & 0x0F);
            self.update_banks();
        } else if addr >= 0x8000 && (value & 0x08) != 0 {
            self.register = (self.register & 0x0F) | ((value << 4) & 0x70);
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.register = value & 0x7F;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.register = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper104;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 80;

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
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 104 should be supported");
    }

    #[test]
    fn write_c000_ffff_selects_lower_prg_nibble() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        mapper.write_prg(0xC000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn write_8000_9fff_with_bit3_sets_upper_prg_bits_and_repoints_c000() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        mapper.write_prg(0xC000, 0x0A);
        mapper.write_prg(0x8000, 0x0B); // upper bits become 0x30
        assert_eq!(mapper.read_prg(0x8000), 58);
        assert_eq!(mapper.read_prg(0xC000), 63);
    }

    #[test]
    fn prg_bank_index_above_15_is_reachable() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        mapper.write_prg(0xC000, 0x0F);
        mapper.write_prg(0x8000, 0x09); // upper bits 0x10, total bank 0x1F (31)
        assert_eq!(mapper.read_prg(0x8000), 31);
        assert_eq!(mapper.read_prg(0xC000), 31);
    }

    #[test]
    fn chr_is_chr_ram_and_not_bank_switched() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        mapper.write_chr(0x0010, 0xAB);
        mapper.write_prg(0xFFFF, 0x07);
        mapper.write_prg(0x8000, 0x0F);
        assert_eq!(mapper.read_chr(0x0010), 0xAB);
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xC000, 0x0F);
        mapper.write_prg(0x8000, 0x0B);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn reset_restores_power_on_prg_state() {
        let mut mapper = create_mapper104(banked_data(16 * 1024, PRG_BANKS), vec![]);
        mapper.write_prg(0xC000, 0x0A);
        mapper.write_prg(0x8000, 0x0B);
        assert_eq!(mapper.read_prg(0x8000), 58);
        assert_eq!(mapper.read_prg(0xC000), 63);

        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 15);
    }
}
