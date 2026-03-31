//! Mapper 054 - Novel Diamond
//!
//! Specifications:
//! - Reference: Mesen2 `NovelDiamond.h` (mapper 54)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 054 - Novel Diamond (unlicensed)
///
/// Hardware: Novel Diamond board
///
/// Specifications:
/// - Reference: Mesen2 `Core/NES/Mappers/Unlicensed/NovelDiamond.h`
/// - PRG-ROM: Up to 128 KiB (4 × 32 KiB banks)
/// - CHR-ROM: Up to 64 KiB (8 × 8 KiB banks)
/// - Mirroring: Fixed from header
///
/// Register ($8000-$FFFF, write):
/// - PRG 32 KiB bank: bits 1:0 of the **write address**
/// - CHR 8 KiB bank:  bits 2:0 of the **write address**
///
/// The written **value** is ignored; bank selection is entirely address-driven.
pub struct Mapper54 {
    base: BaseMapper,
}

impl Mapper54 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_prg_page(0, 0);
        base.select_chr_page(0, 0);
        Self { base }
    }
}

impl Mapper for Mapper54 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.base.select_prg_page(0, (addr & 0x03) as i16);
            self.base.select_chr_page(0, (addr & 0x07) as i16);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.base.banking_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Use 5 PRG banks (non-power-of-two) to avoid modulo-wrapping false-passes.
    const PRG_BANKS: usize = 5;
    /// Use 9 CHR banks (non-power-of-two) to avoid modulo-wrapping false-passes.
    const CHR_BANKS: usize = 9;

    fn make_mapper() -> Mapper54 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper54::new(MapperContext::new_for_test(
            54,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_54_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            54,
            banked_data(32 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 54 must be registered");
    }

    #[test]
    fn power_on_maps_prg_bank_0() {
        let mapper = make_mapper();
        // banked_data fills each 32 KiB bank N with the byte value N.
        // Bank 0 → first byte of $8000 window = 0.
        assert_eq!(mapper.read_prg(0x8000), 0, "Power-on PRG bank must be 0");
    }

    #[test]
    fn power_on_maps_chr_bank_0() {
        let mut mapper = make_mapper();
        // banked_data fills each 8 KiB CHR bank N with the byte value N.
        // Bank 0 → first byte of pattern table = 0.
        assert_eq!(mapper.read_chr(0x0000), 0, "Power-on CHR bank must be 0");
    }

    #[test]
    fn prg_bank_selected_by_write_address_bits_1_0() {
        let mut mapper = make_mapper();
        // addr & 0x03 selects the 32 KiB PRG bank; value is ignored.
        // Use address $8003 → PRG bank 3.
        mapper.write_prg(0x8003, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank must be addr & 0x03 = 3"
        );

        // Switch to addr $8001 → PRG bank 1.
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank must be addr & 0x03 = 1"
        );
    }

    #[test]
    fn prg_bank_2_selected_by_address() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank must be addr & 0x03 = 2"
        );
    }

    #[test]
    fn chr_bank_selected_by_write_address_bits_2_0() {
        let mut mapper = make_mapper();
        // addr & 0x07 selects the 8 KiB CHR bank.
        // Use $8007 → CHR bank 7.
        mapper.write_prg(0x8007, 0x00);
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "CHR bank must be addr & 0x07 = 7"
        );

        // Switch to $8004 → CHR bank 4.
        mapper.write_prg(0x8004, 0x00);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR bank must be addr & 0x07 = 4"
        );
    }

    #[test]
    fn prg_and_chr_banks_both_update_from_same_write_address() {
        let mut mapper = make_mapper();
        // Both banks update on every write; each uses a different bit mask.
        // addr $8002 → PRG bank = 2 & 0x03 = 2, CHR bank = 2 & 0x07 = 2.
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank 2");
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank also 2 (addr & 0x07)");

        // addr $8005 → PRG bank = 5 & 0x03 = 1, CHR bank = 5 & 0x07 = 5.
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank = 5 & 0x03 = 1");
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR bank = 5 & 0x07 = 5");
    }

    #[test]
    fn written_value_does_not_affect_bank_selection() {
        let mut mapper = make_mapper();
        // Any value written to addr $8003 must still select PRG bank 3.
        mapper.write_prg(0x8003, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "Value 0xFF must be ignored");

        mapper.write_prg(0x8003, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 3, "Value 0x00 must be ignored");
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mut mapper = make_mapper(); // created with Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        // Writing must not change mirroring.
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must remain fixed"
        );
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0x00); // PRG bank 3, CHR bank 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG bank must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR bank must match"
        );
    }
}
