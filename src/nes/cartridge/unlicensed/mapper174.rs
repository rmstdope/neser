//! Mapper 174 – NTDEC 5-in-1 multicart
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_174>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Ntdec/Mapper174.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 174 is used by the NTDec 5-in-1 multicart (128 KiB PRG, 64 KiB CHR).
//! It is functionally similar to [Mapper 58](https://www.nesdev.org/wiki/INES_Mapper_058)
//! but with different bit placements in the address register.
//!
//! ## Register
//!
//! A write to any address in `$8000–$FFFF` latches the full address into the
//! bank register.  **The data byte is ignored; banking is determined by address
//! lines only.**
//!
//! ```text
//! A~[1... .... OPPP CCCM]
//!                         M – Nametable mirroring: 0=Vertical, 1=Horizontal
//!                    +++ – C[2:0]: 8 KiB CHR-ROM bank select (bits A3..A1)
//!               +++ – P[2:0]: PRG bank index (bits A6..A4)
//!              + – O: PRG banking mode (bit A7)
//!                      0: NROM-128 – both 16 KiB slots map to same bank P
//!                      1: BNROM-style – 32 KiB bank = P[2:1] (P[0] ignored),
//!                                       PRG A14 from CPU A14
//! ```
//!
//! ## PRG Banking
//!
//! * O=0 (16 KiB mode): both `$8000–$BFFF` and `$C000–$FFFF` map to the same
//!   16 KiB PRG-ROM bank selected by P.
//! * O=1 (32 KiB mode): the two 16 KiB slots are selected by `P & 0b110` and
//!   `(P & 0b110) | 1` respectively (i.e., an aligned 32 KiB bank whose index
//!   is `P >> 1`).
//!
//! ## CHR Banking
//!
//! 8 KiB CHR-ROM bank C is mapped to `$0000–$1FFF`.
//!
//! ## Mirroring
//!
//! Bit A0 of the write address: 0 → Vertical, 1 → Horizontal.
//!
//! No PRG-RAM, no IRQ, no expansion audio.  Power-on/reset state: all bits zero.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 174;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 174 – NTDEC 5-in-1 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper174 {
    base: BaseMapper,
    /// Latched address bus value (full address from last write to $8000–$FFFF).
    latch: u16,
}

impl Mapper174 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, latch: 0 };
        mapper.apply_latch(0);
        mapper
    }

    /// Apply the latched address bus value to the bank registers.
    fn apply_latch(&mut self, addr: u16) {
        let prg_bank = ((addr >> 4) & 0x07) as i16; // bits A6..A4
        let chr_bank = ((addr >> 1) & 0x07) as i16; // bits A3..A1
        let mirroring_h = (addr & 0x01) != 0; // bit A0
        let mode_32k = (addr & 0x80) != 0; // bit A7 = O

        if mode_32k {
            // 32 KiB mode: align to even boundary
            self.base.select_prg_page(0, prg_bank & !1);
            self.base.select_prg_page(1, prg_bank | 1);
        } else {
            // 16 KiB NROM-128 mode: both slots = same bank
            self.base.select_prg_page(0, prg_bank);
            self.base.select_prg_page(1, prg_bank);
        }

        self.base.select_chr_page(0, chr_bank);
        self.base.set_mirroring_hv(mirroring_h);
        self.latch = addr;
    }
}

impl Mapper for Mapper174 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if addr >= 0x8000 {
            self.apply_latch(addr);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![(self.latch & 0xFF) as u8, ((self.latch >> 8) & 0xFF) as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let latch = (data[0] as u16) | ((data[1] as u16) << 8);
            self.apply_latch(latch);
        }
    }

    fn reset(&mut self) {
        self.apply_latch(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to avoid false positives from modulo wrapping.
    const PRG_BANKS: usize = 5;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper174 {
        Mapper174::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_174_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 174 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_is_0() {
        let mapper = make_mapper();
        // latch = 0 → O=0 (16KB mode), P=0 → both slots bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must be Vertical (M=0)"
        );
    }

    // ── PRG banking – 16 KiB mode (O=0) ─────────────────────────────────────

    #[test]
    fn nrom128_mode_both_slots_same_bank() {
        let mut mapper = make_mapper();
        // addr = 0x8010: O=0, P=1 (bits A6..A4 = 0b001), C=0, M=0
        mapper.write_prg(0x8010, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must map to bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must also map to bank 1 (NROM-128 mirror)"
        );
    }

    #[test]
    fn nrom128_mode_bank_2() {
        let mut mapper = make_mapper();
        // addr = 0x8020: O=0, P=2 (bits A6..A4 = 0b010), M=0
        mapper.write_prg(0x8020, 0);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 2);
    }

    // ── PRG banking – 32 KiB mode (O=1) ─────────────────────────────────────

    #[test]
    fn bnrom_mode_32k_aligned_bank() {
        let mut mapper = make_mapper();
        // addr = 0x8080: O=1 (bit 7), P=0 → 32KB bank 0 (slots 0 and 1)
        mapper.write_prg(0x8080, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be low half of 32KB bank"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must be high half of 32KB bank"
        );
    }

    #[test]
    fn bnrom_mode_32k_odd_bank_aligned_down() {
        let mut mapper = make_mapper();
        // addr = 0x8090: O=1, P=1 → aligned down to P=0, slots = 0,1
        mapper.write_prg(0x8090, 0);
        assert_eq!(mapper.read_prg(0x8000), 0, "Odd P aligned to P & !1 = 0");
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn bnrom_mode_32k_bank_2() {
        let mut mapper = make_mapper();
        // addr = 0x80A0: O=1, P=2 → slots = 2,3
        mapper.write_prg(0x80A0, 0);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_by_bits_3_to_1() {
        let mut mapper = make_mapper();
        // addr = 0x8002: C=1 (bit A1 = 1, A3:A1 = 0b001)
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank 1");
        assert_eq!(mapper.read_chr(0x1FFF), 1, "CHR bank 1 covers full 8KB");
    }

    #[test]
    fn chr_bank_3_selects_bank_3() {
        let mut mapper = make_mapper();
        // addr = 0x8006: bits A3:A1 = 0b011 = 3
        mapper.write_prg(0x8006, 0);
        assert_eq!(mapper.read_chr(0x0000), 3);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn bit_a0_0_selects_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0); // A0=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn bit_a0_1_selects_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8011, 0); // A0=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Data byte ignored ─────────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0x00); // P=1
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.write_prg(0x8010, 0xFF); // same address, different data
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Data byte must be ignored; address determines banking"
        );
    }

    // ── Writes below $8000 are ignored ───────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_change_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Write below $8000 must not change PRG bank"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8030, 0); // P=3, C=1, M=0
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must reset to 0");
        assert_eq!(mapper.read_prg(0xC000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must reset to 0");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // addr = 0x8031: O=0, P=3, C=1, M=1
        mapper.write_prg(0x8031, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG $8000 must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored PRG $C000 must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mirroring must match"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0);
        assert!(!mapper.irq_pending(), "Mapper 174 must never assert IRQ");
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper174::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB);
    }
}
