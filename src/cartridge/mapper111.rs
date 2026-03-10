//! Mapper 111 - GTROM
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/GTROM>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

pub struct GtromMapper {
    base: BaseMapper,
    register: u8,
    nametable_ram: [u8; Self::NAMETABLE_RAM_SIZE],
}

impl GtromMapper {
    const NAMETABLE_BANK_SIZE: usize = 0x0400;
    const NAMETABLE_RAM_SIZE: usize = 4 * Self::NAMETABLE_BANK_SIZE;

    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);

        let mut mapper = Self {
            base,
            register: 0,
            nametable_ram: [0; Self::NAMETABLE_RAM_SIZE],
        };
        mapper.apply_register();
        mapper
    }

    fn apply_register(&mut self) {
        let prg_bank = (self.register & 0x03) as i16;
        let chr_bank = ((self.register >> 5) & 0x01) as i16;

        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);
    }

    fn nametable_bank(&self) -> usize {
        ((self.register >> 6) & 0x03) as usize
    }
}

impl Mapper for GtromMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x5000..=0x5FFF).contains(&addr) {
            self.register = value;
            self.apply_register();
        }
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }

        let offset = (addr as usize) & (Self::NAMETABLE_BANK_SIZE - 1);
        let index = self.nametable_bank() * Self::NAMETABLE_BANK_SIZE + offset;
        Some(self.nametable_ram[index])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }

        let offset = (addr as usize) & (Self::NAMETABLE_BANK_SIZE - 1);
        let index = self.nametable_bank() * Self::NAMETABLE_BANK_SIZE + offset;
        self.nametable_ram[index] = value;
        true
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(1 + Self::NAMETABLE_RAM_SIZE);
        snapshot.push(self.register);
        snapshot.extend_from_slice(&self.nametable_ram);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        self.register = data[0];
        if data.len() > 1 {
            let ram_len = (data.len() - 1).min(Self::NAMETABLE_RAM_SIZE);
            self.nametable_ram[..ram_len].copy_from_slice(&data[1..1 + ram_len]);
        }
        self.apply_register();
    }

    fn reset(&mut self) {
        self.register = 0;
        self.apply_register();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::create_mapper;
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 3;
    const CHR_BANKS_8K: usize = 2;

    fn make_mapper() -> GtromMapper {
        GtromMapper::new(MapperContext::new_for_test(
            111,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_111_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            111,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 111 must be registered");
    }

    #[test]
    fn prg_bank_switches_with_register_bits_0_to_1() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x02);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xFFFF), 2);
    }

    #[test]
    fn chr_bank_switches_with_register_bit_5() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x20);

        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x1FFF), 1);
    }

    #[test]
    fn nametable_bank_switches_with_register_bits_6_to_7() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x00);
        assert!(mapper.write_nametable(0x2000, 0x11));

        mapper.write_prg(0x5000, 0xC0);
        assert!(mapper.write_nametable(0x2000, 0x33));

        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x11));

        mapper.write_prg(0x5000, 0xC0);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x33));
    }

    #[test]
    fn registers_snapshot_restore_round_trips_selected_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0xE1);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert!(restored.write_nametable(0x2000, 0x5A));
        assert_eq!(restored.read_nametable(0x2000), Some(0x5A));
    }
}
