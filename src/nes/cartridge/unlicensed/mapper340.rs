//! Mapper 340 - BMC-K-3036 (35-in-1 multicart)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_340>
//! - Reference behavior: FCEUmm `bmck3036.c`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 340;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const PRG_WRITE_START: u16 = 0x8000;
const OUTER_PRG_MASK: u8 = 0x1F;
const INNER_PRG_MASK: u8 = 0x07;
const MODE_NROM_128_BIT: u16 = 0x20;
const HORIZONTAL_MIRROR_BITS: u16 = 0x25;
const UNROM_FIXED_INNER_BANK: i16 = 0x07;
const REGISTERS_SNAPSHOT_LEN: usize = 4;

pub struct Mapper340 {
    base: BaseMapper,
    outer_prg_bank: u8,
    inner_prg_bank: u8,
    nrom_128_mode: bool,
    mirroring_horizontal: bool,
}

impl Mapper340 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let chr_seed = ctx.chr_rom.clone();
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut chr_ram = ChrMemory::new_ram(CHR_BANK_SIZE_BYTES);
        if !chr_seed.is_empty() {
            chr_ram.load_snapshot(&chr_seed);
        }
        base.set_chr_memory(chr_ram);

        let mut mapper = Self {
            base,
            outer_prg_bank: 0,
            inner_prg_bank: 0,
            nrom_128_mode: false,
            mirroring_horizontal: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let bank_8000 = self.prg_bank_8000();
        let bank_c000 = self.prg_bank_c000();

        self.base.select_prg_page(0, bank_8000);
        self.base.select_prg_page(1, bank_c000);

        self.base.select_chr_page(0, 0);
        self.base.set_mirroring_hv(self.mirroring_horizontal);
    }

    fn prg_bank_8000(&self) -> i16 {
        let outer = self.outer_prg_bank as i16;
        if self.nrom_128_mode {
            outer
        } else {
            outer | self.inner_prg_bank as i16
        }
    }

    fn prg_bank_c000(&self) -> i16 {
        let outer = self.outer_prg_bank as i16;
        if self.nrom_128_mode {
            outer
        } else {
            outer | UNROM_FIXED_INNER_BANK
        }
    }

    fn apply_state(
        &mut self,
        outer_prg_bank: u8,
        inner_prg_bank: u8,
        nrom_128_mode: bool,
        mirroring_horizontal: bool,
    ) {
        self.outer_prg_bank = outer_prg_bank;
        self.inner_prg_bank = inner_prg_bank;
        self.nrom_128_mode = nrom_128_mode;
        self.mirroring_horizontal = mirroring_horizontal;
        self.update_banks();
    }

    fn decode_mirroring_horizontal(addr: u16) -> bool {
        (addr & HORIZONTAL_MIRROR_BITS) == HORIZONTAL_MIRROR_BITS
    }
}

impl Mapper for Mapper340 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < PRG_WRITE_START {
            return;
        }

        self.apply_state(
            (addr as u8) & OUTER_PRG_MASK,
            value & INNER_PRG_MASK,
            (addr & MODE_NROM_128_BIT) != 0,
            Self::decode_mirroring_horizontal(addr),
        );
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.outer_prg_bank,
            self.inner_prg_bank,
            self.nrom_128_mode as u8,
            self.mirroring_horizontal as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }

        self.apply_state(
            data[0] & OUTER_PRG_MASK,
            data[1] & INNER_PRG_MASK,
            data[2] != 0,
            data[3] != 0,
        );
    }

    fn reset(&mut self) {
        self.apply_state(0, 0, false, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_16K: usize = 48;

    fn make_mapper() -> Mapper340 {
        Mapper340::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_340_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 340 must be registered in factory");
    }

    #[test]
    fn unrom_mode_uses_outer_or_inner_for_8000_and_outer_or_7_for_c000() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8018, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 27);
        assert_eq!(mapper.read_prg(0xC000), 31);
    }

    #[test]
    fn nrom_128_mode_mirrors_outer_bank_in_both_windows() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8025, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    #[test]
    fn horizontal_mirroring_requires_exact_a5_a2_a0_pattern() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8025, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn chr_ram_is_unbanked_and_writable() {
        let mut mapper = make_mapper();

        mapper.write_chr(0x0123, 0x5A);
        assert_eq!(mapper.read_chr(0x0123), 0x5A);
    }

    #[test]
    fn snapshot_restore_preserves_mode_bank_and_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8025, 0x04);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_prg(0xC000), 5);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn capabilities_match_k3036_baseline() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();

        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(!caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
