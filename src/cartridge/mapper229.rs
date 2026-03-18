//! Mapper 229 – BMC 31-IN-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_229>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 229;

/// Mapper 229 – BMC 31-IN-1
///
/// Simple multicart mapper where any write to `$8000–$FFFF` sets both
/// PRG and CHR banking and mirroring from the write address.
///
/// ## Register map
///
/// Any write to `$8000–$FFFF` (mask `$803F`):
/// ```text
/// %1xxx xxxx xxHP PPPP
///                C CCCC
///  H = mirroring [0=vertical, 1=horizontal]
///  P,C = bank number for both PRG pages and CHR (same bits, addr[4:0])
/// ```
///
/// ## PRG banking (2×16 KB)
///
/// - If bank ≠ 0: both `$8000` and `$C000` windows map to the selected bank.
/// - If bank = 0: `$8000` maps to bank 0, `$C000` maps to bank 1.
///
/// ## CHR banking (1×8 KB)
///
/// CHR bank follows the same 5-bit field as PRG (addr bits 4–0).
///
/// ## Mirroring
///
/// Controlled by address bit A5 (0 = Vertical, 1 = Horizontal).
///
/// ## Power-on / Reset
///
/// Bank 0, CHR bank 0, `$C000` at bank 1, Vertical mirroring.
pub struct Mapper229 {
    base: BaseMapper,
}

impl Mapper229 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base };
        mapper.apply_register(0x8000);
        mapper
    }

    fn apply_register(&mut self, addr: u16) {
        let bank = (addr & 0x1F) as i16;
        let horizontal = addr & 0x20 != 0;

        self.base.select_prg_page(0, bank);
        // Special case: bank 0 → second window fixed to bank 1
        let upper_bank = if bank == 0 { 1 } else { bank };
        self.base.select_prg_page(1, upper_bank);
        self.base.select_chr_page(0, bank);
        self.base.set_mirroring_hv(horizontal);
    }
}

impl Mapper for Mapper229 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn reset(&mut self) {
        self.apply_register(0x8000);
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_register(addr);
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

    /// Use non-power-of-two bank count to prevent silent modulo wrapping.
    /// 24 × 16 KB = 384 KB PRG
    const PRG_BANKS: usize = 24;
    /// 24 × 8 KB CHR banks
    const CHR_BANKS: usize = 24;

    fn make_mapper(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper229 {
        Mapper229::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    fn make_default_mapper() -> Mapper229 {
        make_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        )
    }

    // ─── Factory registration ───────────────────────────────────────────────

    #[test]
    fn mapper_229_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 229 should be registered in factory");
    }

    // ─── Power-on state ─────────────────────────────────────────────────────

    #[test]
    fn power_on_lower_prg_window_at_bank_0() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should start at bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_upper_prg_window_at_bank_1() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should start at bank 1 (special case for bank=0)"
        );
    }

    #[test]
    fn power_on_chr_at_bank_0() {
        let mut mapper = make_default_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR should start at bank 0");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be Vertical on power-on"
        );
    }

    // ─── PRG banking – non-zero bank (both windows same) ─────────────────────

    #[test]
    fn non_zero_bank_both_prg_windows_map_to_same_bank() {
        let mut mapper = make_default_mapper();
        // bank = 5, H=0 → addr = 0x8000 | 5 = 0x8005
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should also be bank 5");
    }

    #[test]
    fn bank_3_both_prg_windows_at_bank_3() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8003, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    #[test]
    fn high_bank_both_windows_at_bank_23() {
        let mut mapper = make_default_mapper();
        // bank = 0x17 = 23; addr = 0x8000 | 0x17 = 0x8017
        // With PRG_BANKS=24 this is the highest bank we can select without wrap
        mapper.write_prg(0x8017, 0x00); // bank = 23 (0x17)
        assert_eq!(mapper.read_prg(0x8000), 23);
        assert_eq!(mapper.read_prg(0xC000), 23);
    }

    // ─── PRG banking – bank 0 special case ───────────────────────────────────

    #[test]
    fn bank_0_lower_window_at_0_upper_window_at_1() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // bank = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be bank 0 when bank=0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should be bank 1 (special case) when bank=0"
        );
    }

    #[test]
    fn bank_0_with_mirroring_bit_still_has_upper_at_bank_1() {
        let mut mapper = make_default_mapper();
        // bank=0, H=1 → addr = 0x8000 | 0x20 = 0x8020
        mapper.write_prg(0x8020, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 bank 0 with H=1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 bank 1 (special case) with H=1"
        );
    }

    // ─── CHR banking ─────────────────────────────────────────────────────────

    #[test]
    fn chr_follows_prg_bank_selection() {
        let mut mapper = make_default_mapper();
        // bank = 7 → addr = 0x8007
        mapper.write_prg(0x8007, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 7, "CHR bank should be 7");
    }

    #[test]
    fn chr_bank_0_on_bank_0_write() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // bank = 0
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 when bank=0");
    }

    #[test]
    fn chr_uses_lower_5_bits_of_address() {
        let mut mapper = make_default_mapper();
        // bank = 0x1F = 31 → wraps with 24 CHR banks to 31%24=7
        // Use bank 15 (0xF) as a non-wrapping test
        mapper.write_prg(0x800F, 0x00); // bank = 15
        assert_eq!(mapper.read_chr(0x0000), 15, "CHR bank 15");
    }

    // ─── Mirroring ───────────────────────────────────────────────────────────

    #[test]
    fn mirroring_addr_bit5_zero_selects_vertical() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // A5=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_addr_bit5_one_selects_horizontal() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8020, 0x00); // A5=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8020, 0x00); // horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0x00); // vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn write_to_upper_rom_region_also_applies_register() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xE005, 0x00); // bank=5 from high address
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    // ─── Reset ───────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8007, 0x00); // select bank 7
        assert_eq!(mapper.read_prg(0x8000), 7);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower window bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "upper window bank 1 after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "vertical mirroring after reset"
        );
    }

    // ─── Write data value is ignored ─────────────────────────────────────────

    #[test]
    fn write_data_value_is_ignored() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5);
        mapper.write_prg(0x8005, 0xFF); // different data, same address
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    // ─── Save state ──────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = make_mapper(prg.clone(), chr.clone());
        // Select bank 9, horizontal
        mapper.write_prg(0x8029, 0x00); // bank=9, H=1 (0x20 | 0x09)
        assert_eq!(mapper.read_prg(0x8000), 9);
        assert_eq!(mapper.read_prg(0xC000), 9);
        assert_eq!(mapper.read_chr(0x0000), 9);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg, chr);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 9, "lower PRG after restore");
        assert_eq!(restored.read_prg(0xC000), 9, "upper PRG after restore");
        assert_eq!(restored.read_chr(0x0000), 9, "CHR after restore");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring after restore"
        );
    }
}
