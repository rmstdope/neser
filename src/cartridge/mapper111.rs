//! Mapper 111 - GTROM
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/GTROM>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
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
        let chr_ram_size = ctx.chr_rom.len().max(8 * 1024);
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.set_chr_memory(ChrMemory::new_ram(chr_ram_size));
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

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.base.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.nametable_ram, mode);
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
    use crate::console::RamInitMode;

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

        mapper.write_prg(0x5000, 0x00);
        mapper.write_chr(0x0010, 0x12);
        mapper.write_prg(0x5000, 0x20);
        mapper.write_chr(0x0010, 0x34);

        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_chr(0x0010), 0x12);
        mapper.write_prg(0x5000, 0x20);
        assert_eq!(mapper.read_chr(0x0010), 0x34);
    }

    #[test]
    fn nametable_bank_switches_with_register_bits_6_to_7() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x00);
        assert!(mapper.write_nametable(0x2000, 0x10));
        mapper.write_prg(0x5000, 0x40);
        assert!(mapper.write_nametable(0x2000, 0x20));
        mapper.write_prg(0x5000, 0x80);
        assert!(mapper.write_nametable(0x2000, 0x30));
        mapper.write_prg(0x5000, 0xC0);
        assert!(mapper.write_nametable(0x2000, 0x40));

        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x10));
        mapper.write_prg(0x5000, 0x40);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x20));
        mapper.write_prg(0x5000, 0x80);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x30));
        mapper.write_prg(0x5000, 0xC0);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x40));
    }

    #[test]
    fn chr_ram_is_writable_even_when_chr_rom_is_present_in_input() {
        let mut mapper = make_mapper();

        mapper.write_chr(0x0010, 0x5A);
        assert_eq!(mapper.read_chr(0x0010), 0x5A);
    }

    #[test]
    fn registers_snapshot_restore_round_trips_selected_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0xE1);
        assert!(mapper.write_nametable(0x2000, 0x5A));
        assert_eq!(mapper.read_nametable(0x2000), Some(0x5A));
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.read_nametable(0x2000), Some(0x5A));
    }

    #[test]
    fn reset_restores_power_on_register_state() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x22);
        assert_eq!(mapper.read_prg(0x8000), 2);
        mapper.write_prg(0x5000, 0x02);
        mapper.write_chr(0x0020, 0x21);
        mapper.write_prg(0x5000, 0x22);
        mapper.write_chr(0x0020, 0x43);
        assert_eq!(mapper.read_chr(0x0020), 0x43);

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0020), 0x21);
    }

    #[test]
    fn initialize_ram_zero_clears_mapper_owned_nametable_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0xC0);
        assert!(mapper.write_nametable(0x2000, 0xAB));
        mapper.initialize_ram(RamInitMode::Zero);

        assert_eq!(mapper.read_nametable(0x2000), Some(0x00));
    }
}
