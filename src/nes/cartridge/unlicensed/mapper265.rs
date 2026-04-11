//! Mapper 265 – BMC-T-262 (multicart with address/data latches)
//!
//! Specifications:
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_265>
//! - Reference implementation: Mesen2 `Core/NES/Mappers/Unlicensed/T262.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/T262.h>
//!
//! # Hardware overview
//!
//! Used by T-262 multicarts, all of which use 8 KiB CHR-RAM.
//! Any write to `$8000–$FFFF` latches **both** the address bits and the data bits.
//!
//! # Address latch (`$8000–$FFFF`, write — address bits)
//!
//! ```text
//! A~[1.L. ..OO POO. ..MN]
//!        |    || |||    |+- N: 1=Replace PRG A14 (NROM-256 when P=1 also)
//!        |    || |||    +-- M: Nametable mirroring (0=Vertical, 1=Horizontal)
//!        |    ++-|++------- OO_OO: 128 KiB outer PRG-ROM bank at $8000–$FFFF
//!        |       +--------- P: PRG-ROM banking mode
//!        |                   0: Fixed Inner bank 7 at $C000–$FFFF (UNROM-style)
//!        |                   1: Mirrored Inner bank at $C000–$FFFF (NROM-128-style)
//!        +----------------- L: Locking bit — prevents further address latch changes
//! ```
//!
//! The outer bank field uses bits 8, 6, 5 of the write address (3 bits total).
//!
//! # Data latch (`$8000–$FFFF`, write — data bits 2:0)
//!
//! ```text
//! D~.... .PPP
//!         +++- Select 16 KiB inner PRG-ROM bank at $8000–$BFFF
//!              (and at $C000–$FFFF when address latch bit 7 is set)
//! ```
//!
//! The data latch is **always** updated (even when the address is locked).
//!
//! # PRG banking
//!
//! Let `base` = 3-bit outer bank × 8 (selects which group of 8 inner banks to use).
//! Let `bank` = 3-bit inner bank from data latch.
//!
//! `$8000–$BFFF` always maps to 16 KiB page `base | bank`.
//!
//! `$C000–$FFFF`:
//! - Mode = 0 (UNROM-style): fixed to page `base | 7`
//! - Mode = 1 (NROM-128): same as lower half → page `base | bank`
//!
//! # CHR
//!
//! All T-262 multicarts use 8 KiB unbanked CHR-RAM. CHR is not bankable.
//!
//! # Power-on / reset state
//!
//! - Page 0 at `$8000–$BFFF`, page 7 at `$C000–$FFFF`
//! - CHR-RAM at bank 0 (unbanked)
//! - Vertical mirroring
//! - Not locked

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 265;
const PRG_BANK_SIZE: usize = 16 * 1024;

/// Mapper 265 – BMC-T-262
pub struct Mapper265 {
    base: BaseMapper,
    /// Address latch is locked after a write sets bit 13 of the address.
    locked: bool,
    /// Outer 3-bit bank multiplied by 8 (values 0, 8, 16, …, 56).
    outer: u8,
    /// PRG mode: `false` = UNROM (upper window fixed at inner 7),
    ///           `true`  = NROM-128 (upper window mirrors lower).
    mode: bool,
    /// Inner 3-bit bank from the data latch.
    bank: u8,
}

impl Mapper265 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        // CHR-RAM is unbanked; no configure_chr_banking needed.

        let mut mapper = Self {
            base,
            locked: false,
            outer: 0,
            mode: false,
            bank: 0,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let lower = (self.outer | self.bank) as i16;
        let upper = (self.outer | if self.mode { self.bank } else { 7 }) as i16;
        self.base.select_prg_page(0, lower);
        self.base.select_prg_page(1, upper);
    }
}

impl Mapper for Mapper265 {
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
        if addr < 0x8000 {
            return;
        }
        if !self.locked {
            // outer = addr[8], addr[6], addr[5] → values 0, 8, 16, …, 56
            self.outer = (((addr & 0x60) >> 2) | ((addr & 0x100) >> 3)) as u8;
            self.mode = addr & 0x80 != 0;
            self.locked = addr & 0x2000 != 0;
            self.base.set_mirroring_hv(addr & 0x02 != 0);
        }
        self.bank = value & 0x07;
        self.apply_banks();
    }

    fn reset(&mut self) {
        self.locked = false;
        self.outer = 0;
        self.mode = false;
        self.bank = 0;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.locked as u8) | ((self.mode as u8) << 1);
        vec![flags, self.outer, self.bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        self.locked = data[0] & 0x01 != 0;
        self.mode = data[0] & 0x02 != 0;
        self.outer = data[1] & 0x38; // mask to valid range (0,8,…,56)
        self.bank = data[2] & 0x07;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 64 × 16 KiB = 1 MB PRG (maximum addressable by this mapper)
    const PRG_BANKS: usize = 64;

    fn make_mapper() -> Mapper265 {
        // Pass empty CHR to use CHR-RAM
        Mapper265::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_265_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 265 must be registered in the factory"
        );
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_lower_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(mapper.read_prg(0xBFFF), 0, "$BFFF → bank 0");
    }

    #[test]
    fn power_on_upper_window_is_bank_7() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 → bank 7 (UNROM fixed)");
        assert_eq!(mapper.read_prg(0xFFFF), 7, "$FFFF → bank 7");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Data latch: inner bank selection ─────────────────────────────────────

    #[test]
    fn data_latch_selects_lower_prg_bank() {
        let mut mapper = make_mapper();
        // Write 0x05 to any $8000 address: bank = 5
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "lower window → bank 5");
    }

    #[test]
    fn upper_window_stays_fixed_at_7_in_unrom_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // inner = 3, mode=0 (UNROM)
        assert_eq!(mapper.read_prg(0xC000), 7, "upper window stays at 7");
    }

    #[test]
    fn data_only_uses_lower_3_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF); // only bits 2:0 → 7
        assert_eq!(mapper.read_prg(0x8000), 7, "bank = 0xFF & 0x07 = 7");
    }

    // ── Mode bit (addr[7]) ────────────────────────────────────────────────────

    #[test]
    fn nrom128_mode_mirrors_lower_to_upper_window() {
        let mut mapper = make_mapper();
        // addr = $8080: bit 7 set → NROM-128 mode; write bank = 3
        mapper.write_prg(0x8080, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3, "lower → bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "upper mirrors lower in NROM-128 mode"
        );
    }

    // ── Address latch: outer bank selection ──────────────────────────────────

    #[test]
    fn outer_bank_addr_bit_5_shifts_base_by_8() {
        let mut mapper = make_mapper();
        // addr = $8020: bit 5 set → outer = ((0x20 & 0x60) >> 2) = 8
        mapper.write_prg(0x8020, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 8, "outer=8, bank=0 → page 8");
        assert_eq!(mapper.read_prg(0xC000), 8 + 7, "upper = outer | 7 = 15");
    }

    #[test]
    fn outer_bank_addr_bit_6_shifts_base_by_16() {
        let mut mapper = make_mapper();
        // addr = $8040: bit 6 set (and mode bit also set) → outer = ((0x40 & 0x60) >> 2) = 16
        // but bit 6 = mode bit too? Let me check: addr = $8040, bit 7 = 0 (mode=false), bit 6 = 1
        // outer = ((0x8040 & 0x60) >> 2) | ((0x8040 & 0x100) >> 3) = (0x40 >> 2) | 0 = 16
        mapper.write_prg(0x8040, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 16, "outer=16, bank=0 → page 16");
    }

    #[test]
    fn outer_bank_addr_bit_8_shifts_base_by_32() {
        let mut mapper = make_mapper();
        // addr = $8100: bit 8 set → outer = ((0 & 0x60) >> 2) | ((0x100 & 0x100) >> 3) = 32
        mapper.write_prg(0x8100, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 32, "outer=32, bank=0 → page 32");
    }

    // ── Locking bit (addr[13]) ────────────────────────────────────────────────

    #[test]
    fn locking_bit_prevents_address_latch_changes() {
        let mut mapper = make_mapper();
        // Write with bit 13 set: locks the address latch
        mapper.write_prg(0xA000, 0x03); // addr[13]=1 → locked=true, bank=3
        // Now try to change outer/mode via address (should be ignored)
        mapper.write_prg(0x8120, 0x03); // outer=40 if unlocked, but must be ignored
        assert_eq!(mapper.read_prg(0x8000), 3, "outer unchanged → page 3");
    }

    #[test]
    fn data_latch_is_still_writable_when_locked() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x03); // lock at bank=3
        mapper.write_prg(0x8000, 0x05); // data latch update (outer=0 ignored)
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "data latch updated to 5 even when locked"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn addr_bit_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn addr_bit_1_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0x00); // set H
        mapper.write_prg(0x8000, 0x00); // clear to V
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable_and_readable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM write/read roundtrip"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_lock_and_restores_power_on_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA020, 0x05); // lock with outer=8, bank=5
        mapper.reset();
        assert!(!mapper.locked, "locked cleared");
        assert_eq!(mapper.read_prg(0x8000), 0, "lower → bank 0");
        assert_eq!(mapper.read_prg(0xC000), 7, "upper → bank 7 (UNROM default)");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8020, 0x03); // outer=8, bank=3
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "lower match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "upper match"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8020, 0x03);
        let prev = mapper.read_prg(0x8000);
        mapper.restore_registers(&[0x00]);
        assert_eq!(mapper.read_prg(0x8000), prev, "state unchanged");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(
            !caps.has_chr_banking,
            "no CHR banking (CHR-RAM is unbanked)"
        );
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(!caps.has_irq, "no IRQ");
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
