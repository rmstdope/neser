//! Mapper 319 – HP898F board (4-in-1 multicart)
//!
//! ## Specifications
//!
//! - Primary reference: Mesen2 `Hp898f.h`
//! - NesDev: <https://www.nesdev.org/wiki/NES_2.0_Mapper_319> (inaccessible; Cloudflare 403)
//!
//! ## Overview
//!
//! The HP898F board is used by several 4-in-1 multicarts. It has two 16 KiB PRG
//! windows (`$8000–$BFFF` and `$C000–$FFFF`) and one 8 KiB CHR window
//! (`$0000–$1FFF`). Mirroring is software-controlled.
//!
//! ## Register Map
//!
//! Registers are written when `(addr & 0x6000) == 0x6000` (i.e., address bits 13
//! and 14 are both set). This matches the ranges `$6000–$7FFF` and `$E000–$FFFF`.
//! The register index is selected by bit 2 of the address: `(addr & 0x04) >> 2`.
//!
//! ### Register 0 (CHR control, selected when addr bit 2 = 0)
//!
//! | Bits | Field      | Description                              |
//! |------|------------|------------------------------------------|
//! | 6:4  | CHR bank   | Upper bits of the CHR bank select        |
//! | 1    | CHR mask 1 | When set, clears bit 1 of the CHR bank   |
//! | 0    | CHR mask 0 | When set, clears bit 2 of the CHR bank   |
//!
//! CHR bank selection:
//! ```text
//! chr_bank = (reg0 >> 4) & 0x07
//! chr_mask = ((reg0 & 0x01) << 2) | (reg0 & 0x02)
//! chr_page = chr_bank & !chr_mask
//! ```
//!
//! ### Register 1 (PRG/mirroring control, selected when addr bit 2 = 1)
//!
//! | Bits | Field      | Description                                  |
//! |------|------------|----------------------------------------------|
//! | 7    | Mirroring  | 1 = Vertical, 0 = Horizontal                 |
//! | 6    | PRG mask   | When set, enables 32 KiB banking             |
//! | 5:3  | PRG bank   | 3-bit PRG bank register                      |
//!
//! PRG bank selection:
//! ```text
//! prg_reg  = (reg1 >> 3) & 0x07
//! prg_mask = (reg1 >> 4) & 0x04   // bit 6 of reg1, moved to bit 2 position
//! page0    = prg_reg & !prg_mask  // $8000–$BFFF
//! page1    = prg_reg | prg_mask   // $C000–$FFFF
//! ```
//!
//! When bit 6 = 0: `page0 == page1 == prg_reg` (same 16 KiB bank mapped twice).
//! When bit 6 = 1: bit 2 of the page is cleared/set to form a 32 KiB pair.
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 319;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const REGISTERS_SNAPSHOT_LEN: usize = 2;

pub struct Mapper319 {
    base: BaseMapper,
    regs: [u8; 2],
}

impl Mapper319 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);
        let mut mapper = Self { base, regs: [0; 2] };
        mapper.apply_banking();
        mapper
    }

    fn is_register_write(addr: u16) -> bool {
        (addr & 0x6000) == 0x6000
    }

    fn register_index(addr: u16) -> usize {
        ((addr & 0x04) >> 2) as usize
    }

    fn chr_page(&self) -> i16 {
        let bank = (self.regs[0] >> 4) & 0x07;
        let mask = ((self.regs[0] & 0x01) << 2) | (self.regs[0] & 0x02);
        (bank & !mask) as i16
    }

    fn prg_pages(&self) -> (i16, i16) {
        let reg = (self.regs[1] >> 3) & 0x07;
        let mask = (self.regs[1] >> 4) & 0x04;
        ((reg & !mask) as i16, (reg | mask) as i16)
    }

    fn resolve_mirroring(&self) -> NametableLayout {
        if self.regs[1] & 0x80 != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        }
    }

    fn apply_banking(&mut self) {
        self.base.select_chr_page(0, self.chr_page());
        let (page0, page1) = self.prg_pages();
        self.base.select_prg_page(0, page0);
        self.base.select_prg_page(1, page1);
        let mirroring = self.resolve_mirroring();
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper319 {
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
        if Self::is_register_write(addr) {
            self.regs[Self::register_index(addr)] = value;
            self.apply_banking();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.regs = [data[0], data[1]];
        self.apply_banking();
    }

    fn reset(&mut self) {
        self.regs = [0; 2];
        self.apply_banking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 11; // 11 × 16 KiB = 176 KiB
    const CHR_BANKS: usize = 11; // 11 × 8 KiB = 88 KiB

    fn make_mapper() -> Mapper319 {
        Mapper319::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_319_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 319 must be registered in the factory"
        );
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_both_prg_pages_map_to_bank_0() {
        let mapper = make_mapper();
        // reg1 = 0: prg_reg=0, prg_mask=0 → page0=0, page1=0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$0000 must read CHR bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── PRG banking ──────────────────────────────────────────────────────────

    #[test]
    fn prg_reg1_bit6_0_both_pages_same_bank() {
        let mut mapper = make_mapper();
        // reg1 = 0b00_101_000 = 0x28 → prg_reg=(0x28>>3)&7=5, prg_mask=0 → page0=5, page1=5
        mapper.write_prg(0x6004, 0x28);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 must map to bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 must map to bank 5 (same as $8000 when bit6=0)"
        );
    }

    #[test]
    fn prg_reg1_bit6_1_enables_32kb_banking() {
        let mut mapper = make_mapper();
        // reg1 = 0b01_100_000 = 0x60
        // prg_reg = (0x60>>3)&7 = 12&7 = 4, prg_mask=(0x60>>4)&4=6&4=4
        // page0 = 4 & !4 = 4 & 0b...11111011 = 0, page1 = 4 | 4 = 4
        mapper.write_prg(0x6004, 0x60);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to bank 0 (bit2 cleared)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must map to bank 4 (bit2 set)"
        );
    }

    #[test]
    fn prg_reg1_bit6_1_with_prg_reg_7() {
        let mut mapper = make_mapper();
        // reg1 = 0b01_111_000 = 0x78
        // prg_reg = (0x78>>3)&7 = 15&7 = 7, prg_mask=(0x78>>4)&4=7&4=4
        // page0 = 7 & !4 = 7 & 3 = 3, page1 = 7 | 4 = 7
        mapper.write_prg(0x6004, 0x78);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 must map to bank 3 (bit2 cleared from 7)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must map to bank 7 (bit2 set)"
        );
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_select_via_reg0_bits_6_4() {
        let mut mapper = make_mapper();
        // reg0 = 0x50 = 0b01010000: bits 6:4 = 0b101 = 5, bit0=0, bit1=0 → mask=0
        // chr_page = 5 & !0 = 5
        mapper.write_prg(0x6000, 0x50);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR must map to bank 5");
    }

    #[test]
    fn chr_reg0_bit0_clears_chr_bit2() {
        let mut mapper = make_mapper();
        // reg0 = 0x71 = 0b01110001: chr_bank=7, bit0=1 → chr_mask=4
        // chr_page = 7 & !4 = 7 & 3 = 3
        mapper.write_prg(0x6000, 0x71);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bit0 set must clear bit2 of chr_bank"
        );
    }

    #[test]
    fn chr_reg0_bit1_clears_chr_bit1() {
        let mut mapper = make_mapper();
        // reg0 = 0x72 = 0b01110010: chr_bank=7, bit1=1 → chr_mask=2
        // chr_page = 7 & !2 = 7 & 5 = 5
        mapper.write_prg(0x6000, 0x72);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bit1 set must clear bit1 of chr_bank"
        );
    }

    #[test]
    fn chr_reg0_bits_0_and_1_both_set() {
        let mut mapper = make_mapper();
        // reg0 = 0x73 = 0b01110011: chr_bank=7, bit0=1, bit1=1 → chr_mask=(1<<2)|2=6
        // chr_page = 7 & !6 = 7 & 1 = 1
        mapper.write_prg(0x6000, 0x73);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bits 0 and 1 both set must clear bits 2 and 1 of chr_bank"
        );
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_vertical_when_reg1_bit7_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6004, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_when_reg1_bit7_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6004, 0x80); // set vertical first
        mapper.write_prg(0x6004, 0x00); // clear bit7 → horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Write address decoding ───────────────────────────────────────────────

    #[test]
    fn write_to_6000_selects_reg0() {
        let mut mapper = make_mapper();
        // addr & 0x6000 == 0x6000 ✓; bit2=0 → reg0
        mapper.write_prg(0x6000, 0x50);
        assert_eq!(mapper.read_chr(0x0000), 5, "write to $6000 must set reg0");
    }

    #[test]
    fn write_to_6004_selects_reg1() {
        let mut mapper = make_mapper();
        // addr & 0x6000 == 0x6000 ✓; bit2=1 → reg1
        mapper.write_prg(0x6004, 0x28);
        assert_eq!(mapper.read_prg(0x8000), 5, "write to $6004 must set reg1");
    }

    #[test]
    fn write_to_e000_selects_reg0() {
        let mut mapper = make_mapper();
        // 0xE000 & 0x6000 = 0x6000 ✓; bit2=0 → reg0
        mapper.write_prg(0xE000, 0x50);
        assert_eq!(mapper.read_chr(0x0000), 5, "write to $E000 must set reg0");
    }

    #[test]
    fn write_to_e004_selects_reg1() {
        let mut mapper = make_mapper();
        // 0xE004 & 0x6000 = 0x6000 ✓; bit2=1 → reg1
        mapper.write_prg(0xE004, 0x28);
        assert_eq!(mapper.read_prg(0x8000), 5, "write to $E004 must set reg1");
    }

    #[test]
    fn writes_to_8000_9fff_are_ignored() {
        let mut mapper = make_mapper();
        // 0x8000 & 0x6000 = 0 ≠ 0x6000 → ignored
        mapper.write_prg(0x8000, 0x50);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "writes to $8000-$9FFF must be ignored"
        );
    }

    #[test]
    fn writes_to_a000_bfff_are_ignored() {
        let mut mapper = make_mapper();
        // 0xA000 & 0x6000 = 0x2000 ≠ 0x6000 → ignored
        mapper.write_prg(0xA000, 0x50);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "writes to $A000-$BFFF must be ignored"
        );
    }

    #[test]
    fn writes_to_c000_dfff_are_ignored() {
        let mut mapper = make_mapper();
        // 0xC000 & 0x6000 = 0x4000 ≠ 0x6000 → ignored
        mapper.write_prg(0xC000, 0x50);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "writes to $C000-$DFFF must be ignored"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_register_state() {
        let mut mapper = make_mapper();
        // Set reg0=0x50 (CHR bank 5), reg1=0x78 (32KB mode, banks 3/7)
        mapper.write_prg(0x6000, 0x50);
        mapper.write_prg(0x6004, 0x78);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            5,
            "snapshot must restore CHR bank 5"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "snapshot must restore PRG page0=3"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            7,
            "snapshot must restore PRG page1=7"
        );
    }

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x70);
        mapper.write_prg(0x6004, 0x78);
        mapper.reset();

        assert_eq!(mapper.read_chr(0x0000), 0, "reset must restore CHR bank 0");
        assert_eq!(mapper.read_prg(0x8000), 0, "reset must restore PRG page0=0");
        assert_eq!(mapper.read_prg(0xC000), 0, "reset must restore PRG page1=0");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── No IRQ ───────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6004, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 319 must never assert IRQ");
    }
}
