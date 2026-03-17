//! Mapper 233 - Super 42-in-1 multicart (reset-based variant)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_233>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper233.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, RamInitMode};

const MAPPER_NUMBER: u16 = 233;

/// Mapper 233 - Super 42-in-1 multicart (reset-based variant)
///
/// This mapper is a variant of Mapper 226 where the high PRG-address bit (bit 5 of
/// the page register) is no longer driven by register 0 bit 7, but instead by a
/// toggle that flips on every soft reset.  This lets the multicart expose a second
/// set of 32 games after the console is reset.
///
/// ## Register map
///
/// Two registers, written via any address in $8000–$FFFF with the address decoded
/// by bit 0 of the address:
///
/// - `$8000` (A0=0): `[.MOP PPPP]`
///   - bits 4–0: low 5 bits of PRG page
///   - bit 5 (O): PRG mode — 0 = 32 KB (both windows), 1 = 16 KB (same page twice)
///   - bit 6 (M): mirroring — 0 = Horizontal, 1 = Vertical
///   - bit 7: unused (unlike Mapper 226, NOT used for the PRG page)
/// - `$8001` (A0=1): `[.... ...H]`
///   - bit 0 (H): high bit of PRG page (bit 6)
///
/// ## PRG-page formula
///
/// ```text
/// prg_page = (reg0 & 0x1F) | (reset_toggle << 5) | ((reg1 & 0x01) << 6)
/// ```
///
/// ## Banking
///
/// - **32 KB mode** (O=0): both 16 KB windows select the aligned even/odd pair
///   (`prg_page & 0xFE` and `(prg_page & 0xFE) | 1`).
/// - **16 KB mode** (O=1): both 16 KB windows select the same page (`prg_page`).
///
/// ## Mirroring
///
/// Controlled by bit 6 of register 0: 0 = Horizontal, 1 = Vertical.
///
/// ## Reset behaviour
///
/// - **Soft reset**: toggles `reset_toggle` (XOR 1), then clears both registers.
/// - **Hard reset**: `reset_toggle` = 0, then clears both registers.
///
/// ## CHR
///
/// Uses 8 KB CHR-RAM (no CHR banking).
pub struct Mapper233 {
    base: BaseMapper,
    /// Low register ($8000): [.MOP PPPP]
    reg0: u8,
    /// High register ($8001): [..... ...H]
    reg1: u8,
    /// Toggle bit that flips on each soft reset; becomes PRG-page bit 5.
    reset_toggle: u8,
    /// Flag indicating that the next call to `reset()` should be treated as a hard reset.
    /// This is set by `initialize_ram()` (hard reset only) and consumed by `reset()`.
    hard_reset_pending: bool,
}

impl Mapper233 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
            reg0: 0,
            reg1: 0,
            reset_toggle: 0,
            hard_reset_pending: false,
        };
        mapper.apply_banks();
        mapper
    }

    fn prg_page(&self) -> i16 {
        ((self.reg0 & 0x1F) | (self.reset_toggle << 5) | ((self.reg1 & 0x01) << 6)) as i16
    }

    fn apply_banks(&mut self) {
        let page = self.prg_page();
        if self.reg0 & 0x20 != 0 {
            // 16 KB mode: both windows see the same page
            self.base.select_prg_page(0, page);
            self.base.select_prg_page(1, page);
        } else {
            // 32 KB mode: aligned even/odd pair
            self.base.select_prg_page(0, page & !1);
            self.base.select_prg_page(1, (page & !1) | 1);
        }
        let horizontal = self.reg0 & 0x40 == 0;
        self.base.set_mirroring_hv(horizontal);
    }
}

impl Mapper for Mapper233 {
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        if addr & 0x0001 == 0 {
            self.reg0 = value;
        } else {
            self.reg1 = value;
        }
        self.apply_banks();
    }

    fn initialize_ram(&mut self) {
        // Called on hard reset only; mark that the next reset should be treated as hard.
        self.hard_reset_pending = true;
    }

    fn reset(&mut self) {
        if self.hard_reset_pending {
            // Hard reset semantics: force reset_toggle to 0.
            self.reset_toggle = 0;
            self.hard_reset_pending = false;
        } else {
            // Soft reset semantics: toggle reset_toggle.
            self.reset_toggle ^= 0x01;
        }
        self.reg0 = 0;
        self.reg1 = 0;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.reg0);
        snap.push(self.reg1);
        snap.push(self.reset_toggle);
        snap.push(self.hard_reset_pending as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected_banking_len = self.base.banking_snapshot().len();
        if data.len() >= expected_banking_len + 4 {
            // New-format snapshot with hard_reset_pending included.
            self.base.restore_banking(&data[..expected_banking_len]);
            self.reg0 = data[expected_banking_len];
            self.reg1 = data[expected_banking_len + 1];
            self.reset_toggle = data[expected_banking_len + 2];
            self.hard_reset_pending = data[expected_banking_len + 3] != 0;
            self.apply_banks();
        } else if data.len() >= expected_banking_len + 3 {
            // Backward-compatible path: older snapshots without hard_reset_pending.
            self.base.restore_banking(&data[..expected_banking_len]);
            self.reg0 = data[expected_banking_len];
            self.reg1 = data[expected_banking_len + 1];
            self.reset_toggle = data[expected_banking_len + 2];
            self.hard_reset_pending = false;
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

    /// Number of 16 KB PRG banks.  Use a non-power-of-two count to avoid
    /// modulo-wrapping false-passes in bank-selection assertions.
    const PRG_BANKS: usize = 48;

    fn make_mapper(prg_rom: Vec<u8>) -> Mapper233 {
        Mapper233::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    // ───────── Factory registration ─────────

    #[test]
    fn mapper_233_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 233 should be registered in factory");
    }

    // ───────── Power-on state ─────────

    #[test]
    fn power_on_lower_window_starts_at_bank_0() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 window should start at bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_upper_window_starts_at_bank_1() {
        let mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // 32 KB mode (O=0), page 0: lower=bank0, upper=bank1
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 window should start at bank 1 on power-on (32KB mode, aligned pair 0-1)"
        );
    }

    // ───────── Register writes – PRG banking (32 KB mode, O=0) ─────────

    #[test]
    fn reg0_prg_bits_select_32kb_bank_pair() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // Write reg0 (addr even): bits 4:0 = 4 → page 4; O=0 → 32KB mode
        // lower = bank 4, upper = bank 5
        mapper.write_prg(0x8000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 4, "lower window should be bank 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "upper window should be bank 5");
    }

    #[test]
    fn reg0_odd_prg_page_aligns_to_even_pair() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // page = 5 (odd) → aligned pair: lower=4, upper=5
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "lower window should align to bank 4"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "upper window should be bank 5");
    }

    // ───────── Register writes – PRG banking (16 KB mode, O=1) ─────────

    #[test]
    fn mode_bit_o_set_maps_same_page_to_both_windows() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // O=1 (bit 5 set), page=3 → both windows = bank 3
        mapper.write_prg(0x8000, 0x23); // 0x20 = O bit, 0x03 = page
        assert_eq!(mapper.read_prg(0x8000), 3, "lower window should be bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "upper window should also be bank 3"
        );
    }

    // ───────── Register writes – high PRG bit via reg1 ─────────

    #[test]
    fn reg1_high_bit_adds_to_prg_page() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // reg0: O=1 (bit5), page bits 4:0 = 1 → page without H = 0x21 = 33
        // reg1: H=1 → adds bit 6 → page = 0x21 | 0x40 = 0x61... wait:
        // reg0 = 0x21: bits4:0=1, bit5=1 (mode), bit6=0 (mirror), bit7=0
        // reg1 = 0x01: H=1
        // page = (reg0 & 0x1F) | (reset_toggle << 5) | ((reg1 & 0x01) << 6)
        //       = (0x21 & 0x1F) | 0 | (0x01 << 6)
        //       = 0x01 | 0x40 = 0x41 = 65... but we only have 48 banks (0..47)
        // Let's use page bits such that result stays in range:
        // reg0: O=1, page bits 4:0 = 0x01 → raw=0x21
        // reg1: H=1 → final page = 0x01 | 0x40 = 0x41 = 65, wraps in 48: 65%48=17
        // Instead, use no H bit variant first, then set H to check offset
        // Better test: O=1, page=2, no H bit → bank 2; then set H → bank 2+64 wraps 48 = (2|64)%48=66%48=18
        // Let's choose values carefully.
        //   O=1, page bits=2, H=0 → page=2, both windows=2
        //   O=1, page bits=2, H=1 → page=2|64=66, %48=18, both windows=18
        mapper.write_prg(0x8000, 0x22); // O=1, page=2
        mapper.write_prg(0x8001, 0x00); // H=0
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "lower window should be bank 2 (H=0)"
        );
        mapper.write_prg(0x8001, 0x01); // H=1
        // page = 2 | 64 = 66; 66 % 48 = 18
        assert_eq!(
            mapper.read_prg(0x8000),
            18,
            "lower window should be bank 18 (page 66 mod 48) when H=1"
        );
    }

    // ───────── Mirroring ─────────

    #[test]
    fn mirroring_bit6_zero_selects_horizontal() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x00); // bit6 = 0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_bit6_one_selects_vertical() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x40); // bit6 = 1 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x40); // Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x00); // Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ───────── Soft reset – reset_toggle behaviour ─────────

    #[test]
    fn soft_reset_clears_registers() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x04); // select page 4
        mapper.reset();
        // After reset, regs cleared → page = reset_toggle<<5 | 0 | 0 = 1<<5 = 32
        // 32 KB mode (O=0), lower=bank32, upper=bank33
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "after first soft reset, lower window should reflect reset_toggle=1 at bank 32"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            33,
            "after first soft reset, upper window should be bank 33"
    fn hard_reset_forces_reset_toggle_and_clears_registers() {
        // Put the existing mapper into a non-default state:
        // - reset_toggle = 1 (after a soft reset)
        // - reg0 and reg1 = non-zero (after writes)
    #[test]
        // reset_toggle starts at 0
        // First soft reset: this should toggle reset_toggle and clear the registers.
        mapper.reset();

        // Now write non-zero values into both registers via the normal PRG interface.
        mapper.write_prg(0x8000, 0xA5);
        mapper.write_prg(0x8001, 0x01);

        // Sanity-check that we're not in the power-on default state anymore.
        assert_ne!(mapper.reset_toggle, 0, "reset_toggle should be non-zero after soft reset");
        assert_ne!(mapper.reg0, 0, "reg0 should be non-zero after writes");
        assert_ne!(mapper.reg1, 0, "reg1 should be non-zero after writes");

        // Emulate the emulator's hard reset path:
        // Bus::reset_cartridge calls initialize_ram(RamInitMode::Zero) then reset()
        // on the existing mapper instance.
        mapper.initialize_ram(RamInitMode::Zero);
        mapper.reset();

        // Hard reset must force reset_toggle back to 0 and clear both registers.
        mapper.reset();
            mapper.reset_toggle, 0,
            "hard reset should force reset_toggle back to 0"
        );
        assert_eq!(
            mapper.reg0, 0,
            "hard reset should clear reg0"
        );
        assert_eq!(
            mapper.reg1, 0,
            "hard reset should clear reg1"
            32,
            "1st soft reset → bank 32 (reset_toggle=1)"
        );

        mapper.reset();
        // reset_toggle = 0 again → page = 0; lower=0, upper=1
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "2nd soft reset → bank 0 (reset_toggle=0)"
        );
    }

    #[test]
    fn soft_reset_does_not_affect_reset_toggle_on_hard_reset_path() {
        // Simulate that after multiple soft resets the toggle is 1,
        // then we create a fresh mapper (hard reset) and toggle must be 0.
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        mapper.reset(); // toggle → 1
        assert_eq!(mapper.read_prg(0x8000), 32);

        // New mapper instance = hard reset, toggle starts at 0
        let fresh = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(
            fresh.read_prg(0x8000),
            0,
            "hard reset (new instance) starts with reset_toggle=0"
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

    // ───────── reg0 bit 7 is unused (unlike Mapper 226) ─────────

    #[test]
    fn reg0_bit7_does_not_affect_prg_page() {
        let mut mapper = make_mapper(banked_data(16 * 1024, PRG_BANKS));
        // O=1, page bits 4:0 = 2, bit7 = 0 → page = 2
        mapper.write_prg(0x8000, 0x22);
        let bank_without_bit7 = mapper.read_prg(0x8000);

        // O=1, page bits 4:0 = 2, bit7 = 1 → page must still = 2 (bit7 not used)
        mapper.write_prg(0x8000, 0xA2); // 0x80 | 0x22
        let bank_with_bit7 = mapper.read_prg(0x8000);

        assert_eq!(
            bank_without_bit7, bank_with_bit7,
            "reg0 bit7 should have no effect on PRG page in mapper 233"
        );
    }

    // ───────── Save state ─────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = make_mapper(prg.clone());
        mapper.reset(); // reset_toggle = 1
        mapper.write_prg(0x8000, 0x22); // O=1, page=2, H=bit from reg1
        mapper.write_prg(0x8001, 0x00); // H=0

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper(prg);
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            // page = (2 & 0x1F) | (1 << 5) | 0 = 2 | 32 = 34; both windows = 34 (O=1)
            34,
            "restored mapper should show bank 34 (page 34 with reset_toggle=1)"
        );
        assert_eq!(restored.read_prg(0xC000), 34);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
