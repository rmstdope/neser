//! Mapper 293 – NewStar 12-in-1 / 76-in-1 Multicart
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/NES_2.0_Mapper_293>
//!
//! Hardware summary:
//! - PCB: NewStar 12-in-1 and 76-in-1 multicarts
//! - PRG-ROM: Up to 1 MiB (8 outer × 8 inner × 16 KiB)
//! - CHR: 8 KiB unbanked CHR-RAM
//! - Mirroring: Programmable (H/V) via register 2 bit 7
//!
//! Two banking registers selected by address bits 13 and 14:
//!
//! Register 1 ($8000–$9FFF, $C000–$DFFF) – written when bit 13 of address = 0:
//! ```text
//! 7654 3210
//! ---------
//! .... PIII
//!      |+++- Inner 16 KiB PRG-ROM bank select (bits 0–2)
//!      +---- PRG banking mode bit 1 (bit 3)
//! ```
//!
//! Register 2 ($8000–$BFFF) – written when bit 14 of address = 0:
//! ```text
//! 7654 3210
//! ---------
//! MP10 ...2
//! ||++----+- Outer 128 KiB PRG-ROM bank bits (unusual distribution – see below)
//! |+-------- PRG banking mode bit 0 (bit 6)
//! +--------- Nametable mirroring: 0=Vertical, 1=Horizontal (bit 7)
//! ```
//!
//! The two registers' address ranges **overlap** at $8000–$9FFF: a write to
//! that range updates both registers simultaneously.
//!
//! Outer bank bit distribution in register 2:
//! - outer[0] = reg2 bit 4
//! - outer[1] = reg2 bit 5
//! - outer[2] = reg2 bit 0
//!
//! PRG banking modes (mode = {reg1[3], reg2[6]}):
//! - 0 (UNROM):     $8000–$BFFF = inner bank; $C000–$FFFF = inner bank #7
//! - 1:             $8000–$BFFF = inner bank AND $FE (even); $C000–$FFFF = inner bank #7
//! - 2 (NROM-128):  $8000–$BFFF = inner bank; $C000–$FFFF = mirror of $8000–$BFFF
//! - 3 (NROM-256):  $8000–$FFFF = 32 KiB bank (inner bit 0 ignored)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 293 – NewStar 12-in-1 / 76-in-1 Multicart
pub struct Mapper293 {
    base: BaseMapper,
    /// Register 1: written when addr bit 13 = 0 ($8000–$9FFF or $C000–$DFFF)
    reg1: u8,
    /// Register 2: written when addr bit 14 = 0 ($8000–$BFFF)
    reg2: u8,
}

impl Mapper293 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        let mut s = Self {
            base,
            reg1: 0,
            reg2: 0,
        };
        s.update_banks();
        s
    }

    /// Extract the 3-bit outer bank from register 2.
    ///
    /// outer[0] = reg2[4], outer[1] = reg2[5], outer[2] = reg2[0]
    fn outer_bank(&self) -> usize {
        let bit0 = ((self.reg2 >> 4) & 0x01) as usize;
        let bit1 = ((self.reg2 >> 5) & 0x01) as usize;
        let bit2 = (self.reg2 & 0x01) as usize;
        (bit2 << 2) | (bit1 << 1) | bit0
    }

    /// Extract the 3-bit inner bank from register 1 (bits 0–2).
    fn inner_bank(&self) -> usize {
        (self.reg1 & 0x07) as usize
    }

    /// Extract the 2-bit PRG banking mode: {reg1[3], reg2[6]}.
    fn mode(&self) -> u8 {
        let bit1 = (self.reg1 >> 3) & 0x01;
        let bit0 = (self.reg2 >> 6) & 0x01;
        (bit1 << 1) | bit0
    }

    /// Compute the absolute 16 KiB bank index for PRG slot `slot` (0=$8000, 1=$C000).
    fn prg_bank_for_slot(&self, slot: usize) -> i16 {
        let outer = self.outer_bank();
        let inner = self.inner_bank();
        let base = (outer << 3) as i16;

        match self.mode() {
            0 => {
                // UNROM: slot 0 = inner, slot 1 = last inner (#7)
                if slot == 0 {
                    base | inner as i16
                } else {
                    base | 7
                }
            }
            1 => {
                // Even inner bank for slot 0, last inner (#7) fixed for slot 1
                if slot == 0 {
                    base | (inner as i16 & !1)
                } else {
                    base | 7
                }
            }
            2 => {
                // NROM-128: both slots use the same inner bank (mirror)
                base | inner as i16
            }
            3 => {
                // NROM-256: 32 KiB block (inner bit 0 ignored)
                let aligned = inner as i16 & !1;
                base | aligned | slot as i16
            }
            _ => unreachable!(),
        }
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank_for_slot(0));
        self.base.select_prg_page(1, self.prg_bank_for_slot(1));

        self.base.set_mirroring_hv((self.reg2 & 0x80) != 0);
    }
}

impl Mapper for Mapper293 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        // Register 1 selected when bit 13 of address = 0
        if (addr & 0x2000) == 0 {
            self.reg1 = value;
        }
        // Register 2 selected when bit 14 of address = 0
        if (addr & 0x4000) == 0 {
            self.reg2 = value;
        }
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg1, self.reg2]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.reg1 = data[0];
            self.reg2 = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg1 = 0;
        self.reg2 = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper293 {
        // 8 outer × 8 inner × 16 KiB = 1 MiB PRG-ROM; 8 KiB CHR-RAM (empty vec → RAM)
        let prg = banked_data(16 * 1024, 64);
        let chr = vec![]; // empty = CHR-RAM
        Mapper293::new(MapperContext::new_for_test(
            293,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_293_is_registered() {
        let prg = banked_data(16 * 1024, 64);
        let chr = vec![]; // empty = CHR-RAM
        let result = create_mapper(MapperContext::new_for_test(
            293,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 293 must be registered");
    }

    // --- Default state ---

    #[test]
    fn default_prg_mode0_unrom_inner0_outer0() {
        let mapper = make_mapper();
        // Mode 0 (UNROM): $8000 = inner 0, $C000 = inner 7
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should be bank 7 (last in outer 0)"
        );
    }

    #[test]
    fn default_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- Register selection (overlapping addresses) ---

    #[test]
    fn write_8000_to_9fff_updates_both_registers() {
        let mut mapper = make_mapper();
        // Writing 0x80 to $8000 updates both registers to 0x80.
        // reg2[7]=1 selects horizontal mirroring, and reg1[2:0]=0 keeps inner bank 0.
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.inner_bank(), 0); // reg1[2:0]=0
    }

    #[test]
    fn write_a000_to_bfff_updates_only_reg2() {
        let mut mapper = make_mapper();
        // Set reg1 to something known first by writing to $8000
        mapper.write_prg(0x8000, 0x05); // reg1[2:0]=5, reg2=0x05
        // Now write to $A000 (bit 13=1, bit 14=0): only reg2 should update
        mapper.write_prg(0xA000, 0x80); // reg2=0x80 → horizontal
        assert_eq!(mapper.reg1, 0x05, "reg1 must not change from $A000 write");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_c000_to_dfff_updates_only_reg1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x80); // reg1=0x80, reg2=0x80 (horiz)
        // Now write to $C000 (bit 13=0, bit 14=1): only reg1 should update
        mapper.write_prg(0xC000, 0x01); // reg1=0x01
        assert_eq!(mapper.reg2, 0x80, "reg2 must not change from $C000 write");
        assert_eq!(mapper.inner_bank(), 1, "inner bank from reg1[2:0]=1");
    }

    #[test]
    fn write_e000_to_ffff_updates_neither_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // reg1=3, reg2=3
        mapper.write_prg(0xE000, 0xFF); // bit13=1, bit14=1 → neither register
        assert_eq!(mapper.reg1, 0x03);
        assert_eq!(mapper.reg2, 0x03);
    }

    // --- Inner bank selection ---

    #[test]
    fn inner_bank_select_mode0() {
        let mut mapper = make_mapper();
        // Mode 0: $8000 = inner bank, $C000 = inner 7
        // Write to $C000 (only reg1): inner bank = 3
        mapper.write_prg(0xC000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    // --- Outer bank selection ---

    #[test]
    fn outer_bank_selects_128kb_region() {
        let mut mapper = make_mapper();
        // Set outer bank = 1 via reg2 bits (outer[0]=reg2[4]=1, others=0)
        // outer[0]=1 means reg2[4]=1 → reg2 = 0x10
        // Write to $A000 to update only reg2 (first ensure reg1 is known state from reset)
        mapper.write_prg(0xA000, 0x10); // outer bank = 1
        // Mode 0 by default (reg1=0, reg2[6]=0, reg1[3]=0 → mode=0)
        // inner bank = 0 (reg1=0), outer bank = 1
        // effective bank at $8000 = (1 << 3) | 0 = 8
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 should be bank 8 (outer=1, inner=0)"
        );
        // $C000 = last inner in outer = (1 << 3) | 7 = 15
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "$C000 should be bank 15 (outer=1, inner#7)"
        );
    }

    #[test]
    fn outer_bank_bit2_uses_reg2_bit0() {
        let mut mapper = make_mapper();
        // outer[2]=1 → reg2[0]=1 → reg2 = 0x01 → outer = (1<<2)|(0<<1)|0 = 4
        mapper.write_prg(0xA000, 0x01); // outer = 4
        // inner=0, mode=0 → $8000 = (4<<3)|0 = 32
        assert_eq!(mapper.read_prg(0x8000), 32);
        assert_eq!(mapper.read_prg(0xC000), 39); // (4<<3)|7 = 39
    }

    // --- PRG banking modes ---

    #[test]
    fn mode1_even_inner_bank() {
        let mut mapper = make_mapper();
        // Mode 1: mode bit0 = reg2[6] = 1, mode bit1 = reg1[3] = 0
        // Write to $A000 to set reg2 with bit6=1: reg2 = 0x40 → outer=0, mode=1
        mapper.write_prg(0xA000, 0x40);
        // Inner bank = 3 (odd), mode 1 forces even → inner & $FE = 2
        mapper.write_prg(0xC000, 0x03); // reg1[2:0] = 3, reg1[3] = 0
        assert_eq!(mapper.mode(), 1);
        assert_eq!(mapper.read_prg(0x8000), 2, "mode1: inner 3 AND $FE = 2");
        assert_eq!(mapper.read_prg(0xC000), 7, "mode1: $C000 fixed at inner #7");
    }

    #[test]
    fn mode2_nrom128_mirror() {
        let mut mapper = make_mapper();
        // Mode 2: mode bit1 = reg1[3] = 1, mode bit0 = reg2[6] = 0
        // Write $C000 to set reg1 with bit3=1: reg1 = 0x08 → inner=0, mode_bit1=1
        mapper.write_prg(0xC000, 0x08);
        // reg2[6]=0 → mode_bit0=0 → mode = 2
        assert_eq!(mapper.mode(), 2);
        // Both slots use same inner bank (mirror)
        assert_eq!(mapper.read_prg(0x8000), mapper.read_prg(0xC000));
        // Set inner to 5
        mapper.write_prg(0xC000, 0x0D); // 0x0D = 0000_1101: inner=5, mode_bit1=1
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 5, "mode2: $C000 mirrors $8000");
    }

    #[test]
    fn mode3_nrom256_32kb() {
        let mut mapper = make_mapper();
        // Mode 3: mode bit1 = reg1[3] = 1, mode bit0 = reg2[6] = 1
        mapper.write_prg(0xA000, 0x40); // reg2[6]=1 → mode_bit0=1
        mapper.write_prg(0xC000, 0x0A); // reg1 = 0x0A = 0000_1010: inner=2, mode_bit1=1
        assert_eq!(mapper.mode(), 3);
        // inner=2 (even) → $8000=2, $C000=3
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);

        // Test with odd inner (bit 0 ignored)
        mapper.write_prg(0xC000, 0x0B); // inner=3, mode_bit1=1; inner & !1 = 2
        assert_eq!(mapper.read_prg(0x8000), 2, "mode3: inner bit 0 ignored");
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_horizontal_when_reg2_bit7_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_vertical_when_reg2_bit7_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- CHR-RAM ---

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        mapper.write_chr(0x1FFF, 0x55);
        assert_eq!(mapper.read_chr(0x1FFF), 0x55);
    }

    // --- Reset ---

    #[test]
    fn reset_restores_defaults() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.reset();
        assert_eq!(mapper.reg1, 0);
        assert_eq!(mapper.reg2, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    // --- Save state ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x40); // reg2: mode_bit0=1, outer=0
        mapper.write_prg(0xC000, 0x0B); // reg1: inner=3, mode_bit1=1
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.reg1, mapper.reg1);
        assert_eq!(r.reg2, mapper.reg2);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }
}
