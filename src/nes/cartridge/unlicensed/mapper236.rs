//! Mapper 236 - Realtec 8031/8155/8099/8106 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_236>
//!
//! Known Limitations:
//! - Solder pad (DIP switch) value is always treated as 0 (no physical switch emulation).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 236;

/// Mapper 236 - Realtec 8031/8155/8099/8106 multicart
///
/// Hardware: Address-latch based PRG/CHR bank switching for multicarts.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_236>
/// - PRG-ROM: 128 KiB (CHR-ROM variant) or 512 KiB (CHR-RAM variant)
/// - CHR: 64 KiB CHR-ROM (standard) or 8 KiB CHR-RAM (800-in-1 variant)
/// - Mirroring: Switchable H/V (controlled by write address bit 5)
///
/// Two address-latched registers, decoded by bit 14 of write address:
///
/// Lower Address Latch ($8000-$BFFF, write) — bit 14 = 0:
///   CHR-ROM variant: bits 2-0 = CHR 8KB bank, bit 5 = mirroring
///   CHR-RAM variant: bits 1-0 = outer PRG A19..A18 group, bit 5 = mirroring
///
/// Upper Address Latch ($C000-$FFFF, write) — bit 14 = 1:
///   Both variants: bits 5-4 = PRG mode, bits 2-0 = inner PRG 16KB bank
///
/// PRG Modes (bits 5-4 of upper latch address):
///   0 (0x00): UNROM — $8000-$BFFF = inner bank, $C000-$FFFF = last bank in group
///   1 (0x10): Solder Pad — UNROM banking + reads mask low 4 address bits (DIP=0)
///   2 (0x20): NROM-256 — 32KB bank = (outer|inner) aligned pair
///   3 (0x30): NROM-128 — both windows fixed to inner bank
pub struct Mapper236 {
    base: BaseMapper,
    /// PRG mode from bits 5-4 of upper latch address (0x00/0x10/0x20/0x30)
    bank_mode: u8,
    /// Outer PRG bank group (CHR-RAM variant only): shifted bits 1-0 of lower latch → bits 4-3
    outer_bank: u8,
    /// Inner PRG bank from bits 2-0 of upper latch address
    prg_reg: u8,
    /// CHR 8KB bank from bits 2-0 of lower latch address (CHR-ROM variant only)
    chr_reg: u8,
    /// True when CHR-ROM is present (standard variant); false for CHR-RAM (800-in-1)
    has_chr_rom: bool,
}

impl Mapper236 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let has_chr_rom = !ctx.chr_rom.is_empty();
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: has_chr_rom,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        if has_chr_rom {
            base.configure_chr_banking(8 * 1024);
        }
        let mut mapper = Self {
            base,
            bank_mode: 0,
            outer_bank: 0,
            prg_reg: 0,
            chr_reg: 0,
            has_chr_rom,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let combined = self.outer_bank | self.prg_reg;
        match self.bank_mode {
            0x00 | 0x10 => {
                // UNROM: lower window = inner bank, upper window = last bank in outer group
                self.base.select_prg_page(0, combined as i16);
                self.base.select_prg_page(1, (self.outer_bank | 7) as i16);
            }
            0x20 => {
                // NROM-256: 32KB bank from aligned pair
                let bank32 = (combined & 0xFE) as i16;
                self.base.select_prg_page(0, bank32);
                self.base.select_prg_page(1, bank32 + 1);
            }
            0x30 => {
                // NROM-128: both windows = same bank
                self.base.select_prg_page(0, combined as i16);
                self.base.select_prg_page(1, combined as i16);
            }
            _ => {}
        }
        if self.has_chr_rom {
            self.base.select_chr_page(0, self.chr_reg as i16);
        }
    }
}

impl Mapper for Mapper236 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.base().try_read_prg_ram(addr) {
            return value;
        }
        if (0x8000..=0xFFFF).contains(&addr) && self.bank_mode == 0x10 {
            // Solder pad mode: force low 4 address bits to DIP value (always 0)
            return self.base.read_prg_rom(addr & 0xFFF0);
        }
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x8000..=0xBFFF => {
                // Lower address latch
                self.base.set_mirroring_hv(addr & 0x20 != 0);
                if self.has_chr_rom {
                    self.chr_reg = (addr & 0x07) as u8;
                } else {
                    self.outer_bank = ((addr & 0x03) as u8) << 3;
                }
                self.apply_banks();
            }
            0xC000..=0xFFFF => {
                // Upper address latch
                self.bank_mode = (addr & 0x30) as u8;
                self.prg_reg = (addr & 0x07) as u8;
                self.apply_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.bank_mode);
        snap.push(self.outer_bank);
        snap.push(self.prg_reg);
        snap.push(self.chr_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected_banking_len = self.base.banking_snapshot().len();
        if data.len() >= expected_banking_len + 4 {
            self.base.restore_banking(&data[..expected_banking_len]);
            self.bank_mode = data[expected_banking_len];
            self.outer_bank = data[expected_banking_len + 1];
            self.prg_reg = data[expected_banking_len + 2];
            self.chr_reg = data[expected_banking_len + 3];
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

    const PRG_BANKS: usize = 8; // 8 x 16KB = 128KB (CHR-ROM variant)
    const CHR_BANKS: usize = 8; // 8 x 8KB = 64KB
    const PRG_BANKS_RAM_VARIANT: usize = 32; // 32 x 16KB = 512KB (CHR-RAM variant)

    fn create_chr_rom_mapper(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper236 {
        Mapper236::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    fn create_chr_ram_mapper(prg_rom: Vec<u8>) -> Mapper236 {
        Mapper236::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    // ───────── Factory registration ─────────

    #[test]
    fn mapper_236_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 236 should be registered in factory");
    }

    // ───────── Power-on state (CHR-ROM variant) ─────────

    #[test]
    fn power_on_lower_window_starts_at_bank_0() {
        let mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 window should start at bank 0"
        );
    }

    #[test]
    fn power_on_upper_window_fixed_to_last_bank() {
        let mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // UNROM mode, outer_bank=0, so upper window = bank 7
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 window should start at bank 7 (UNROM last bank)"
        );
    }

    // ───────── Mirroring (lower latch, bit 5) ─────────

    #[test]
    fn lower_latch_bit5_zero_sets_vertical_mirroring() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        mapper.write_prg(0x8000, 0); // bit 5 = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn lower_latch_bit5_one_sets_horizontal_mirroring() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        mapper.write_prg(0x8020, 0); // bit 5 = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_toggles_correctly() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        mapper.write_prg(0x8020, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ───────── CHR bank selection (lower latch, CHR-ROM variant) ─────────

    #[test]
    fn lower_latch_selects_chr_bank() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Write lower latch with chr bits = 3 (addr bits 2-0 = 0b011 = 0x03)
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank 3 should be visible at $0000"
        );
    }

    #[test]
    fn lower_latch_chr_bits_select_all_eight_banks() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        for bank in 0u8..8 {
            mapper.write_prg(0x8000 | (bank as u16), 0);
            assert_eq!(
                mapper.read_chr(0x0000),
                bank,
                "CHR bank {bank} should be selectable"
            );
        }
    }

    // ───────── PRG Mode 0 (UNROM) via upper latch ─────────

    #[test]
    fn upper_latch_mode0_lower_window_switchable() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Mode 0 (bits 5-4 = 00), inner bank = 3 (bits 2-0 = 011)
        mapper.write_prg(0xC003, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 window should be at bank 3"
        );
    }

    #[test]
    fn upper_latch_mode0_upper_window_fixed_to_last_bank() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Mode 0, inner bank = 5 — upper window should remain at bank 7
        mapper.write_prg(0xC005, 0);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 window should be fixed at last bank (7) in UNROM mode"
        );
    }

    // ───────── PRG Mode 2 (NROM-256) via upper latch ─────────

    #[test]
    fn upper_latch_mode2_selects_32kb_bank() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Mode 2 (bits 5-4 = 10 = 0x20), inner bank = 2 (even, so bank pair 2-3)
        mapper.write_prg(0xC022, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 window should be at bank 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 window should be at bank 3"
        );
    }

    #[test]
    fn upper_latch_mode2_aligns_to_even_bank_pairs() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Mode 2, inner bank = 3 (odd) — should align to pair 2-3
        mapper.write_prg(0xC023, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 should align to bank 2 (3 & 0xFE)"
        );
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should be bank 3");
    }

    // ───────── PRG Mode 3 (NROM-128) via upper latch ─────────

    #[test]
    fn upper_latch_mode3_both_windows_same_bank() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        // Mode 3 (bits 5-4 = 11 = 0x30), inner bank = 5
        mapper.write_prg(0xC035, 0);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 window should be bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 window should also be bank 5"
        );
    }

    // ───────── Mode 1 (Solder Pad) ─────────

    #[test]
    fn solder_pad_mode_forces_low_4_address_bits_to_zero() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = create_chr_rom_mapper(prg, chr);
        // Mode 1, bank 0
        mapper.write_prg(0xC010, 0); // bits 5-4 = 0x10, bits 2-0 = 0
        // Read at $8005 — low nibble 5 should be forced to 0, reading $8000
        let at_8000 = mapper.read_prg(0x8000);
        let at_8005 = mapper.read_prg(0x8005);
        assert_eq!(
            at_8005, at_8000,
            "Solder pad mode should force addr bits 3-0 to zero"
        );
    }

    #[test]
    fn solder_pad_mode_reads_differ_from_normal_mode() {
        // Create PRG where offset 0 and offset 5 within bank 0 hold distinct values
        let mut prg = vec![0u8; 16 * 1024 * PRG_BANKS];
        prg[0] = 0xAA; // byte at bank-0 offset 0
        prg[5] = 0x55; // byte at bank-0 offset 5
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = create_chr_rom_mapper(prg, chr);
        // In UNROM mode (mode 0), read at $8005 should return the raw byte at offset 5 (0x55)
        mapper.write_prg(0xC000, 0); // Mode 0
        let normal_at_5 = mapper.read_prg(0x8005);
        // In solder pad mode (mode 1), read at $8005 should return byte at offset 0 (0xAA)
        mapper.write_prg(0xC010, 0); // Mode 1
        let solder_at_5 = mapper.read_prg(0x8005);
        let solder_at_0 = mapper.read_prg(0x8000);
        // Solder pad mode forces low 4 bits to 0, so $8005 → $8000
        assert_eq!(
            solder_at_5, solder_at_0,
            "Solder pad: $8005 reads like $8000"
        );
        assert_ne!(
            normal_at_5, solder_at_5,
            "Solder pad should differ from normal read at offset > 0 (0x55 vs 0xAA)"
        );
    }

    // ───────── CHR-RAM variant (outer bank via lower latch) ─────────

    #[test]
    fn chr_ram_variant_lower_latch_sets_outer_bank() {
        let mut mapper = create_chr_ram_mapper(banked_data(16 * 1024, PRG_BANKS_RAM_VARIANT));
        // Mode 0 (UNROM), lower latch with outer bank bits 1-0 = 0b01 → outer = 0x08
        // Then inner prg = 0: combined = 0x08 | 0 = 0x08 → bank 8
        mapper.write_prg(0x8001, 0); // outer = (0x01 << 3) = 8
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 window should be at bank 8 with outer_bank=8"
        );
    }

    #[test]
    fn chr_ram_variant_upper_window_uses_outer_bank_last() {
        let mut mapper = create_chr_ram_mapper(banked_data(16 * 1024, PRG_BANKS_RAM_VARIANT));
        // Set outer bank = 0x08 (bits 1-0 = 01, shifted left 3)
        mapper.write_prg(0x8001, 0); // outer = 8
        // In UNROM mode, upper window = outer_bank | 7 = 8 | 7 = 15
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "$C000 window should be outer_bank|7 = 15"
        );
    }

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = create_chr_ram_mapper(banked_data(16 * 1024, PRG_BANKS_RAM_VARIANT));
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    // ───────── Write data value is ignored ─────────

    #[test]
    fn write_data_value_is_ignored() {
        let mut mapper = create_chr_rom_mapper(
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
        );
        mapper.write_prg(0xC003, 0x00);
        let bank_with_0 = mapper.read_prg(0x8000);
        mapper.write_prg(0xC003, 0xFF);
        let bank_with_ff = mapper.read_prg(0x8000);
        assert_eq!(bank_with_0, bank_with_ff, "Data value should be ignored");
        assert_eq!(bank_with_0, 3);
    }

    // ───────── Save state ─────────

    #[test]
    fn registers_snapshot_and_restore() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = create_chr_rom_mapper(prg.clone(), chr.clone());
        // Single lower-latch write: bit5=1 (horizontal) + bits2-0=3 (chr bank 3)
        mapper.write_prg(0x8023, 0);
        mapper.write_prg(0xC035, 0); // mode 3, prg bank 5

        let snap = mapper.registers_snapshot();

        let mut restored = create_chr_rom_mapper(prg, chr);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_prg(0xC000), 5);
        assert_eq!(restored.read_chr(0x0000), 3);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
