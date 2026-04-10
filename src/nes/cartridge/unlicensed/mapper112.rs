//! Mapper 112 - NTDEC multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_112>
//! - Mesen2 reference: `Core/NES/Mappers/Ntdec/Mapper112.h`
//!
//! Known Limitations:
//! - No IRQ support (matching mapper hardware).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 112 - NTDEC multicart
pub struct Mapper112 {
    base: BaseMapper,
    current_register: u8,
    outer_chr_bank: u8,
    registers: [u8; 8],
}

impl Mapper112 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;
    const SNAPSHOT_SIZE: usize = 11;

    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            current_register: 0,
            outer_chr_bank: 0,
            registers: [0; 8],
        };
        mapper.set_power_on_state();
        mapper
    }

    fn update_state(&mut self) {
        self.base.select_prg_page(0, self.registers[0] as i16);
        self.base.select_prg_page(1, self.registers[1] as i16);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        let chr_2k_0 = (self.registers[2] as i16) * 2;
        self.base.select_chr_page(0, chr_2k_0);
        self.base.select_chr_page(1, chr_2k_0 + 1);

        let chr_2k_1 = (self.registers[3] as i16) * 2;
        self.base.select_chr_page(2, chr_2k_1);
        self.base.select_chr_page(3, chr_2k_1 + 1);

        self.base.select_chr_page(
            4,
            (self.registers[4] as u16 | (((self.outer_chr_bank as u16) & 0x10) << 4)) as i16,
        );
        self.base.select_chr_page(
            5,
            (self.registers[5] as u16 | (((self.outer_chr_bank as u16) & 0x20) << 3)) as i16,
        );
        self.base.select_chr_page(
            6,
            (self.registers[6] as u16 | (((self.outer_chr_bank as u16) & 0x40) << 2)) as i16,
        );
        self.base.select_chr_page(
            7,
            (self.registers[7] as u16 | (((self.outer_chr_bank as u16) & 0x80) << 1)) as i16,
        );
    }

    fn set_power_on_state(&mut self) {
        self.current_register = 0;
        self.outer_chr_bank = 0;
        self.registers = [0; 8];
        self.base.set_mirroring(NametableLayout::Vertical);
        self.update_state();
    }
}

impl Mapper for Mapper112 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if let 0x8000..=0xFFFF = addr {
            match addr & 0xE001 {
                0x8000 => self.current_register = value & 0x07,
                0xA000 => self.registers[self.current_register as usize] = value,
                0xC000 => self.outer_chr_bank = value,
                0xE000 => {
                    self.base.set_mirroring(if (value & 0x01) != 0 {
                        NametableLayout::Horizontal
                    } else {
                        NametableLayout::Vertical
                    });
                }
                _ => {}
            }
        }
        self.update_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::SNAPSHOT_SIZE);
        data.push(self.base.mirroring().to_snapshot_byte());
        data.push(self.current_register);
        data.push(self.outer_chr_bank);
        data.extend_from_slice(&self.registers);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < Self::SNAPSHOT_SIZE {
            return;
        }
        self.base
            .set_mirroring(NametableLayout::from_snapshot_byte(data[0]));
        self.current_register = data[1];
        self.outer_chr_bank = data[2];
        self.registers.copy_from_slice(&data[3..11]);
        self.update_state();
    }

    fn reset(&mut self) {
        self.set_power_on_state();
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper112;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const TEST_PRG_BANKS_8K: usize = 11;
    const TEST_CHR_BANKS_1K: usize = 48;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            112,
            banked_data(8 * 1024, TEST_PRG_BANKS_8K),
            banked_data(1024, TEST_CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 112 should be implemented")
    }

    fn make_mapper_impl() -> Mapper112 {
        Mapper112::new(MapperContext::new_for_test(
            112,
            banked_data(8 * 1024, TEST_PRG_BANKS_8K),
            banked_data(1024, TEST_CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_112_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            112,
            banked_data(8 * 1024, TEST_PRG_BANKS_8K),
            banked_data(1024, TEST_CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok(), "Mapper 112 should be registered in factory");
    }

    #[test]
    fn prg_8000_and_a000_are_switchable_while_c000_e000_are_fixed() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0x8000, 1);
        mapper.write_prg(0xA000, 5);

        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), (TEST_PRG_BANKS_8K - 2) as u8);
        assert_eq!(mapper.read_prg(0xE000), (TEST_PRG_BANKS_8K - 1) as u8);
    }

    #[test]
    fn chr_2k_and_1k_registers_and_outer_chr_bits_control_expected_windows() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 4);
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 7);
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0xA000, 8);
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 9);
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0xA000, 10);
        mapper.write_prg(0x8000, 7);
        mapper.write_prg(0xA000, 11);
        mapper.write_prg(0xC000, 0xF0);

        assert_eq!(mapper.read_chr(0x0000), 8);
        assert_eq!(mapper.read_chr(0x0400), 9);
        assert_eq!(mapper.read_chr(0x0800), 14);
        assert_eq!(mapper.read_chr(0x0C00), 15);
        assert_eq!(mapper.read_chr(0x1000), 24);
        assert_eq!(mapper.read_chr(0x1400), 25);
        assert_eq!(mapper.read_chr(0x1800), 26);
        assert_eq!(mapper.read_chr(0x1C00), 27);
    }

    #[test]
    fn mirroring_is_controlled_by_e000_bit0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xE000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn writes_below_0x8000_are_ignored() {
        let mut mapper = make_mapper_impl();
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0xA000, 3);

        let before_prg = mapper.read_prg(0x8000);
        let before_current = mapper.current_register;
        let before_outer = mapper.outer_chr_bank;
        let before_registers = mapper.registers;

        mapper.write_prg(0x4020, 1);
        mapper.write_prg(0x5FFF, 7);

        assert_eq!(mapper.read_prg(0x8000), before_prg);
        assert_eq!(mapper.current_register, before_current);
        assert_eq!(mapper.outer_chr_bank, before_outer);
        assert_eq!(mapper.registers, before_registers);
    }
}
