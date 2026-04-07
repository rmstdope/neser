//! Mapper 204 – Pirate multicart (4-in-1, 64-in-1, 80-in-1, 150-in-1)
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_204.xhtml>
//! - Fallback: Mesen2 `Mapper204.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper204.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` selects banks based on **address
//! lines only** (the data byte is ignored):
//!
//! | Address bits | Function                                         |
//! |--------------|--------------------------------------------------|
//! | A[2:1]       | Bank group select (`bitMask = addr & 0x06`)      |
//! | A0           | Sub-bank select within group                     |
//! | A4           | Nametable mirroring (0: Vertical, 1: Horizontal) |
//!
//! **PRG banking (16 KB pages):**
//! - When `bitMask != 0x06`: both `$8000–$BFFF` and `$C000–$FFFF` map to
//!   `page = bitMask + (addr & 0x01)` (16 KB mirrored mode).
//! - When `bitMask == 0x06`: `$8000–$BFFF` maps to page 6,
//!   `$C000–$FFFF` maps to page 7 (32 KB fixed mode).
//!
//! **CHR banking:** always the same `page` as PRG slot 0, as an 8 KB bank.
//!
//! **Mirroring:** A4=0 → Vertical, A4=1 → Horizontal.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: all banks 0, vertical mirroring, 16 KB mirrored mode.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 204;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 204 – Pirate multicart (4-in-1, 64-in-1, etc.)
///
/// See the module-level documentation for hardware details.
pub struct Mapper204 {
    base: BaseMapper,
    /// PRG slot 0 (and CHR) bank index.
    prg_page0: u8,
    /// PRG slot 1 bank index (equals prg_page0 in mirrored mode, or page0+1 in 32KB mode).
    prg_page1: u8,
    /// Mirroring: false = Vertical, true = Horizontal (A4 of write address).
    mirroring_h: bool,
}

impl Mapper204 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
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
        let mut mapper = Self {
            base,
            prg_page0: 0,
            prg_page1: 0,
            mirroring_h: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_page0 as i16);
        self.base.select_prg_page(1, self.prg_page1 as i16);
        self.base.select_chr_page(0, self.prg_page0 as i16);
        self.base.set_mirroring_hv(self.mirroring_h);
    }
}

impl Mapper for Mapper204 {
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
        if addr < 0x8000 {
            return;
        }
        // Data byte is ignored; banking is determined by address lines only.
        let _ = value;
        let bit_mask = addr & 0x06;
        self.prg_page0 = (bit_mask + if bit_mask == 0x06 { 0 } else { addr & 0x01 }) as u8;
        self.prg_page1 = (bit_mask + if bit_mask == 0x06 { 1 } else { addr & 0x01 }) as u8;
        self.mirroring_h = (addr & 0x10) != 0;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // byte0: prg_page0, byte1: prg_page1, byte2: mirroring flag (0=vertical, 1=horizontal)
        vec![
            self.prg_page0,
            self.prg_page1,
            if self.mirroring_h { 1 } else { 0 },
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_page0 = data[0];
            self.prg_page1 = data[1];
            self.mirroring_h = data[2] != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_page0 = 0;
        self.prg_page1 = 0;
        self.mirroring_h = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 9;
    const CHR_BANKS: usize = 9;

    fn make_mapper() -> Mapper204 {
        Mapper204::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_204_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 204 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_mirrors_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must map to PRG bank 0 at power-on (16 KB mirror)"
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
            "Power-on mirroring must be Vertical (A4=0)"
        );
    }

    // ── 16 KB mirrored mode (bitMask != 0x06) ────────────────────────────────

    #[test]
    fn write_8000_selects_bank_0_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8000: bitMask=0, bit0=0 → page=0; both slots → bank 0
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must be bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must mirror bank 0 (16 KB mode)"
        );
    }

    #[test]
    fn write_8001_selects_bank_1_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8001: bitMask=0, bit0=1 → page=1; both slots → bank 1
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must be bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror bank 1 (16 KB mode)"
        );
    }

    #[test]
    fn write_8002_selects_bank_2_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8002: bitMask=2, bit0=0 → page=2; both slots → bank 2
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 must be bank 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 must mirror bank 2 (16 KB mode)"
        );
    }

    #[test]
    fn write_8003_selects_bank_3_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8003: bitMask=2, bit0=1 → page=3; both slots → bank 3
        mapper.write_prg(0x8003, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 must be bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must mirror bank 3 (16 KB mode)"
        );
    }

    #[test]
    fn write_8004_selects_bank_4_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8004: bitMask=4, bit0=0 → page=4; both slots → bank 4
        mapper.write_prg(0x8004, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 must be bank 4");
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must mirror bank 4 (16 KB mode)"
        );
    }

    #[test]
    fn write_8005_selects_bank_5_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8005: bitMask=4, bit0=1 → page=5; both slots → bank 5
        mapper.write_prg(0x8005, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 must be bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 must mirror bank 5 (16 KB mode)"
        );
    }

    // ── 32 KB mode (bitMask == 0x06, i.e., addr bits [2:1] = 0b11) ───────────

    #[test]
    fn write_8006_selects_banks_6_and_7() {
        let mut mapper = make_mapper();
        // addr=0x8006: bitMask=6, bit0=0 → page0=6, page1=7 (32 KB fixed mode)
        mapper.write_prg(0x8006, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 must be bank 6");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 must be bank 7");
    }

    #[test]
    fn write_8007_also_selects_banks_6_and_7() {
        let mut mapper = make_mapper();
        // addr=0x8007: bitMask=6, bit0=1 → still page0=6, page1=7 (bit0 ignored)
        mapper.write_prg(0x8007, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 must be bank 6");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must be bank 7 (bit0 ignored in 32KB mode)"
        );
    }

    #[test]
    fn banks_6_and_7_differ_in_32kb_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8006, 0);
        assert_ne!(
            mapper.read_prg(0x8000),
            mapper.read_prg(0xC000),
            "$8000 and $C000 must map to different banks in 32KB mode"
        );
    }

    // ── CHR banking ────────────────────────────────────────────────────────────

    #[test]
    fn chr_follows_prg_slot0_bank() {
        let mut mapper = make_mapper();
        // addr=0x8004: page0=4 → CHR bank 4
        mapper.write_prg(0x8004, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR bank must match PRG slot 0 bank"
        );
    }

    #[test]
    fn chr_in_32kb_mode_uses_bank_6() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8006, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            6,
            "CHR must use page0 (bank 6) in 32 KB mode"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 4, "CHR end of window");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn a4_0_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // A4=0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "A4=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn a4_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0); // A4=1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "A4=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_changes_independently_of_bank() {
        let mut mapper = make_mapper();
        // bank 4, horizontal
        mapper.write_prg(0x8014, 0); // bitMask=4, bit0=0, A4=1 → H
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg(0x8000), 4);
        // bank 4, vertical
        mapper.write_prg(0x8004, 0); // bitMask=4, bit0=0, A4=0 → V
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 4);
    }

    // ── Data byte ignored ─────────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8004, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4);
        mapper.write_prg(0x8004, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 4, "Data byte must be ignored");
    }

    // ── Write below $8000 ignored ─────────────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_change_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes below $8000 must not affect PRG bank"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes below $8000 must not affect CHR bank"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 204 must never assert IRQ");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8017, 0); // bank 7 (32KB mode), horizontal
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 must be bank 0 after reset (16 KB mirror)"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips_16kb_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8005, 0); // page0=5, page1=5, V mirroring
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_page0, mapper.prg_page0, "prg_page0 preserved");
        assert_eq!(restored.prg_page1, mapper.prg_page1, "prg_page1 preserved");
        assert_eq!(
            restored.mirroring_h, mapper.mirroring_h,
            "mirroring preserved"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored $8000 matches"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored $C000 matches"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR matches"
        );
    }

    #[test]
    fn registers_snapshot_round_trips_32kb_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8016, 0); // bitMask=6, A4=1 → 32KB mode, horizontal
        assert_eq!(mapper.prg_page0, 6);
        assert_eq!(mapper.prg_page1, 7);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_page0, 6, "prg_page0=6 preserved");
        assert_eq!(restored.prg_page1, 7, "prg_page1=7 preserved");
        assert!(restored.mirroring_h, "horizontal mirroring preserved");
        assert_ne!(
            restored.read_prg(0x8000),
            restored.read_prg(0xC000),
            "32KB mode: $8000 and $C000 must differ"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper204::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
