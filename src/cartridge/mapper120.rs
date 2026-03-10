use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper120 {
    base: BaseMapper,
    chr_bank: u8,
}

impl Mapper120 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, chr_bank: 0 };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper120 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        120
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr == 0x41FF {
            self.chr_bank = value;
            self.apply_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.chr_bank = value;
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.chr_bank = 0;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    fn make_mapper() -> Mapper120 {
        let prg_rom = [vec![0x11; PRG_BANK_SIZE], vec![0x22; PRG_BANK_SIZE]].concat();
        let chr_rom = [vec![0xAA; CHR_BANK_SIZE], vec![0x55; CHR_BANK_SIZE]].concat();
        Mapper120::new(MapperContext::new_for_test(
            120,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_120_is_registered_in_factory() {
        let prg_rom = vec![0x11; PRG_BANK_SIZE];
        let chr_rom = vec![0xAA; CHR_BANK_SIZE];
        let result = create_mapper(MapperContext::new_for_test(
            120,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "mapper 120 must be creatable by factory");
    }

    #[test]
    fn write_41ff_switches_chr_8kb_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x41FF, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        mapper.write_prg(0x41FF, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 0x55);
    }

    #[test]
    fn writes_outside_41ff_do_not_change_fixed_prg_window() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0x11);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0x11);
    }
}
