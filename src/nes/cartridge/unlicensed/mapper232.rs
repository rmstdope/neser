//! Mapper 232 - Camerica/Codemasters "Quattro" (BF9096)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_232>
//! - Fallback: Mesen2 `Core/NES/Mappers/Codemasters/BF9096.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 232;

/// Mapper 232 - Camerica/Codemasters "Quattro" multicart (BF9096)
///
/// Used by Quattro Adventure, Quattro Sports, Quattro Arcade, and the
/// Aladdin Deck Enhancer (submapper 1).
///
/// ## Register map
///
/// - `$8000–$BFFF`: `[...B B...]` — 2-bit PRG block select
///   - Standard (submapper 0): `prg_block = (value >> 3) & 0x03`
///   - Aladdin Deck Enhancer (submapper 1): bits are swapped:
///     `prg_block = ((value >> 4) & 0x01) | ((value >> 2) & 0x02)`
/// - `$C000–$FFFF`: `[.... ..PP]` — 2-bit PRG page select within block
///   - `prg_page = value & 0x03`
///
/// ## PRG banking (16 KB banks)
///
/// - `$8000–$BFFF` → bank `(prg_block << 2) | prg_page`
/// - `$C000–$FFFF` → bank `(prg_block << 2) | 3`  (fixed last page of block)
///
/// ## CHR
///
/// 8 KB CHR-RAM; no banking.
///
/// ## Mirroring
///
/// Fixed from iNES header; no mapper-controlled mirroring.
///
/// ## Power-on / Reset
///
/// `prg_block = 0`, `prg_page = 0`:
/// - `$8000–$BFFF` = bank 0
/// - `$C000–$FFFF` = bank 3
pub struct Mapper232 {
    base: BaseMapper,
    /// 2-bit PRG block (outer bank), selected via $8000–$BFFF
    prg_block: u8,
    /// 2-bit PRG page within block, selected via $C000–$FFFF
    prg_page: u8,
    /// NES 2.0 submapper ID (0 = standard, 1 = Aladdin Deck Enhancer)
    submapper: u8,
}

impl Mapper232 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        let mut mapper = Self {
            base,
            prg_block: 0,
            prg_page: 0,
            submapper,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let lo = ((self.prg_block << 2) | self.prg_page) as i16;
        let hi = ((self.prg_block << 2) | 3) as i16;
        self.base.select_prg_page(0, lo);
        self.base.select_prg_page(1, hi);
    }
}

impl Mapper for Mapper232 {
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
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0xC000 {
            self.prg_page = value & 0x03;
            self.apply_banks();
        } else if addr >= 0x8000 {
            self.prg_block = if self.submapper == 1 {
                // Aladdin Deck Enhancer: swap outer-bank bits
                ((value >> 4) & 0x01) | ((value >> 2) & 0x02)
            } else {
                (value >> 3) & 0x03
            };
            self.apply_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.prg_block);
        snap.push(self.prg_page);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected_banking_len = self.base.banking_snapshot().len();
        if data.len() >= expected_banking_len + 2 {
            self.base.restore_banking(&data[..expected_banking_len]);
            self.prg_block = data[expected_banking_len];
            self.prg_page = data[expected_banking_len + 1];
            self.apply_banks();
        } else {
            self.base.restore_banking(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Use 16 16-KB banks (256 KB) to cover all 4 blocks × 4 pages.
    const PRG_BANKS: usize = 16;

    fn make_mapper(prg_rom: Vec<u8>) -> Mapper232 {
        Mapper232::new(
            MapperContext::new_for_test(MAPPER_NUMBER, prg_rom, vec![], NametableLayout::Vertical)
                .with_prg_ram_banks(0),
        )
    }

    fn make_mapper_submapper1(prg_rom: Vec<u8>) -> Mapper232 {
        Mapper232::new(
            MapperContext::new_for_test(MAPPER_NUMBER, prg_rom, vec![], NametableLayout::Vertical)
                .with_prg_ram_banks(0)
                .with_submapper(1),
        )
    }

    // ───────── Factory registration ─────────

    #[test]
    fn mapper_232_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 232 should be registered in factory");
    }

    // ───────── Power-on state ─────────

    #[test]
    fn power_on_lower_window_at_bank_0() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 window should start at bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_upper_window_at_bank_3() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 window should start at bank 3 (last page of block 0) on power-on"
        );
    }

    // ───────── PRG page select ($C000–$FFFF) ─────────

    #[test]
    fn page_select_sets_lower_window_within_block() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // block = 0 (default), page = 2 → lower = (0<<2)|2 = 2
        mapper.write_prg(0xC000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2, "lower window should be bank 2");
        // upper is always last page of block: (0<<2)|3 = 3
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "upper window should remain bank 3"
        );
    }

    #[test]
    fn page_select_only_uses_low_two_bits() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Write 0xFF — only bits 1-0 matter → page = 3
        mapper.write_prg(0xC000, 0xFF);
        // block=0 → lower = 3, upper = 3
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    // ───────── PRG block select ($8000–$BFFF) ─────────

    #[test]
    fn block_select_shifts_both_windows() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Select block 1: write value with bits 4-3 = 01 → value = 0x08
        mapper.write_prg(0x8000, 0x08);
        // block=1, page=0 → lower = (1<<2)|0 = 4; upper = (1<<2)|3 = 7
        assert_eq!(mapper.read_prg(0x8000), 4, "lower window should be bank 4");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "upper window should be bank 7 (last of block 1)"
        );
    }

    #[test]
    fn block_select_uses_bits_4_and_3_of_value() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Block 2: bits 4-3 = 0b10 → value = 0x10
        mapper.write_prg(0x8000, 0x10);
        // lower = (2<<2)|0 = 8; upper = (2<<2)|3 = 11
        assert_eq!(mapper.read_prg(0x8000), 8);
        assert_eq!(mapper.read_prg(0xC000), 11);
    }

    #[test]
    fn block_select_only_uses_two_bits() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Value 0xFF: bits 4-3 = 0b11 → block = 3; lower=(3<<2)|0=12, upper=15
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 12);
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    // ───────── Block + page combined ─────────

    #[test]
    fn block_1_page_2_selects_bank_6() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // block=1 (value=0x08), page=2 → lower=(1<<2)|2=6, upper=7
        mapper.write_prg(0x8000, 0x08); // block 1
        mapper.write_prg(0xC000, 0x02); // page 2
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn upper_window_always_fixed_to_last_page_of_block() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Change page repeatedly, upper window must stay at block's last page
        mapper.write_prg(0x8000, 0x10); // block = 2
        mapper.write_prg(0xC000, 0x00);
        assert_eq!(
            mapper.read_prg(0xC000),
            11,
            "upper = bank 11 (page 3 of block 2)"
        );
        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0xC000), 11);
        mapper.write_prg(0xC000, 0x02);
        assert_eq!(mapper.read_prg(0xC000), 11);
        mapper.write_prg(0xC000, 0x03);
        assert_eq!(mapper.read_prg(0xC000), 11);
    }

    // ───────── Submapper 1 – Aladdin Deck Enhancer ─────────

    #[test]
    fn submapper1_block_select_swaps_bits() {
        let mut mapper = make_mapper_submapper1(banked_data(16 * 1024, PRG_BANKS));
        // Aladdin: prg_block = ((value >> 4) & 0x01) | ((value >> 2) & 0x02)
        // value=0x10 (bit4=1,bit2=0): block = (1&1) | (0&2) = 1 → lower=4, upper=7
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 4, "Aladdin block from bit4=1");
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn submapper1_block_select_bit2_gives_block_2() {
        let mut mapper = make_mapper_submapper1(banked_data(16 * 1024, PRG_BANKS));
        // value=0x08 (bit3=1,bit2=0): block = ((0x08>>4)&1) | ((0x08>>2)&2)
        //   = (0&1) | (2&2) = 0 | 2 = 2 → lower=8, upper=11
        mapper.write_prg(0x8000, 0x08);
        assert_eq!(mapper.read_prg(0x8000), 8, "Aladdin block=2 from bit2=1");
        assert_eq!(mapper.read_prg(0xC000), 11);
    }

    // ───────── CHR-RAM ─────────

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    // ───────── Save state ─────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = make_mapper(prg.clone());
        // block=2, page=1 → lower=(2<<2)|1=9, upper=(2<<2)|3=11
        mapper.write_prg(0x8000, 0x10); // block 2
        mapper.write_prg(0xC000, 0x01); // page 1

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 9, "lower bank after restore");
        assert_eq!(restored.read_prg(0xC000), 11, "upper bank after restore");
    }
}
