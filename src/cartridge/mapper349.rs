//! Mapper 349 - BMC G-146 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_349>
//! - Mesen reference: BmcG146
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 349 - BMC G-146 address-latch multicart.
///
/// Writes to $8000-$FFFF decode the 16-bit CPU address:
/// - A11=0,A6=0: 32KB mode via `(addr & 0x1E)` (consecutive 16KB banks)
/// - A11=0,A6=1: 16KB mirror mode via `(addr & 0x1F)` (same bank in both slots)
/// - A11=1: $8000-$BFFF from `(addr & 0x1F)`, $C000-$FFFF fixed to `(addr & 0x18) | 0x07`
/// - A7 selects mirroring (0=V, 1=H)
pub struct Mapper349 {
    base: BaseMapper,
    register_addr: u16,
}

impl Mapper349 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            has_irq: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_chr_page(0, 0);
        let mut mapper = Self {
            base,
            register_addr: 0x8000,
        };
        mapper.apply_register_addr(0x8000);
        mapper
    }

    fn apply_register_addr(&mut self, addr: u16) {
        self.register_addr = addr;
        if (addr & 0x0800) != 0 {
            let bank0 = (addr & 0x001F) as i16;
            let bank1 = ((addr & 0x0018) | 0x0007) as i16;
            self.base.select_prg_page(0, bank0);
            self.base.select_prg_page(1, bank1);
        } else if (addr & 0x0040) != 0 {
            let bank = (addr & 0x001F) as i16;
            self.base.select_prg_page(0, bank);
            self.base.select_prg_page(1, bank);
        } else {
            let bank = (addr & 0x001E) as i16;
            self.base.select_prg_page(0, bank);
            self.base.select_prg_page(1, bank + 1);
        }
        self.base.set_mirroring_hv((addr & 0x0080) != 0);
    }
}

impl Mapper for Mapper349 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_register_addr(addr);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            (self.register_addr & 0x00FF) as u8,
            ((self.register_addr >> 8) & 0x00FF) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let addr = u16::from(data[0]) | (u16::from(data[1]) << 8);
            self.apply_register_addr(addr);
        }
    }

    fn reset(&mut self) {
        self.base.select_chr_page(0, 0);
        self.apply_register_addr(0x8000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 48;
    const CHR_BANKS: usize = 3;

    fn make_mapper() -> Mapper349 {
        Mapper349::new(MapperContext::new_for_test(
            349,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_349_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            349,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 349 must be registered");
    }

    #[test]
    fn a11_clear_a6_clear_selects_32k_pair() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8012, 0);
        assert_eq!(mapper.read_prg(0x8000), 0x12 & 0x1E);
        assert_eq!(mapper.read_prg(0xC000), (0x12 & 0x1E) + 1);
    }

    #[test]
    fn a11_clear_a6_set_selects_mirrored_16k_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8045, 0);
        assert_eq!(mapper.read_prg(0x8000), 0x45 & 0x1F);
        assert_eq!(mapper.read_prg(0xC000), 0x45 & 0x1F);
    }

    #[test]
    fn a11_set_uses_split_low_and_fixed_high() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x88A2, 0);
        assert_eq!(mapper.read_prg(0x8000), 0xA2 & 0x1F);
        assert_eq!(mapper.read_prg(0xC000), (0xA2 & 0x18) | 0x07);
    }

    #[test]
    fn mirroring_comes_from_a7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8080, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn chr_is_fixed_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x88A2, 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    #[test]
    fn capabilities_report_no_irq_or_audio() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(!caps.has_chr_banking);
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_mapping() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x88A2, 0);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }

    #[test]
    fn reset_restores_power_on_prg_and_mirroring_state() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x88A2, 0);
        assert_ne!(mapper.read_prg(0x8000), 0);
        assert_ne!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
