//! Mapper 237 – Teletubbies 420-in-1 multicart
//!
//! Specifications:
//! - Main: <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_237.xhtml>
//!
//! Known Limitations:
//! - The "type" bit (T, address A2) is not emulated; setting it has no visible effect.
//!   The original hardware forces PRG A1 high when T=1, making the ROM appear to return
//!   a cartridge-type identifier, but no known software requires this to be accurate.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 237;

/// Mapper 237 – Teletubbies 420-in-1 multicart
///
/// A single write to $8000–$FFFF configures PRG banking and mirroring using
/// both address bits and data bits:
///
/// ```text
///  address (A15..A0)        data (D7..D0)
///  1... .... .... .BLT      mtMB Bbbb
///                  |||      |||| ||||
///                  |||      |||| |+++── inner 16 KiB bank  (D2:0)
///                  |||      |||+-+-+─── outer 128 KiB bank (A4, D4, D3)
///                  |||      ||+──────── mode  M  (D5): 0=UNROM  1=NROM
///                  |||      |+─────────transp t  (D6): 0=latch→A14  1=CPU→A14
///                  |||      +──────────mirror m  (D7): 0=horiz  1=vert
///                  ||+────────────────  type  T  (A2): ignored
///                  |+─────────────────  lock  L  (A3)
///                  +──────────────────  outer bank B (A4)
/// ```
///
/// **PRG banking modes** (`mt` = upper two bits of data byte):
///
/// | mt   | $8000          | $C000          | Description            |
/// |------|----------------|----------------|------------------------|
/// | `00` | `<BBBbbb>`     | `<BBB111>`     | UNROM                  |
/// | `40` | `<BBBbb0>`     | `<BBB111>`     | UNROM, lsb ignored     |
/// | `80` | `<BBBbbb>`     | `<BBBbbb>`     | 16 KiB NROM            |
/// | `C0` | `<BBBbb0>`     | `<BBBbb1>`     | 32 KiB NROM            |
///
/// **Lock bit (L, A3):** when set, only the inner bank (`bbb`) is updated on
/// subsequent writes.  The lock is cleared only on reset.
///
/// **Reset:** L, T, m, B (A4) are cleared.  t, M, and b are also cleared here.
pub struct Mapper237 {
    base: BaseMapper,
    /// Inner bank (D2:0)
    inner_bank: u8,
    /// Outer bank 3 bits: {A4, D4, D3}
    outer_bank: u8,
    /// Mode bit M (D5): false=UNROM, true=NROM
    mode: bool,
    /// Transparency bit t (D6)
    transparency: bool,
    /// Mirroring bit m (D7): false=horizontal, true=vertical
    mirroring_vertical: bool,
    /// Lock bit L (A3): when true only inner_bank updates
    locked: bool,
}

impl Mapper237 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        let mut mapper = Self {
            base,
            inner_bank: 0,
            outer_bank: 0,
            mode: false,
            transparency: false,
            mirroring_vertical: false,
            locked: false,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let outer = (self.outer_bank as i16) << 3;
        let inner = self.inner_bank as i16;

        match (self.mode, self.transparency) {
            (false, false) => {
                // UNROM: $8000=<BBBbbb>, $C000=<BBB111>
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, outer | 7);
            }
            (false, true) => {
                // UNROM with transparency: lsb of inner ignored (treated as 0)
                // $8000=<BBBbb0>, $C000=<BBB111>
                self.base.select_prg_page(0, outer | (inner & 0b110));
                self.base.select_prg_page(1, outer | 7);
            }
            (true, false) => {
                // 16 KiB NROM: both windows same bank
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, outer | inner);
            }
            (true, true) => {
                // 32 KiB NROM: A14 from CPU address
                // $8000 (A14=0) → <BBBbb0>, $C000 (A14=1) → <BBBbb1>
                self.base.select_prg_page(0, outer | (inner & 0b110));
                self.base.select_prg_page(1, outer | (inner & 0b110) | 1);
            }
        }

        // m=0 → horizontal, m=1 → vertical
        self.base.set_mirroring_hv(!self.mirroring_vertical);
    }
}

impl Mapper for Mapper237 {
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }

        let inner = value & 0x07;

        if self.locked {
            // Only inner bank may be updated when locked
            self.inner_bank = inner;
        } else {
            let addr_b = ((addr >> 4) & 1) as u8; // A4
            let lock = (addr >> 3) & 1 != 0; // A3
            // A2 (type bit) is intentionally ignored

            let data_b_lo = (value >> 3) & 1; // D3
            let data_b_hi = (value >> 4) & 1; // D4
            let mode = (value >> 5) & 1 != 0; // D5
            let transparency = (value >> 6) & 1 != 0; // D6
            let mirroring = (value >> 7) & 1 != 0; // D7

            self.inner_bank = inner;
            self.outer_bank = (addr_b << 2) | (data_b_hi << 1) | data_b_lo;
            self.mode = mode;
            self.transparency = transparency;
            self.mirroring_vertical = mirroring;
            self.locked = lock;
        }

        self.apply_banks();
    }

    fn reset(&mut self) {
        self.inner_bank = 0;
        self.outer_bank = 0;
        self.mode = false;
        self.transparency = false;
        self.mirroring_vertical = false;
        self.locked = false;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.inner_bank);
        snap.push(self.outer_bank);
        let flags = (self.mode as u8)
            | ((self.transparency as u8) << 1)
            | ((self.mirroring_vertical as u8) << 2)
            | ((self.locked as u8) << 3);
        snap.push(flags);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_len = self.base.banking_snapshot().len();
        if data.len() >= banking_len + 3 {
            self.base.restore_banking(&data[..banking_len]);
            self.inner_bank = data[banking_len];
            self.outer_bank = data[banking_len + 1];
            let flags = data[banking_len + 2];
            self.mode = flags & 0x01 != 0;
            self.transparency = flags & 0x02 != 0;
            self.mirroring_vertical = flags & 0x04 != 0;
            self.locked = flags & 0x08 != 0;
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

    // 8 outer banks × 8 inner banks = 64 × 16 KiB = 1 MiB maximum
    const PRG_BANKS: usize = 64;

    fn make_mapper(prg_rom: Vec<u8>) -> Mapper237 {
        Mapper237::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // ───────── Factory registration ─────────

    #[test]
    fn mapper_237_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 237 should be registered in factory");
    }

    // ───────── Power-on state ─────────

    #[test]
    fn power_on_lower_window_is_bank_0() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should start at bank 0");
    }

    #[test]
    fn power_on_upper_window_is_last_bank_in_outer_group() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // outer=0, UNROM mode → $C000 = outer|7 = 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should be bank 7 (UNROM last bank in outer group 0)"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ───────── Mirroring ─────────

    #[test]
    fn mirroring_bit_0_selects_horizontal() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D7=0 → horizontal
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_bit_1_selects_vertical() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D7=1 → vertical
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ───────── UNROM mode (M=0, t=0) – inner bank ─────────

    #[test]
    fn unrom_inner_bank_selects_lower_window() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D[2:0]=5 → inner bank 5, mode=UNROM
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should map to bank 5");
    }

    #[test]
    fn unrom_upper_window_fixed_to_last_bank_in_outer_group() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // inner=3, outer=0 → $C000=7
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should be fixed at bank 7"
        );
    }

    #[test]
    fn unrom_all_inner_banks_accessible() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        for b in 0u8..8 {
            mapper.write_prg(0x8000, b);
            assert_eq!(
                mapper.read_prg(0x8000),
                b,
                "inner bank {b} should be selectable"
            );
        }
    }

    // ───────── Outer bank selection ─────────

    #[test]
    fn outer_bank_from_data_d3_shifts_window_by_8() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D3=1 → outer_bank=1 → $8000=bank 8 (outer<<3|inner=0)
        mapper.write_prg(0x8000, 0x08);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 should be bank 8 with outer=1"
        );
    }

    #[test]
    fn outer_bank_from_data_d4_shifts_window_by_16() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D4=1 → outer_bank=2 → $8000=bank 16
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "$8000 should be bank 16 with outer=2"
        );
    }

    #[test]
    fn outer_bank_from_address_a4_shifts_window_by_32() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // A4=1 → outer_bank=4 → $8000=bank 32
        mapper.write_prg(0x8010, 0x00); // addr bit 4 set
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "$8000 should be bank 32 with outer=4"
        );
    }

    #[test]
    fn outer_bank_all_three_bits_combined() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // A4=1, D4=1, D3=1 → outer=7 → $8000=bank 56 (7<<3|inner=0)
        mapper.write_prg(0x8010, 0x18); // A4=1, D4=1, D3=1
        assert_eq!(
            mapper.read_prg(0x8000),
            56,
            "$8000 should be bank 56 with outer=7"
        );
    }

    #[test]
    fn outer_bank_plus_inner_bank_combined() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // outer=3 (D4=1, D3=1), inner=5 → bank 3*8+5=29
        mapper.write_prg(0x8000, 0x1D); // D4=1, D3=1, D2:0=5
        assert_eq!(
            mapper.read_prg(0x8000),
            29,
            "$8000 should be bank 29 with outer=3, inner=5"
        );
    }

    #[test]
    fn unrom_upper_window_uses_outer_bank_last() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // outer=2 (D4=1), inner=0 → $C000=outer<<3|7 = 16|7=23
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(
            mapper.read_prg(0xC000),
            23,
            "$C000 should be bank 23 (outer=2, last in group)"
        );
    }

    // ───────── UNROM with transparency (M=0, t=1) ─────────

    #[test]
    fn unrom_transparency_ignores_lsb_of_inner_bank() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D6=1 (t=1), D5=0 (M=0), D2:0=7 (inner=7=0b111) → lsb ignored → bank 6 (0b110)
        mapper.write_prg(0x8000, 0x47); // 0b0100_0111
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "With transparency, bank 7 should map to bank 6 (lsb cleared)"
        );
    }

    #[test]
    fn unrom_transparency_upper_window_still_fixed() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // t=1, M=0, inner=5 → $C000 still = outer|7
        mapper.write_prg(0x8000, 0x45); // 0b0100_0101
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should still be outer|7 with transparency"
        );
    }

    // ───────── NROM 16 KiB mode (M=1, t=0) ─────────

    #[test]
    fn nrom16_both_windows_map_to_same_bank() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // D5=1 (M=1), D6=0 (t=0), inner=5
        mapper.write_prg(0x8000, 0x25); // 0b0010_0101
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should also be bank 5");
    }

    #[test]
    fn nrom16_different_inner_banks() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        for b in 0u8..8 {
            mapper.write_prg(0x8000, 0x20 | b); // M=1, t=0, inner=b
            assert_eq!(
                mapper.read_prg(0x8000),
                b,
                "16KiB NROM bank {b}: $8000 should be bank {b}"
            );
            assert_eq!(
                mapper.read_prg(0xC000),
                b,
                "16KiB NROM bank {b}: $C000 should also be bank {b}"
            );
        }
    }

    // ───────── NROM 32 KiB mode (M=1, t=1) ─────────

    #[test]
    fn nrom32_lower_window_gets_even_bank() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // M=1, t=1, inner=6 (0b110) → $8000=bank 6 (6&6|0), $C000=bank 7 (6&6|1)
        mapper.write_prg(0x8000, 0x66); // 0b0110_0110
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 should be bank 6");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 should be bank 7");
    }

    #[test]
    fn nrom32_odd_inner_bank_aligns_to_pair() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // inner=7 (0b111) → aligned pair = 6,7
        mapper.write_prg(0x8000, 0x67); // 0b0110_0111
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "$8000 should be bank 6 (7&0b110)"
        );
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 should be bank 7");
    }

    #[test]
    fn nrom32_bank_0_pair() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // inner=0 → $8000=0, $C000=1
        mapper.write_prg(0x8000, 0x60); // M=1, t=1, inner=0
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be bank 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 should be bank 1");
    }

    // ───────── Lock bit ─────────

    #[test]
    fn lock_write_is_a_full_write_setting_lock_for_future() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // First write with A3=1: full write (not yet locked), AND sets lock for future
        // A3=1(lock), D4=1,D3=1 → outer=3, inner=2 → bank=3*8+2=26
        mapper.write_prg(0x8008, 0x1A); // A3=1, D4=1,D3=1, inner=2
        assert_eq!(
            mapper.read_prg(0x8000),
            26,
            "write with lock bit: outer=3, inner=2 → bank 26"
        );
    }

    #[test]
    fn lock_bit_prevents_outer_bank_update_on_subsequent_writes() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // First write with A3=1: sets outer=3, inner=2, locks mapper
        mapper.write_prg(0x8008, 0x1A); // A3=1(lock), D4=1,D3=1, inner=2 → outer=3, bank=26

        // Second write: locked → only inner=5 updates, outer stays 3
        mapper.write_prg(0x8000, 0x05); // inner=5, outer bits=0 (ignored because locked)
        assert_eq!(
            mapper.read_prg(0x8000),
            29,
            "outer bank should be unchanged after lock, inner updated to 5"
        );
    }

    #[test]
    fn lock_bit_prevents_mode_update() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Set NROM-16 mode with lock in one write (A3=1, M=1, inner=2)
        mapper.write_prg(0x8008, 0x22); // A3=1(lock), D5=1(M=NROM16), inner=2 → locked, NROM16
        // Subsequent write: locked → only inner updates, mode stays NROM16
        mapper.write_prg(0x8000, 0x00); // M=0, t=0, inner=0 → mode stays NROM16
        // NROM-16: both windows = outer<<3|inner = 0|0 = 0
        assert_eq!(mapper.read_prg(0x8000), 0, "inner bank updated to 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "NROM-16 mode preserved: $C000 equals $8000 bank"
        );
    }

    #[test]
    fn lock_set_by_first_write_applies_from_second_write() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // First write with A3=1: full write (unlocked), sets outer=2 (D4=1), inner=3, lock=true
        mapper.write_prg(0x8008, 0x13); // A3=1(lock), D4=1(outer D1=1) → outer=2, inner=3 → bank=19
        assert_eq!(
            mapper.read_prg(0x8000),
            19,
            "first write with lock bit: outer and inner should both be set"
        );

        // Second write: try to change outer bank → outer must stay at 2
        mapper.write_prg(0x8000, 0x07); // A3=0, D=7, inner=7 (no outer bits)
        // outer stays 2, inner=7 → bank=2*8+7=23
        assert_eq!(
            mapper.read_prg(0x8000),
            23,
            "second write while locked: outer stays 2, inner=7 → bank 23"
        );
    }

    // ───────── Reset ─────────

    #[test]
    fn reset_clears_lock_and_restores_power_on_state() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Set NROM-16, vertical mirroring, outer=3, inner=5, with lock (A3=1)
        mapper.write_prg(0x8008, 0xBD); // A3=1(lock), 0xBD=1011_1101: m=1,M=1,outer bits,inner=5
        // Verify locked state is active
        mapper.write_prg(0x8000, 0x00); // This should only update inner (locked)
        // After reset, all state should return to power-on defaults
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should return to bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should return to bank 7 (UNROM last in outer 0) after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should return to horizontal after reset"
        );
        // Verify lock is cleared: a new write should update outer bank
        mapper.write_prg(0x8000, 0x10); // D4=1 → outer=2, inner=0 → bank 16
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "outer bank should be updatable after reset (lock cleared)"
        );
    }

    // ───────── Write address range ─────────

    #[test]
    fn writes_to_c000_also_update_registers() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0xC000, 0x05); // inner=5 via $C000 write
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "$C000 write should set inner bank"
        );
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
        // 0xBD = 1011_1101: m=1(vert), t=0, M=1(NROM16), D4=1, D3=1, inner=5
        // outer = (0<<2)|(1<<1)|1 = 3, bank = 3*8+5 = 29
        mapper.write_prg(0x8000, 0xBD);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg);
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 29, "restored bank should be 29");
        assert_eq!(
            restored.read_prg(0xC000),
            29,
            "restored NROM-16: $C000 should also be 29"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be restored"
        );
    }
}
