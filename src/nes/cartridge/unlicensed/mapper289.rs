//! Mapper 289 – BMC-60311C
//!
//! Specifications:
//! - Primary reference: NesDev wiki (page unavailable at time of implementation, 403 error).
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Bmc60311C.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Bmc60311C.h>
//!
//! # Hardware overview
//!
//! Used by NTDEC multicart boards labeled BMC-60311C.
//!
//! - PRG-ROM: Two switchable 16 KiB windows ($8000–$BFFF and $C000–$FFFF).
//! - CHR-ROM: 8 KiB window at $0000–$1FFF, fixed to bank 0.
//! - Mirroring: controlled by mode bit 3 (0 = Vertical, 1 = Horizontal).
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Register map
//!
//! | Address range          | Description                                    |
//! |------------------------|------------------------------------------------|
//! | $8000–$FFFF            | `inner_prg = value & 0x07`                     |
//! | $6000–$7FFF (even)     | `mode = value & 0x0F` (addr & 1 == 0)          |
//! | $6000–$7FFF (odd)      | `outer_prg = value`   (addr & 1 == 1)          |
//!
//! Even/odd detection uses `addr & 0xE001`: 0x6000 = mode, 0x6001 = outer_prg.
//!
//! # PRG banking
//!
//! `page = outer_prg | (if mode & 0x04 == 0 { inner_prg } else { 0 })`
//!
//! | Mode bits 1:0 | PRG layout                                                   |
//! |---------------|--------------------------------------------------------------|
//! | 0 (NROM-128)  | $8000–$BFFF = page; $C000–$FFFF = page (same bank)          |
//! | 1 (NROM-256)  | $8000–$FFFF = 32 KiB bank at page & 0xFE                    |
//! | 2 (UNROM)     | $8000–$BFFF = page; $C000–$FFFF = outer_prg \| 7            |
//! | 3             | No change                                                    |
//!
//! # CHR banking
//!
//! Fixed to bank 0; CHR-ROM content at $0000–$1FFF is always from bank 0.
//!
//! # Mirroring
//!
//! - `mode & 0x08 == 0` → Vertical
//! - `mode & 0x08 != 0` → Horizontal
//!
//! # Power-on / reset state
//!
//! inner_prg = 0, outer_prg = 0, mode = 0: NROM-128, PRG bank 0, Vertical mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 289;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 289 – BMC-60311C multicart.
///
/// Three internal state variables:
/// - `inner_prg`: bits 2:0 from writes to $8000–$FFFF.
/// - `outer_prg`: full byte from writes to odd addresses in $6000–$7FFF.
/// - `mode`: bits 3:0 from writes to even addresses in $6000–$7FFF.
pub struct Mapper289 {
    base: BaseMapper,
    inner_prg: u8,
    outer_prg: u8,
    mode: u8,
}

impl Mapper289 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            inner_prg: 0,
            outer_prg: 0,
            mode: 0,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        let inner = if self.mode & 0x04 != 0 {
            0
        } else {
            self.inner_prg
        };
        let page = self.outer_prg | inner;

        match self.mode & 0x03 {
            0 => {
                // NROM-128: both windows fixed to same 16 KB bank
                self.base.select_prg_page(0, page as i16);
                self.base.select_prg_page(1, page as i16);
            }
            1 => {
                // NROM-256: 32 KB bank, two consecutive 16 KB pages
                let aligned = (page & 0xFE) as i16;
                self.base.select_prg_page(0, aligned);
                self.base.select_prg_page(1, aligned + 1);
            }
            2 => {
                // UNROM: switchable lower half, fixed upper half within outer bank
                self.base.select_prg_page(0, page as i16);
                self.base.select_prg_page(1, (self.outer_prg | 0x07) as i16);
            }
            _ => {
                // Mode 3: unknown, leave banks unchanged
            }
        }

        self.base.select_chr_page(0, 0);
        // mode bit 3: 1 = Horizontal, 0 = Vertical
        self.base.set_mirroring_hv(self.mode & 0x08 != 0);
    }
}

impl Mapper for Mapper289 {
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
            0x8000..=0xFFFF => {
                self.inner_prg = value & 0x07;
                self.apply_state();
            }
            0x6000..=0x7FFF => match addr & 0xE001 {
                0x6000 => {
                    self.mode = value & 0x0F;
                    self.apply_state();
                }
                0x6001 => {
                    self.outer_prg = value;
                    self.apply_state();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.inner_prg, self.outer_prg, self.mode]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        self.inner_prg = data[0] & 0x07;
        self.outer_prg = data[1];
        self.mode = data[2] & 0x0F;
        self.apply_state();
    }

    fn reset(&mut self) {
        self.inner_prg = 0;
        self.outer_prg = 0;
        self.mode = 0;
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to detect false-pass from modulo wrapping.
    const PRG_BANKS_16K: usize = 11;
    const CHR_BANKS_8K: usize = 7;

    fn make_mapper() -> Mapper289 {
        Mapper289::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_289_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 289 must be registered in factory");
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_mode_0_prg_both_banks_are_bank_0() {
        let mapper = make_mapper();
        // mode=0 (NROM-128): both windows fixed to bank 0
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xBFFF), 0, "$BFFF should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xFFFF), 0, "$FFFF should be PRG bank 0");
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR $0000 should be bank 0");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR $1FFF should be bank 0");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        // mode bit3=0 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be Vertical at power-on (mode bit3=0)"
        );
    }

    // ── Register write: $8000–$FFFF → inner_prg ─────────────────────────────

    #[test]
    fn write_8000_sets_inner_prg_bits_2_0() {
        let mut mapper = make_mapper();
        // mode=0 NROM-128: page = outer_prg | inner_prg = 0 | (value & 7)
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3, "inner_prg = value & 7 = 3");
        mapper.write_prg(0xFFFF, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "inner_prg = 5 from $FFFF write");
    }

    #[test]
    fn write_8000_masks_to_3_bits() {
        let mut mapper = make_mapper();
        // value=0xFF → inner_prg = 0xFF & 0x07 = 7; page=7
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 7, "inner_prg masked to 3 bits");
    }

    #[test]
    fn write_8000_to_ffff_range_all_update_inner_prg() {
        let mut mapper = make_mapper();
        for addr in [
            0x8000u16, 0x9000, 0xA000, 0xB000, 0xC000, 0xD000, 0xE000, 0xFFFF,
        ] {
            mapper.write_prg(addr, 0x02);
            assert_eq!(
                mapper.read_prg(0x8000),
                2,
                "addr ${:04X} should update inner_prg",
                addr
            );
        }
    }

    // ── Register write: $6000–$7FFF even → mode ─────────────────────────────

    #[test]
    fn write_6000_even_sets_mode_bits_3_0() {
        let mut mapper = make_mapper();
        // mode=0x08 → Horizontal mirroring
        mapper.write_prg(0x6000, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$6000 write sets mode; bit3=1 → Horizontal"
        );
    }

    #[test]
    fn write_6002_even_sets_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6002, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$6002 (even) also sets mode"
        );
    }

    #[test]
    fn write_6000_masks_mode_to_4_bits() {
        let mut mapper = make_mapper();
        // 0xFF & 0x0F = 0x0F → mode=0x0F; bits 1:0=3 (unknown), bit3=1 (Horizontal)
        mapper.write_prg(0x6000, 0xFF);
        assert_eq!(mapper.mode, 0x0F, "mode masked to 4 bits");
    }

    // ── Register write: $6000–$7FFF odd → outer_prg ─────────────────────────

    #[test]
    fn write_6001_odd_sets_outer_prg() {
        let mut mapper = make_mapper();
        // mode=0, inner_prg=0: page = outer_prg | 0 = outer_prg
        mapper.write_prg(0x6001, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 4, "$6001 write sets outer_prg=4");
    }

    #[test]
    fn write_6003_odd_sets_outer_prg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 0x06);
        assert_eq!(mapper.read_prg(0x8000), 6, "$6003 (odd) sets outer_prg=6");
    }

    #[test]
    fn write_6001_full_byte_stored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0xAB);
        assert_eq!(mapper.outer_prg, 0xAB, "outer_prg stores full byte");
    }

    // ── Mode 0: NROM-128 (both windows same bank) ────────────────────────────

    #[test]
    fn mode_0_nrom128_both_windows_same_bank() {
        let mut mapper = make_mapper();
        // mode=0, inner_prg=3: page=3; both $8000 and $C000 = bank 3
        mapper.write_prg(0x6000, 0x00); // mode=0
        mapper.write_prg(0x8000, 0x03); // inner_prg=3
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000-$BFFF = bank 3");
        assert_eq!(mapper.read_prg(0xBFFF), 3, "$BFFF = bank 3");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000-$FFFF = bank 3");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF = bank 3");
    }

    #[test]
    fn mode_0_nrom128_outer_prg_ignored_inner_prg_used() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x00); // outer_prg=0
        mapper.write_prg(0x6000, 0x00); // mode=0, bit2=0 → inner_prg used
        mapper.write_prg(0x8000, 0x05); // inner_prg=5
        // page = 0 | 5 = 5
        assert_eq!(mapper.read_prg(0x8000), 5, "mode 0: page = outer|inner = 5");
    }

    // ── Mode 0: mode bit 2 set → inner_prg ignored ──────────────────────────

    #[test]
    fn mode_bit2_set_inner_prg_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x03); // outer_prg=3
        mapper.write_prg(0x6000, 0x04); // mode=0x04: bit2=1 → inner_prg ignored
        mapper.write_prg(0x8000, 0x07); // inner_prg=7 (should be ignored)
        // page = outer_prg | 0 = 3
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "mode bit2=1: inner_prg ignored, page=outer_prg"
        );
    }

    #[test]
    fn mode_bit2_clear_inner_prg_ored_with_outer() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x08); // outer_prg=0x08
        mapper.write_prg(0x6000, 0x00); // mode=0: bit2=0 → inner used
        mapper.write_prg(0x8000, 0x03); // inner_prg=3
        // page = 0x08 | 0x03 = 0x0B = 11
        assert_eq!(
            mapper.read_prg(0x8000),
            11 % PRG_BANKS_16K as u8,
            "page = outer | inner"
        );
    }

    // ── Mode 1: NROM-256 (32 KB aligned) ────────────────────────────────────

    #[test]
    fn mode_1_nrom256_lower_window_is_page_aligned() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // mode=1 (NROM-256)
        mapper.write_prg(0x8000, 0x03); // inner_prg=3: page=3; aligned = 2
        // $8000-$BFFF = bank 2 (page & 0xFE)
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "mode 1: lower window = page & 0xFE = 2"
        );
        assert_eq!(mapper.read_prg(0xBFFF), 2, "$BFFF still bank 2");
    }

    #[test]
    fn mode_1_nrom256_upper_window_is_page_aligned_plus_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // mode=1 (NROM-256)
        mapper.write_prg(0x8000, 0x02); // inner_prg=2: page=2; aligned=2
        // $C000-$FFFF = bank 3 (page & 0xFE + 1)
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "mode 1: upper window = (page & 0xFE) + 1 = 3"
        );
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF still bank 3");
    }

    #[test]
    fn mode_1_nrom256_even_page_both_windows_sequential() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // mode=1
        mapper.write_prg(0x8000, 0x04); // inner_prg=4: page=4 (even), aligned=4
        assert_eq!(mapper.read_prg(0x8000), 4, "lower = bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "upper = bank 5");
    }

    // ── Mode 2: UNROM ────────────────────────────────────────────────────────

    #[test]
    fn mode_2_unrom_lower_window_switchable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x00); // outer_prg=0
        mapper.write_prg(0x6000, 0x02); // mode=2 (UNROM)
        mapper.write_prg(0x8000, 0x05); // inner_prg=5: page=5
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "mode 2: lower window = page = 5"
        );
        assert_eq!(mapper.read_prg(0xBFFF), 5, "$BFFF = bank 5");
    }

    #[test]
    fn mode_2_unrom_upper_window_fixed_outer_or_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x08); // outer_prg=0x08
        mapper.write_prg(0x6000, 0x02); // mode=2
        mapper.write_prg(0x8000, 0x03); // inner_prg=3
        // upper = outer_prg | 7 = 0x08 | 0x07 = 0x0F = 15 % 11 = 4
        assert_eq!(
            mapper.read_prg(0xC000),
            (0x08u8 | 0x07) % PRG_BANKS_16K as u8,
            "mode 2: upper = outer_prg | 7"
        );
    }

    #[test]
    fn mode_2_unrom_upper_window_outer_prg_zero_gives_bank_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x00); // outer_prg=0
        mapper.write_prg(0x6000, 0x02); // mode=2
        mapper.write_prg(0x8000, 0x00); // inner_prg=0
        // upper = 0 | 7 = 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7 % PRG_BANKS_16K as u8,
            "mode 2: upper = 0 | 7 = 7"
        );
    }

    // ── Mode 3: Unknown ──────────────────────────────────────────────────────

    #[test]
    fn mode_3_unknown_leaves_banks_unchanged() {
        let mut mapper = make_mapper();
        // Set mode=0 with known bank
        mapper.write_prg(0x6000, 0x00); // mode=0
        mapper.write_prg(0x8000, 0x03); // inner_prg=3
        let prg0_before = mapper.read_prg(0x8000);
        let prg1_before = mapper.read_prg(0xC000);

        // Switch to mode=3 (unknown): banks should not change
        mapper.write_prg(0x6000, 0x03); // mode=3
        assert_eq!(
            mapper.read_prg(0x8000),
            prg0_before,
            "mode 3: lower bank unchanged"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            prg1_before,
            "mode 3: upper bank unchanged"
        );
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn chr_is_always_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x07);
        mapper.write_prg(0x6001, 0xFF);
        mapper.write_prg(0x6000, 0x0F);
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR always bank 0 at $0000");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR always bank 0 at $1FFF");
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn mode_bit3_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x08); // mode bit3=1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mode bit3=1 → Horizontal"
        );
    }

    #[test]
    fn mode_bit3_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x08); // Horizontal
        mapper.write_prg(0x6000, 0x00); // bit3=0 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mode bit3=0 → Vertical"
        );
    }

    // ── Reset behavior ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x04);
        mapper.write_prg(0x6000, 0x0B); // mode=0x0B
        mapper.write_prg(0x8000, 0x07);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "page 0 = 0 after reset");
        assert_eq!(mapper.read_prg(0xC000), 0, "page 1 = 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Vertical mirroring after reset (mode bit3=0)"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x08); // outer_prg=8
        mapper.write_prg(0x6000, 0x02); // mode=2 (UNROM)
        mapper.write_prg(0x8000, 0x03); // inner_prg=3; page=8|3=11
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // mode 2 UNROM: lower = page = 11 % 11 = 0; upper = 8|7=15 % 11 = 4
        assert_eq!(
            restored.read_prg(0x8000),
            11 % PRG_BANKS_16K as u8,
            "restored lower bank"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            (0x08u8 | 0x07) % PRG_BANKS_16K as u8,
            "restored upper bank"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "restored mirroring"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        let bank_before = mapper.read_prg(0x8000);
        mapper.restore_registers(&[0x00, 0x00]); // only 2 bytes, needs 3
        assert_eq!(
            mapper.read_prg(0x8000),
            bank_before,
            "state unchanged after short restore"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring required");
        assert!(!caps.has_chr_banking, "CHR is fixed to bank 0");
        assert_eq!(caps.prg_bank_size_kb, 16, "16 KB PRG banks");
        assert_eq!(caps.chr_bank_size_kb, 8, "8 KB CHR bank");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0x6001, 0xFF);
        mapper.write_prg(0x6000, 0x0F);
        assert!(!mapper.irq_pending(), "Mapper 289 must never assert IRQ");
    }
}
