//! Mapper 331 – BMC 12-in-1
//!
//! Specifications:
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//! - Backup: https://nesdev-wiki.nes.science/wikipages/NES_2.0_Mapper_331.xhtml (404).
//! - Primary reference: Mesen2 `Bmc12in1.h` (class `Bmc12in1`).
//!
//! # Hardware overview
//!
//! Multi-cart board used for "12-in-1" compilations.
//!
//! ## Register map
//!
//! | Address range | Bits used | Function          |
//! |---------------|-----------|-------------------|
//! | $A000–$BFFF   | [7:0]     | Register 0 (reg0) |
//! | $C000–$DFFF   | [7:0]     | Register 1 (reg1) |
//! | $E000–$FFFF   | [3:0]     | Mode register     |
//!
//! Writes to $8000–$9FFF are ignored.
//!
//! ## Outer bank
//!
//! `outer = (mode & 0x03) << 3`  — selects a group of 8 × 16 KB PRG banks
//! and 32 × 4 KB CHR banks.
//!
//! ## PRG banking (two 16 KB windows at $8000 and $C000)
//!
//! - **16 KB mode** (mode bit 3 = 0):
//!   - $8000 → `outer | (reg0 & 0x07)`
//!   - $C000 → `outer | 0x07`  (fixed to last bank in outer group)
//! - **32 KB mode** (mode bit 3 = 1):
//!   - $8000 → `outer | (reg0 & 0x06)`      (even)
//!   - $C000 → `outer | (reg0 & 0x06) | 1`  (odd, consecutive pair)
//!
//! ## CHR banking (two 4 KB windows at $0000 and $1000)
//!
//! - $0000 (4 KB) → `(reg0 >> 3) | ((outer & 0x18) << 2)`
//! - $1000 (4 KB) → `(reg1 >> 3) | ((outer & 0x18) << 2)`
//!
//! Where `(outer & 0x18) << 2` expands to `(mode & 0x03) << 5`.
//!
//! ## Mirroring
//!
//! Controlled by mode bit 2:
//! - `mode & 0x04 == 0` → Vertical
//! - `mode & 0x04 != 0` → Horizontal
//!
//! ## Power-on / reset state
//!
//! All registers zero: outer = 0, 16 KB mode, $8000 → bank 0, $C000 → bank 7,
//! CHR $0000/$1000 both → bank 0, mirroring = Vertical.
//!
//! ## Known limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 331;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 4 * 1024;
const REGISTERS_SNAPSHOT_LEN: usize = 3;

/// Mapper 331 – BMC 12-in-1
///
/// Registers:
/// - `reg0`: written at $A000–$BFFF
/// - `reg1`: written at $C000–$DFFF
/// - `mode`: written at $E000–$FFFF (only bits [3:0] used)
pub struct Mapper331 {
    base: BaseMapper,
    reg0: u8,
    reg1: u8,
    mode: u8,
}

impl Mapper331 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: PRG_BANK_SIZE_BYTES / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE_BYTES / 1024,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        let mut mapper = Self {
            base,
            reg0: 0,
            reg1: 0,
            mode: 0,
        };
        mapper.update_state();
        mapper
    }

    fn update_state(&mut self) {
        let outer = (self.mode & 0x03) << 3;
        // CHR banking: two 4 KB pages
        let chr_outer = (outer as u16) << 2; // = (mode & 0x03) << 5
        let chr0 = ((self.reg0 >> 3) as u16) | chr_outer;
        let chr1 = ((self.reg1 >> 3) as u16) | chr_outer;
        self.base.select_chr_page(0, chr0 as i16);
        self.base.select_chr_page(1, chr1 as i16);

        // PRG banking
        if self.mode & 0x08 != 0 {
            // 32 KB mode: consecutive pair selected by reg0 bits [2:1]
            let pair_base = (outer | (self.reg0 & 0x06)) as i16;
            self.base.select_prg_page(0, pair_base);
            self.base.select_prg_page(1, pair_base | 1);
        } else {
            // 16 KB mode
            self.base
                .select_prg_page(0, (outer | (self.reg0 & 0x07)) as i16);
            self.base.select_prg_page(1, (outer | 0x07) as i16);
        }

        // Mirroring: bit 2 of mode
        self.base.set_mirroring_hv((self.mode & 0x04) != 0);
    }
}

impl Mapper for Mapper331 {
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
        match addr & 0xE000 {
            0xA000 => {
                self.reg0 = value;
                self.update_state();
            }
            0xC000 => {
                self.reg1 = value;
                self.update_state();
            }
            0xE000 => {
                self.mode = value & 0x0F;
                self.update_state();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg0, self.reg1, self.mode]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.reg0 = data[0];
        self.reg1 = data[1];
        self.mode = data[2] & 0x0F;
        self.update_state();
    }

    fn reset(&mut self) {
        self.reg0 = 0;
        self.reg1 = 0;
        self.mode = 0;
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    // PRG: 16 KB banks; mode bits [1:0] select 4 outer groups × 8 inner banks = 32 possible banks;
    //      use 48 banks to give extra headroom beyond the valid range.
    const PRG_BANKS_16K: usize = 48;
    // CHR: 4 KB banks; there are up to 4 × 32 = 128 possible banks; use 48 (non-power-of-two) for testing.
    const CHR_BANKS_4K: usize = 48;

    fn make_mapper() -> Mapper331 {
        Mapper331::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_4K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_331_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_4K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 331 must be registered in factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_maps_to_bank_0() {
        // mode=0, reg0=0 → outer=0, 16KB mode, $8000 = outer|0 = bank 0
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_maps_to_bank_7() {
        // mode=0 → outer=0, 16KB mode, $C000 = outer|7 = bank 7
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should map to PRG bank 7 at power-on"
        );
    }

    #[test]
    fn power_on_chr_0000_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 should map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_1000_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "CHR $1000 should map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical at power-on (mode bit2=0)"
        );
    }

    // ── PRG banking – 16 KB mode (mode bit 3 = 0) ────────────────────────────

    #[test]
    fn prg_16kb_mode_8000_selects_bank_by_reg0_bits_2_0() {
        // Write reg0=5 (0b00000101): outer=0, $8000 = outer|5 = bank 5
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "$8000 should select bank reg0 & 0x07 = 5"
        );
    }

    #[test]
    fn prg_16kb_mode_c000_always_fixed_to_last_in_group() {
        // Write reg0=5: $C000 = outer|7 regardless of reg0
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should be fixed to outer|7 in 16KB mode"
        );
    }

    #[test]
    fn prg_16kb_mode_c000_fixed_with_different_reg0() {
        // Write reg0=3: $C000 still = outer|7 = 7
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 fixed to 7 regardless of reg0 in 16KB mode"
        );
    }

    #[test]
    fn prg_register_written_only_at_a000() {
        // Writes to $8000 should be ignored (bit pattern $8000 & $E000 = $8000)
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5); // should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "writes to $8000–$9FFF should be ignored"
        );
    }

    // ── PRG banking – 32 KB mode (mode bit 3 = 1) ────────────────────────────

    #[test]
    fn prg_32kb_mode_selects_consecutive_pair() {
        // mode = 0x08: bit3=1 (32KB), outer=0
        // reg0 = 0x04 (bits[2:1] = 2 → pair base = 0|4 = 4)
        // $8000 = 4, $C000 = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x08); // set 32KB mode
        mapper.write_prg(0xA000, 0x04); // reg0 bits[2:1]=2 → pair 4/5
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5 (pair)");
    }

    #[test]
    fn prg_32kb_mode_pair_ignores_bit0_of_reg0() {
        // reg0 = 0x05 (bits[2:1] = 2 → same pair as 0x04)
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x08);
        mapper.write_prg(0xA000, 0x05); // bit0 set but should be masked
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 bank 4 (bit0 of reg0 masked)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 bank 5 (consecutive with bank 4)"
        );
    }

    // ── Outer bank (mode bits [1:0]) ──────────────────────────────────────────

    #[test]
    fn outer_bank_shifts_prg_by_8_banks() {
        // mode = 0x01: outer = 8, 16KB mode
        // $8000 = 8 | (reg0 & 0x07) = 8, $C000 = 8|7 = 15
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 should be bank 8");
        assert_eq!(mapper.read_prg(0xC000), 15, "$C000 should be bank 15");
    }

    #[test]
    fn outer_bank_2_shifts_prg_by_16_banks() {
        // mode = 0x02: outer = 16
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 16, "$8000 should be bank 16");
        assert_eq!(mapper.read_prg(0xC000), 23, "$C000 should be bank 23");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_page0_selected_by_reg0_bits_7_3() {
        // reg0 = 0x28 (bits[7:3]=5): CHR page0 = 5, outer=0 → page0=5
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x28); // 0x28 >> 3 = 5
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR page0 should be 5 (reg0[7:3]=5)"
        );
    }

    #[test]
    fn chr_page1_selected_by_reg1_bits_7_3() {
        // reg1 = 0x48 (bits[7:3]=9): CHR page1 = 9
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x48); // 0x48 >> 3 = 9
        assert_eq!(
            mapper.read_chr(0x1000),
            9,
            "CHR page1 should be 9 (reg1[7:3]=9)"
        );
    }

    #[test]
    fn chr_outer_bank_shifts_by_32() {
        // mode = 0x01: outer=8, chr_outer = 8<<2 = 32
        // reg0=0: CHR page0 = 0 | 32 = 32
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x01); // mode: outer=1 → chr_outer = (1<<3)<<2 = 32
        assert_eq!(
            mapper.read_chr(0x0000),
            32,
            "CHR page0 should be 32 with outer=1"
        );
    }

    #[test]
    fn chr_pages_are_independent() {
        // reg0 = 0x10 (page0 = 2), reg1 = 0x58 (page1 = 11)
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x10); // 0x10 >> 3 = 2
        mapper.write_prg(0xC000, 0x58); // 0x58 >> 3 = 11
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR page0 should be 2");
        assert_eq!(mapper.read_chr(0x1000), 11, "CHR page1 should be 11");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mode_bit2_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x04); // bit2=1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mode bit2=1 should set horizontal mirroring"
        );
    }

    #[test]
    fn mode_bit2_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x04); // set horizontal
        mapper.write_prg(0xE000, 0x00); // clear bit2
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mode bit2=0 should set vertical mirroring"
        );
    }

    // ── Mode register upper nibble is ignored ─────────────────────────────────

    #[test]
    fn mode_register_ignores_upper_nibble() {
        // Writing 0xF4 should be equivalent to 0x04 (horizontal mirroring, outer=0)
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0xF4);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "upper nibble of mode register must be ignored"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "outer bank should be 0 when mode upper nibble is masked"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip_preserves_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x28); // reg0: CHR page0=5, PRG bank=0
        mapper.write_prg(0xC000, 0x48); // reg1: CHR page1=9
        mapper.write_prg(0xE000, 0x05); // mode: outer=1, 16KB, vertical
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // CHR banks preserved
        assert_eq!(restored.read_chr(0x0000), 5 | 32, "CHR page0 restored");
        assert_eq!(restored.read_chr(0x1000), 9 | 32, "CHR page1 restored");
        // PRG: outer=8 | (reg0&7)=0 = 8
        assert_eq!(restored.read_prg(0x8000), 8, "PRG $8000 restored");
        assert_eq!(restored.read_prg(0xC000), 15, "PRG $C000 restored");
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        mapper.restore_registers(&[3]); // too short; state should be unchanged
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "restore with insufficient data must be no-op"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x0F); // non-zero mode
        mapper.write_prg(0xA000, 0xFF);
        mapper.write_prg(0xC000, 0xFF);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "PRG $C000 must be bank 7 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR page0 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "CHR page1 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring must be vertical after reset"
        );
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x0F);
        assert!(!mapper.irq_pending(), "Mapper 331 must never assert IRQ");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

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
