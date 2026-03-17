//! Mapper 228 – Action Enterprises (Action 52 / Cheetahmen II)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_228>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/ActionEnterprises.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 228;

/// Mapper 228 – Action Enterprises (Action 52, Cheetahmen II)
///
/// Used by Active Enterprises for their multicart products.
/// PRG ROM consists of up to three 512 KB chips (chips 0, 1, 3; chip 2 = open bus).
///
/// ## Register map
///
/// Any write to `$8000–$FFFF`:
/// ```text
/// Address bits  FEDCBA98 76543210
///               1.MHHPPP PPS.CCCC
/// Data bits                      76543210
///                                ......CC
/// ```
/// - `A13` = M: Mirroring (0 = Vertical, 1 = Horizontal)
/// - `A12–A11` = HH: PRG chip select (chips 0, 1, 3 present; chip 2 = open bus)
/// - `A10–A6` = PPPPP: 16 KB PRG bank within selected chip
/// - `A5` = S: PRG bank size (0 = 32 KB mode, 1 = 16 KB mode)
/// - `A3–A0` = CCCC: High 4 bits of 8 KB CHR bank
/// - `D1–D0` = CC: Low 2 bits of 8 KB CHR bank
///
/// ## PRG banking (16 KB pages)
///
/// Combined page = `(chip_remap << 5) | ((addr >> 6) & 0x1F)`:
/// - Mode 0 (S=0, 32 KB): `$8000` = `page & 0xFE`, `$C000` = `(page & 0xFE) + 1`
/// - Mode 1 (S=1, 16 KB): `$8000` = `page`,         `$C000` = `page`
///
/// Chip remap: chip 3 → index 2 (linear ROM ordering: 0, 1, 3 → 0, 1, 2).
///
/// ## CHR banking (8 KB)
///
/// CHR bank = `((addr & 0x0F) << 2) | (value & 0x03)` (6-bit index).
///
/// ## Mirroring
///
/// Controlled by address bit A13 on each write.
///
/// ## Power-on / Reset
///
/// Bank 0, CHR bank 0, Vertical mirroring (emulated by writing 0 to $8000).
pub struct Mapper228 {
    base: BaseMapper,
}

impl Mapper228 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base };
        mapper.apply_register(0x8000, 0);
        mapper
    }

    fn apply_register(&mut self, addr: u16, value: u8) {
        let chip_select = (addr >> 11) & 0x03;
        // Chip 3 maps to linear index 2 (ROM layout: chips 0, 1, 3 → indices 0, 1, 2)
        let chip_remap = if chip_select == 3 { 2 } else { chip_select };

        let prg_page = ((addr >> 6) & 0x1F) | (chip_remap << 5);
        if addr & 0x20 != 0 {
            // 16 KB mode: both windows point to same bank
            self.base.select_prg_page(0, prg_page as i16);
            self.base.select_prg_page(1, prg_page as i16);
        } else {
            // 32 KB mode: consecutive pair
            self.base.select_prg_page(0, (prg_page & 0xFE) as i16);
            self.base.select_prg_page(1, ((prg_page & 0xFE) + 1) as i16);
        }

        let chr_bank = ((addr & 0x0F) << 2) | (u16::from(value) & 0x03);
        self.base.select_chr_page(0, chr_bank as i16);

        let horizontal = addr & 0x2000 != 0;
        self.base.set_mirroring_hv(horizontal);
    }
}

impl Mapper for Mapper228 {
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
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_register(addr, value);
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

    /// Use a non-power-of-two bank count to avoid silent modulo wrapping.
    /// 3 chips × 32 banks each = 96 16-KB banks = 1.5 MB
    const PRG_BANKS: usize = 96;
    /// 64 8-KB CHR banks
    const CHR_BANKS: usize = 64;

    fn make_mapper(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper228 {
        Mapper228::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    fn make_default_mapper() -> Mapper228 {
        make_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        )
    }

    // ─── Factory registration ───────────────────────────────────────────────

    #[test]
    fn mapper_228_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 228 should be registered in factory");
    }

    // ─── Power-on state ─────────────────────────────────────────────────────

    #[test]
    fn power_on_both_prg_windows_at_bank_0_and_1() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should start at bank 0 (32KB mode pair start)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should start at bank 1 (32KB mode pair end)"
        );
    }

    #[test]
    fn power_on_chr_at_bank_0() {
        let mut mapper = make_default_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should start at bank 0 on power-on"
        );
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

    // ─── PRG banking – 32 KB mode (S=0, addr bit A5=0) ─────────────────────

    #[test]
    fn prg_32kb_mode_lower_window_gets_even_bank() {
        let mut mapper = make_default_mapper();
        // chip 0, bank 3 (A10-A6 = 3 → addr bits = 3<<6 = 0xC0 in addr):
        // addr = 0x8000 | (3 << 6) = 0x80C0; S=0 (bit A5=0)
        // page = 3; 32KB mode: lower = 3 & 0xFE = 2, upper = 3
        mapper.write_prg(0x80C0, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 should be bank 2 (even)");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should be bank 3 (odd)");
    }

    #[test]
    fn prg_32kb_mode_selects_consecutive_pair() {
        let mut mapper = make_default_mapper();
        // chip 0, bank 5 (odd): lower = 4, upper = 5
        // addr = 0x8000 | (5 << 6) = 0x8140
        mapper.write_prg(0x8140, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5");
    }

    // ─── PRG banking – 16 KB mode (S=1, addr bit A5=1) ─────────────────────

    #[test]
    fn prg_16kb_mode_both_windows_select_same_bank() {
        let mut mapper = make_default_mapper();
        // addr = 0x8000 | (1 << 5) | (4 << 6) = 0x8000 | 0x20 | 0x100 = 0x8120
        // chip=0, prg_page=4, S=1 → both windows = bank 4
        mapper.write_prg(0x8120, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be bank 4");
        assert_eq!(mapper.read_prg(0xC000), 4, "$C000 should be bank 4");
    }

    #[test]
    fn prg_16kb_mode_odd_bank_same_in_both_windows() {
        let mut mapper = make_default_mapper();
        // bank 7, S=1: addr = 0x8000 | 0x20 | (7 << 6) = 0x81E0
        mapper.write_prg(0x81E0, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    // ─── PRG banking – chip select ───────────────────────────────────────────

    #[test]
    fn chip_select_1_offsets_prg_by_32_banks() {
        let mut mapper = make_default_mapper();
        // chip 1 (A12=1, A11=0): addr bits A12-A11 = 0b10 → (addr >> 11) & 3 = 2? No:
        // chip_select = (addr >> 11) & 0x03
        // chip 1: A12=0, A11=1 → bits = 0b01 → chip_select = 1
        // addr = 0x8000 | (1 << 11) | (0 << 6) | 0x20 (16KB mode for clarity)
        //      = 0x8000 | 0x0800 | 0x20 = 0x8820
        // chip_remap=1, page=0, S=1 → prg_page = 0 | (1<<5) = 32
        mapper.write_prg(0x8820, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "chip 1 bank 0 should map to PRG bank 32"
        );
        assert_eq!(mapper.read_prg(0xC000), 32);
    }

    #[test]
    fn chip_select_3_remaps_to_chip_index_2() {
        let mut mapper = make_default_mapper();
        // chip 3 (A12=1, A11=1): addr bits A12-A11 = 0b11 → chip_select = 3 → remap to 2
        // addr = 0x8000 | (3 << 11) | (0 << 6) | 0x20 = 0x8000 | 0x1800 | 0x20 = 0x9820
        // chip_remap=2, page=0, S=1 → prg_page = 0 | (2<<5) = 64
        mapper.write_prg(0x9820, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            64,
            "chip 3 (remapped to index 2) bank 0 should map to PRG bank 64"
        );
    }

    // ─── CHR banking ────────────────────────────────────────────────────────

    #[test]
    fn chr_high_4_bits_from_address_low_4_bits() {
        let mut mapper = make_default_mapper();
        // A3-A0 = 0b0101 = 5; D1-D0 = 0b00
        // CHR bank = (5 << 2) | 0 = 20
        // addr = 0x8000 | 5 = 0x8005 (with S=0, chip=0, prg=0)
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 20, "CHR bank 20: A3-A0=5, D1-D0=0");
    }

    #[test]
    fn chr_low_2_bits_from_data_value() {
        let mut mapper = make_default_mapper();
        // A3-A0 = 0, D1-D0 = 3
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank 3: A3-A0=0, D1-D0=3");
    }

    #[test]
    fn chr_combined_addr_and_data_bits() {
        let mut mapper = make_default_mapper();
        // A3-A0 = 0xF (= 15), D1-D0 = 2
        // CHR bank = (15 << 2) | 2 = 62
        mapper.write_prg(0x800F, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            62,
            "CHR bank 62: A3-A0=15, D1-D0=2"
        );
    }

    // ─── Mirroring ──────────────────────────────────────────────────────────

    #[test]
    fn mirroring_bit_a13_zero_selects_vertical() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 0x00); // A13=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit_a13_one_selects_horizontal() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xA000, 0x00); // A13=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ─── Save state ─────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = make_mapper(prg.clone(), chr.clone());
        // Set chip 1, bank 4, 16KB mode, CHR bank 7 (A3-A0=1, D1-D0=3), horizontal
        // addr = 0x8000 | (1<<11) | (4<<6) | 0x20 | 0x2000 | 1 = 0xAA21
        // chip_select = (0xAA21 >> 11) & 3 = (0x15) & 3 = 1
        // prg_page = (0xAA21 >> 6) & 0x1F = 0x28 & 0x1F = 8; with chip: 8 | (1<<5) = 40
        // S=1 → both windows = 40
        // CHR: (1 << 2) | 3 = 7
        // Mirroring: A13=1 → horizontal
        mapper.write_prg(0xAA21, 0x03);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg, chr);
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            40,
            "lower PRG bank after restore"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            40,
            "upper PRG bank after restore"
        );
        assert_eq!(restored.read_chr(0x0000), 7, "CHR bank after restore");
    }
}
