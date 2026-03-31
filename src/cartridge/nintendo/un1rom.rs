//! Mapper 094 - HVC-UN1ROM
//!
//! Specifications:
//! - NesDev wiki: <https://www.nesdev.org/wiki/UN1ROM>
//! - Mesen2 mapper implementation: <https://github.com/SourMesen/Mesen2/blob/master/Core/Database/Mappers/Mapper094.cpp>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

pub struct Un1romMapper {
    base: BaseMapper,
    prg_bank: u8,
}

impl Un1romMapper {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let chr_seed = ctx.chr_rom.clone();
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -1);
        base.set_bus_conflicts(true);
        base.set_mirroring(NametableLayout::Vertical);

        // Mapper 94 is CHR-RAM-only. If a dump provides CHR bytes, treat them
        // as initial CHR-RAM contents for compatibility with bad dumps.
        let mut chr_ram = ChrMemory::new_ram(8 * 1024);
        if !chr_seed.is_empty() {
            chr_ram.load_snapshot(&chr_seed);
        }
        base.set_chr_memory(chr_ram);

        Self { base, prg_bank: 0 }
    }
}

impl Mapper for Un1romMapper {
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
            let effective = self.base.apply_bus_conflict(addr, value);
            self.prg_bank = (effective >> 2) & 0x0F;
            self.base.select_prg_page(0, self.prg_bank as i16);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.prg_bank = bank;
            self.base.select_prg_page(0, self.prg_bank as i16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    const PRG_BANK_SIZE: usize = 16 * 1024;
    const PRG_BANKS: usize = 13;

    fn make_prg_rom() -> Vec<u8> {
        let mut prg = vec![0xFF; PRG_BANK_SIZE * PRG_BANKS];
        for bank in 0..PRG_BANKS {
            let offset = bank * PRG_BANK_SIZE;
            prg[offset + 0x123] = bank as u8;
        }
        prg
    }

    fn read_prg_bank(mapper: &Un1romMapper, base: u16) -> u8 {
        mapper.read_prg(base + 0x123)
    }

    fn make_mapper() -> Un1romMapper {
        Un1romMapper::new(MapperContext::new_for_test(
            94,
            make_prg_rom(),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_94_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            94,
            make_prg_rom(),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 94 must be registered in the factory"
        );
    }

    #[test]
    fn prg_bank_selection_uses_bits_5_2_only() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x04);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 1);

        mapper.write_prg(0x8000, 0x08);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 2);

        mapper.write_prg(0x8000, 0x88);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 2);

        mapper.write_prg(0x8000, 0xCB);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 2);
    }

    #[test]
    fn upper_16k_window_is_fixed_to_last_prg_bank() {
        let mut mapper = make_mapper();
        let last = (PRG_BANKS - 1) as u8;
        assert_eq!(read_prg_bank(&mapper, 0xC000), last);

        mapper.write_prg(0x8000, 0x04);
        mapper.write_prg(0x9000, 0x18);
        mapper.write_prg(0xFFFF, 0x3C);

        assert_eq!(read_prg_bank(&mapper, 0xC000), last);
    }

    #[test]
    fn chr_ram_round_trip() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn chr_rom_payload_is_treated_as_seeded_chr_ram() {
        let mut chr_seed = vec![0; 8 * 1024];
        chr_seed[0x0100] = 0x3A;
        let mut mapper = Un1romMapper::new(MapperContext::new_for_test(
            94,
            make_prg_rom(),
            chr_seed,
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.read_chr(0x0100), 0x3A);
        mapper.write_chr(0x0100, 0xC7);
        assert_eq!(mapper.read_chr(0x0100), 0xC7);
    }

    #[test]
    fn bus_conflicts_apply_before_bank_selection() {
        let mut prg = make_prg_rom();
        prg[0] = 0x14;
        let mut mapper = Un1romMapper::new(MapperContext::new_for_test(
            94,
            prg,
            vec![],
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x8000, 0x3C);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 5);
    }

    #[test]
    fn bus_conflicts_can_mask_bank_bits_to_zero() {
        let mut prg = make_prg_rom();
        prg[0] = 0x03;
        let mut mapper = Un1romMapper::new(MapperContext::new_for_test(
            94,
            prg,
            vec![],
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x8000, 0x3C);
        assert_eq!(read_prg_bank(&mapper, 0x8000), 0);
    }

    #[test]
    fn mirroring_is_hardwired_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
