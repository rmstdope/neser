//! Mapper 337 – BMC-CTC-12IN1 (12-in-1 multicart, 1991 New Star Co. Ltd.)
//!
//! Specifications:
//! - Primary reference: libretro-fceumm `12in1.c` (CaH4e3, 2009)
//! - UNIF board name: BMC-CTC-12IN1
//!
//! ## Register Map ($8000–$FFFF, write)
//!
//! | Address range | Register   | Bits stored |
//! |---------------|------------|-------------|
//! | $8000–$9FFF   | (ignored)  | –           |
//! | $A000–$BFFF   | prgchr\[0\]| [7:0]       |
//! | $C000–$DFFF   | prgchr\[1\]| [7:0]       |
//! | $E000–$FFFF   | ctrl       | [3:0]       |
//!
//! ## PRG Banking (two 16 KB windows)
//!
//! Let `outer = (ctrl & 0x03) << 3`.
//!
//! - **Normal mode** (`ctrl` bit 3 = 0):
//!   - `$8000` → `outer | (prgchr[0] & 0x07)`
//!   - `$C000` → `outer | 0x07` (last bank in outer group)
//! - **NROM mode** (`ctrl` bit 3 = 1):
//!   - `$8000` → `outer | (prgchr[0] & 0x06)` (even bank)
//!   - `$C000` → `outer | (prgchr[0] & 0x06) | 1` (odd bank, consecutive pair)
//!
//! ## CHR Banking (two 4 KB windows)
//!
//! Let `outer_chr = (ctrl & 0x03) << 5`.
//!
//! - `$0000` → `outer_chr | (prgchr[0] >> 3)`
//! - `$1000` → `outer_chr | (prgchr[1] >> 3)`
//!
//! ## Mirroring
//!
//! Controlled by `ctrl` bit 2:
//! - Bit 2 = 0 → Vertical
//! - Bit 2 = 1 → Horizontal
//!
//! ## Power-on / Reset
//!
//! All registers initialised to 0: PRG bank 0/7, CHR banks 0/0, vertical mirroring.
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 337;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 4 * 1024;
const CTRL_MASK: u8 = 0x0F;
const CTRL_NROM_BIT: u8 = 0x08;
const CTRL_MIRROR_BIT: u8 = 0x04;
const CTRL_OUTER_MASK: u8 = 0x03;
const REGISTERS_SNAPSHOT_LEN: usize = 3;

pub struct Mapper337 {
    base: BaseMapper,
    prg_chr_0: u8,
    prg_chr_1: u8,
    ctrl: u8,
}

impl Mapper337 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 4,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_chr_0: 0,
            prg_chr_1: 0,
            ctrl: 0,
        };
        mapper.apply_state(0, 0, 0);
        mapper
    }

    fn prg_bank_8000(prg_chr_0: u8, ctrl: u8) -> i16 {
        let outer = ((ctrl & CTRL_OUTER_MASK) as i16) << 3;
        if ctrl & CTRL_NROM_BIT != 0 {
            outer | ((prg_chr_0 & 0x06) as i16)
        } else {
            outer | ((prg_chr_0 & 0x07) as i16)
        }
    }

    fn prg_bank_c000(prg_chr_0: u8, ctrl: u8) -> i16 {
        let outer = ((ctrl & CTRL_OUTER_MASK) as i16) << 3;
        if ctrl & CTRL_NROM_BIT != 0 {
            outer | ((prg_chr_0 & 0x06) as i16) | 1
        } else {
            outer | 7
        }
    }

    fn chr_bank_0000(prg_chr_0: u8, ctrl: u8) -> i16 {
        let outer_chr = ((ctrl & CTRL_OUTER_MASK) as i16) << 5;
        outer_chr | ((prg_chr_0 >> 3) as i16)
    }

    fn chr_bank_1000(prg_chr_1: u8, ctrl: u8) -> i16 {
        let outer_chr = ((ctrl & CTRL_OUTER_MASK) as i16) << 5;
        outer_chr | ((prg_chr_1 >> 3) as i16)
    }

    fn apply_state(&mut self, prg_chr_0: u8, prg_chr_1: u8, ctrl: u8) {
        self.prg_chr_0 = prg_chr_0;
        self.prg_chr_1 = prg_chr_1;
        self.ctrl = ctrl;

        self.base
            .select_prg_page(0, Self::prg_bank_8000(prg_chr_0, ctrl));
        self.base
            .select_prg_page(1, Self::prg_bank_c000(prg_chr_0, ctrl));
        self.base
            .select_chr_page(0, Self::chr_bank_0000(prg_chr_0, ctrl));
        self.base
            .select_chr_page(1, Self::chr_bank_1000(prg_chr_1, ctrl));
        self.base.set_mirroring_hv((ctrl & CTRL_MIRROR_BIT) != 0);
    }
}

impl Mapper for Mapper337 {
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
        match addr {
            0xA000..=0xBFFF => self.apply_state(value, self.prg_chr_1, self.ctrl),
            0xC000..=0xDFFF => self.apply_state(self.prg_chr_0, value, self.ctrl),
            0xE000..=0xFFFF => self.apply_state(self.prg_chr_0, self.prg_chr_1, value & CTRL_MASK),
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_chr_0, self.prg_chr_1, self.ctrl]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.apply_state(data[0], data[1], data[2]);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two counts to prevent false-pass modulo wrapping.
    const PRG_BANKS_16K: usize = 48;
    const CHR_BANKS_4K: usize = 96;

    fn make_mapper() -> Mapper337 {
        Mapper337::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_4K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_337_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_4K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 337 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_is_bank_7() {
        let mapper = make_mapper();
        // Normal mode, outer=0, fixed to bank 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should map to PRG bank 7 at power-on"
        );
    }

    #[test]
    fn power_on_chr_0000_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 should be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_1000_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "CHR $1000 should be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical at power-on"
        );
    }

    // ── PRG banking: normal mode (ctrl bit 3 = 0) ────────────────────────────

    #[test]
    fn normal_mode_prg_8000_follows_prgchr0_bits_2_0() {
        // prgchr[0] = 5 (bits[2:0]=5), ctrl = 0 → $8000 = 0 | 5 = bank 5
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be PRG bank 5");
    }

    #[test]
    fn normal_mode_prg_c000_is_fixed_to_last_in_outer_group() {
        // ctrl = 0, any prgchr[0] value → $C000 = 0 | 7 = bank 7
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 3); // prgchr[0] = 3, still fixed at 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should always be bank 7 in normal mode"
        );
    }

    #[test]
    fn normal_mode_prg_8000_uses_only_low_3_bits_of_prgchr0() {
        // prgchr[0] = 0xF8 (upper bits set), bits[2:0] = 0 → $8000 = bank 0
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0xF8);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should use only bits[2:0]"
        );
    }

    // ── PRG banking: outer bank (ctrl bits [1:0]) ─────────────────────────────

    #[test]
    fn outer_bank_shifts_prg_window_by_8_banks() {
        // ctrl = 1 (outer=1): outer = 1<<3 = 8; prgchr[0]=2 → $8000 = 8|2 = 10
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 2); // prgchr[0] = 2
        mapper.write_prg(0xE000, 1); // ctrl = 1 (outer=1, normal mode)
        assert_eq!(
            mapper.read_prg(0x8000),
            10,
            "$8000 should be bank 10 with outer=1, prgchr0=2"
        );
        // $C000 = outer | 7 = 8 | 7 = 15
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "$C000 should be bank 15 with outer=1"
        );
    }

    #[test]
    fn ctrl_outer_bits_only_use_bits_1_0() {
        // ctrl high bits above 3 are masked (ctrl & 0x0F, then outer = ctrl[1:0]<<3)
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0xFF); // ctrl = 0x0F, outer = 0x03 << 3 = 24
        mapper.write_prg(0xA000, 0); // prgchr[0] = 0
        // NROM bit (bit3=1), outer = (0x0F & 3) << 3 = 3<<3 = 24 → even bank
        // $8000 = 24 | (0 & 6) = 24
        assert_eq!(
            mapper.read_prg(0x8000),
            24,
            "$8000 should use only ctrl[1:0] for outer bank"
        );
    }

    // ── PRG banking: NROM mode (ctrl bit 3 = 1) ──────────────────────────────

    #[test]
    fn nrom_mode_both_windows_are_consecutive_even_odd_pair() {
        // ctrl = 0x08 (NROM bit set, outer=0), prgchr[0]=4 (bits[2:1]=2) → even=4, odd=5
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 4);
        mapper.write_prg(0xE000, 0x08);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 should be even bank 4 in NROM mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 should be odd bank 5 in NROM mode"
        );
    }

    #[test]
    fn nrom_mode_odd_bit_of_prgchr0_is_ignored_for_bank_base() {
        // ctrl = 0x08, prgchr[0] = 7 (bits[2:1]=3, bit0 ignored) → even = 6, odd = 7
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 7);
        mapper.write_prg(0xE000, 0x08);
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "$8000 should be bank 6 (even); bit0 of prgchr[0] is ignored"
        );
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 should be bank 7 (odd)");
    }

    #[test]
    fn nrom_mode_with_outer_bank_2() {
        // ctrl = 0x0A (bit3=NROM, outer=2): outer = 2<<3 = 16; prgchr[0]=2 → banks 16+2=18, 19
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 2);
        mapper.write_prg(0xE000, 0x0A);
        assert_eq!(
            mapper.read_prg(0x8000),
            18,
            "$8000 should be bank 18 in NROM mode with outer=2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            19,
            "$C000 should be bank 19 in NROM mode with outer=2"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_0000_bank_comes_from_prgchr0_bits_7_3() {
        // prgchr[0] = 0x18 (bits[7:3]=3), ctrl=0 → CHR bank 0 = 3
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x18); // 0b0001_1000 → bits[7:3] = 3
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR $0000 bank should be 3");
    }

    #[test]
    fn chr_1000_bank_comes_from_prgchr1_bits_7_3() {
        // prgchr[1] = 0x28 (bits[7:3]=5), ctrl=0 → CHR bank 1 = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x28); // 0b0010_1000 → bits[7:3] = 5
        assert_eq!(mapper.read_chr(0x1000), 5, "CHR $1000 bank should be 5");
    }

    #[test]
    fn chr_outer_bank_extends_both_chr_windows() {
        // ctrl = 0x01 (outer=1), outer_chr = 1<<5 = 32
        // prgchr[0]=0x10 → bits[7:3]=2 → CHR$0000 = 32|2 = 34
        // prgchr[1]=0x20 → bits[7:3]=4 → CHR$1000 = 32|4 = 36
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x10); // prgchr[0]: bits[7:3]=2
        mapper.write_prg(0xC000, 0x20); // prgchr[1]: bits[7:3]=4
        mapper.write_prg(0xE000, 0x01); // ctrl: outer=1
        assert_eq!(
            mapper.read_chr(0x0000),
            34,
            "CHR $0000 should be bank 34 with outer=1"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            36,
            "CHR $1000 should be bank 36 with outer=1"
        );
    }

    #[test]
    fn chr_windows_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x08); // prgchr[0] bits[7:3]=1 → CHR$0000=1
        mapper.write_prg(0xC000, 0x40); // prgchr[1] bits[7:3]=8 → CHR$1000=8
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x1000), 8);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn ctrl_bit2_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x04); // ctrl bit2 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "ctrl bit2=1 should select horizontal mirroring"
        );
    }

    #[test]
    fn ctrl_bit2_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x04); // set horizontal first
        mapper.write_prg(0xE000, 0x00); // clear bit2
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "ctrl bit2=0 should select vertical mirroring"
        );
    }

    // ── Write address decode ──────────────────────────────────────────────────

    #[test]
    fn writes_to_8000_9fff_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 3); // establish known state
        mapper.write_prg(0x8000, 0xFF); // should be ignored
        mapper.write_prg(0x9FFF, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "prgchr[0] should be unchanged");
    }

    #[test]
    fn writes_below_8000_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0xFF);
        mapper.write_prg(0x6000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "banks should remain at power-on"
        );
    }

    #[test]
    fn a000_bfff_writes_update_prgchr0_not_prgchr1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x20); // prgchr[1] bits[7:3]=4 → CHR$1000=4
        mapper.write_prg(0xA000, 0x08); // prgchr[0] bits[7:3]=1 → CHR$0000=1
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "prgchr[1] unchanged by A000 write"
        );
    }

    #[test]
    fn c000_dfff_writes_update_prgchr1_not_prgchr0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x08); // prgchr[0] bits[7:3]=1 → CHR$0000=1
        mapper.write_prg(0xC000, 0x20); // prgchr[1] bits[7:3]=4 → CHR$1000=4
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "prgchr[0] unchanged by C000 write"
        );
        assert_eq!(mapper.read_chr(0x1000), 4);
    }

    #[test]
    fn ctrl_high_nibble_bits_are_masked() {
        // Writing 0xF4 to E000 should store only 0x04 (low nibble), giving horizontal
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0xF4);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "only low 4 bits of ctrl are stored"
        );
        // NROM bit is clear (0xF4 & 0x0F = 0x04, bit3=0) → normal mode
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 fixed at 7 in normal mode"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x15); // prgchr[0]
        mapper.write_prg(0xC000, 0x38); // prgchr[1]
        mapper.write_prg(0xE000, 0x09); // ctrl: outer=1, NROM mode
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // ctrl=0x09: NROM mode, outer=1(<<3=8), prgchr[0]=0x15 & 6 = 4 → $8000=12, $C000=13
        assert_eq!(
            restored.read_prg(0x8000),
            12,
            "restored $8000 bank should be 12"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            13,
            "restored $C000 bank should be 13"
        );
        // CHR$0000: outer_chr=(1<<5)=32, prgchr[0]>>3=0x15>>3=2 → 34
        assert_eq!(
            restored.read_chr(0x0000),
            34,
            "restored CHR $0000 should be bank 34"
        );
        // CHR$1000: outer_chr=32, prgchr[1]>>3=0x38>>3=7 → 39
        assert_eq!(
            restored.read_chr(0x1000),
            39,
            "restored CHR $1000 should be bank 39"
        );
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x15);
        mapper.write_prg(0xC000, 0x38);
        mapper.write_prg(0xE000, 0x09);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_dynamic_mirroring);
        assert!(caps.has_chr_banking);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 4);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
