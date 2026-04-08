//! Mapper 294 – 63-1601 multicart PCB
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/NES_2.0_Mapper_294>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 294 – 63-1601 multicart PCB (強捧 16-in-1 multicart, UNROM games)
///
/// Hardware summary:
/// - PRG-ROM: up to 2 MiB (128 × 16 KiB banks via 4-bit outer × 3-bit inner)
/// - CHR: 8 KiB CHR-RAM (fixed bank 0, no CHR banking)
/// - Mirroring: programmable V/H via outer register bit 4
/// - IRQ: none
/// - Audio: none
/// - Bus conflicts: yes (UNROM-style inner writes)
///
/// Outer register (write to any address where `addr & 0xC100 == 0x4100`):
/// - Bits 3:0 – outer PRG bank A20..A17 (selects 128 KiB UNROM block)
/// - Bit  4   – mirroring (0=Vertical, 1=Horizontal)
///
/// Inner UNROM banking (write to $8000–$FFFF, bus-conflict applied):
/// - Bits 2:0 of written value → inner 16 KiB page within outer block
///
/// Bank mapping:
/// - `$8000–$BFFF`: bank = `(outer & 0x0F) << 3 | (inner & 0x07)`
/// - `$C000–$FFFF`: bank = `(outer & 0x0F) << 3 | 0x07` (fixed last in outer block)
pub struct Mapper294 {
    base: BaseMapper,
    outer_bank: u8,
    inner_bank: u8,
    mirroring_horizontal: bool,
}

const OUTER_PRG_MASK: u8 = 0x0F;
const INNER_PRG_MASK: u8 = 0x07;
const MIRRORING_BIT: u8 = 0x10;

impl Mapper294 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.set_chr_memory(ChrMemory::new_ram(8 * 1024));
        base.set_bus_conflicts(true);

        let mut mapper = Self {
            base,
            outer_bank: 0,
            inner_bank: 0,
            mirroring_horizontal: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let outer = self.outer_bank as i16;
        let inner = self.inner_bank as i16;
        let bank_8000 = (outer << 3) | inner;
        let bank_c000 = (outer << 3) | (INNER_PRG_MASK as i16);
        self.base.select_prg_page(0, bank_8000);
        self.base.select_prg_page(1, bank_c000);
        self.base.select_chr_page(0, 0);
        self.base.set_mirroring_hv(self.mirroring_horizontal);
    }

    fn is_outer_reg(addr: u16) -> bool {
        addr & 0xC100 == 0x4100
    }
}

impl Mapper for Mapper294 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_outer_reg(addr) {
            self.outer_bank = value & OUTER_PRG_MASK;
            self.mirroring_horizontal = (value & MIRRORING_BIT) != 0;
            self.update_banks();
        } else if addr >= 0x8000 {
            let effective = self.base.apply_bus_conflict(addr, value);
            self.inner_bank = effective & INNER_PRG_MASK;
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.outer_bank,
            self.inner_bank,
            self.mirroring_horizontal as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        self.outer_bank = data[0] & OUTER_PRG_MASK;
        self.inner_bank = data[1] & INNER_PRG_MASK;
        self.mirroring_horizontal = data[2] != 0;
        self.update_banks();
    }

    fn reset(&mut self) {
        self.outer_bank = 0;
        self.inner_bank = 0;
        self.mirroring_horizontal = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    /// Non-power-of-2 count avoids modulo-wrap false-passes in bank assertions.
    const PRG_BANKS_16K: usize = 48; // 6 outer blocks × 8 inner banks
    const PRG_BANK_SIZE: usize = 16 * 1024;

    /// Build PRG-ROM where every byte is 0xFF (so bus-conflict AND passes write values
    /// through unchanged) and the bank index is stored at offset 0x100 within each bank.
    fn make_prg_rom() -> Vec<u8> {
        let mut rom = vec![0xFF_u8; PRG_BANK_SIZE * PRG_BANKS_16K];
        for bank in 0..PRG_BANKS_16K {
            rom[bank * PRG_BANK_SIZE + 0x100] = bank as u8;
        }
        rom
    }

    fn make_mapper() -> Mapper294 {
        Mapper294::new(MapperContext::new_for_test(
            294,
            make_prg_rom(),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    /// Read the bank-identification byte from a 16 KiB window.
    /// offset 0x100 within each 16 KiB bank.
    fn read_bank_id(mapper: &Mapper294, window_base: u16) -> u8 {
        mapper.read_prg(window_base + 0x100)
    }

    // ── Registration ────────────────────────────────────────────────────────

    #[test]
    fn mapper_294_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            294,
            make_prg_rom(),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 294 must be registered in factory");
    }

    // ── Power-on state ──────────────────────────────────────────────────────

    #[test]
    fn power_on_8000_maps_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            0,
            "$8000 must map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_c000_maps_to_last_inner_bank_in_outer_block_0() {
        let mapper = make_mapper();
        // outer=0, so fixed bank = 0*8 + 7 = 7
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "$C000 must map to bank 7 (last in outer block 0) at power-on"
        );
    }

    // ── UNROM inner banking ─────────────────────────────────────────────────

    #[test]
    fn inner_write_selects_prg_bank_at_8000() {
        let mut mapper = make_mapper();
        // outer=0, write inner=3 → bank 0*8+3 = 3
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            3,
            "$8000 should map to bank 3 after inner write 3"
        );
    }

    #[test]
    fn inner_write_does_not_change_fixed_c000_bank() {
        let mut mapper = make_mapper();
        // outer=0; whatever inner, $C000 stays at bank 7
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "$C000 must stay fixed at bank 7 regardless of inner write"
        );
    }

    #[test]
    fn inner_write_only_uses_lower_3_bits() {
        let mut mapper = make_mapper();
        // 0x08 has bit 3 set; only bits 2:0 = 0 should be used → bank 0
        mapper.write_prg(0x8000, 0x08);
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            0,
            "Inner write 0x08 should select bank 0 (only lower 3 bits used)"
        );
    }

    // ── Bus conflicts ───────────────────────────────────────────────────────

    /// Make a mapper where the ROM byte at $8000 (bank 0, offset 0) is 0x06,
    /// so a write of 0x07 produces effective inner = 0x07 & 0x06 = 0x06.
    fn make_mapper_for_bus_conflict() -> Mapper294 {
        let mut prg = make_prg_rom();
        prg[0] = 0b0000_0110; // bank 0, offset 0 → used by bus-conflict AND
        Mapper294::new(MapperContext::new_for_test(
            294,
            prg,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn inner_write_applies_bus_conflict() {
        let mut mapper = make_mapper_for_bus_conflict();
        // ROM byte at $8000 = 0b0000_0110; writing 0x07 → 0x07 & 0x06 = 0x06 (inner bank 6)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            6, // outer=0, inner=0x07 & 0x06 = 0x06 → bank 0*8+6 = 6
            "Bus conflict must AND write value (0x07) with ROM byte (0x06), selecting inner bank 6"
        );
    }

    // ── Outer bank register ─────────────────────────────────────────────────

    #[test]
    fn outer_register_at_4100_changes_prg_block() {
        let mut mapper = make_mapper();
        // Write outer=1 → outer block 1, inner=0 → bank 1*8+0 = 8
        mapper.write_prg(0x4100, 0x01);
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            8,
            "$8000 should map to bank 8 after outer=1, inner=0"
        );
        // fixed bank for outer=1: 1*8+7 = 15
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            15,
            "$C000 should map to bank 15 after outer=1"
        );
    }

    #[test]
    fn outer_register_mask_c100_triggers_on_4100_variants() {
        let mut mapper = make_mapper();
        // Any address where addr & 0xC100 == 0x4100 triggers the outer register.
        // Representative matches: 0x4100, 0x41FE (bits 15:14=01, bit 8=1, rest free).
        mapper.write_prg(0x41FE, 0x02); // addr & 0xC100 = 0x4100 → outer=2
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            16, // outer=2, inner=0 → 2*8+0=16
            "Write to 0x41FE (mask match) should update outer bank"
        );
    }

    #[test]
    fn outer_register_only_uses_lower_4_bits_for_prg() {
        let mut mapper = make_mapper();
        // outer = 0xFF; only bits 3:0 = 0x0F apply to PRG (4 outer blocks)
        // However our PRG_BANKS_16K=48 only has 6 outer blocks (idx 0-5)
        // Use outer=0x03 (3) to avoid out-of-range: 3*8+0=24
        mapper.write_prg(0x4100, 0x03);
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            24,
            "Only lower 4 bits of outer register used for PRG bank"
        );
    }

    // ── Outer + inner combined ──────────────────────────────────────────────

    #[test]
    fn outer_and_inner_combine_correctly() {
        let mut mapper = make_mapper();
        // outer=2 → block base = 2*8 = 16; inner=5 → bank = 16+5 = 21
        mapper.write_prg(0x4100, 0x02); // outer=2
        mapper.write_prg(0x8000, 0x05); // inner=5
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            21,
            "$8000 should be bank 21 (outer=2, inner=5)"
        );
        // fixed: 2*8+7 = 23
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            23,
            "$C000 should be bank 23 (outer=2, fixed=7)"
        );
    }

    #[test]
    fn inner_write_after_outer_uses_new_outer_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x03); // outer=3, base=24
        mapper.write_prg(0x8000, 0x06); // inner=6 → bank=30
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            30,
            "$8000 should be bank 30 (outer=3, inner=6)"
        );
    }

    // ── Mirroring ───────────────────────────────────────────────────────────

    #[test]
    fn power_on_mirroring_from_header() {
        let mapper = make_mapper(); // created with NametableLayout::Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring should match header"
        );
    }

    #[test]
    fn outer_register_bit4_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x10); // bit4=1 → horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Outer register bit 4 = 1 should select horizontal mirroring"
        );
    }

    #[test]
    fn outer_register_bit4_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x10); // set horizontal first
        mapper.write_prg(0x4100, 0x00); // bit4=0 → vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Outer register bit 4 = 0 should select vertical mirroring"
        );
    }

    // ── CHR-RAM ─────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable_and_readable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM at $0000 must retain written value"
        );
    }

    // ── Save-state ──────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore_preserves_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x12); // outer=2, bit4=1 (horizontal)
        mapper.write_prg(0x8000, 0x05); // inner=5
        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);
        assert_eq!(
            read_bank_id(&mapper2, 0x8000),
            read_bank_id(&mapper, 0x8000),
            "Bank at $8000 must match after restore"
        );
        assert_eq!(
            read_bank_id(&mapper2, 0xC000),
            read_bank_id(&mapper, 0xC000),
            "Bank at $C000 must match after restore"
        );
        assert_eq!(
            mapper2.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring must match after restore"
        );
    }

    // ── Reset ───────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x12); // outer=2, horizontal
        mapper.write_prg(0x8000, 0x05); // inner=5
        mapper.reset();
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            0,
            "$8000 must be bank 0 after reset"
        );
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "$C000 must be bank 7 after reset"
        );
    }
}
