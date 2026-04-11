//! Mapper 261 – BMC-810544-C-A1 (multicart address latch)
//!
//! Specifications:
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_261>
//! - Reference implementation: Mesen2 `Core/NES/Mappers/Unlicensed/Bmc810544CA1.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Bmc810544CA1.h>
//!
//! # Hardware overview
//!
//! Used on the BMC-810544-C-A1 and NTDEC-2746 multicart boards (e.g., "200-in-1 Elfland",
//! "14-in-1 NTDEC").
//!
//! All banking information is encoded in the **write address**, not the data byte.
//! Any write to `$8000–$FFFF` latches the address bus and re-applies banks.
//!
//! # Address latch (`$8000–$FFFF`, write — address bits only)
//!
//! ```text
//! A~1... ..PP PmpM CCCC
//!          || |||| ++++- Select 8 KiB CHR-ROM bank at PPU $0000–$1FFF
//!          || |||+------ Nametable mirroring: 0=Vertical, 1=Horizontal
//!          || ||+------- Bit 0 of 16 KiB PRG-ROM bank in NROM-128 mode
//!          || |+-------- PRG-ROM banking mode
//!          || |           0: NROM-128 (16 KiB at $8000–$BFFF, mirrored to $C000–$FFFF)
//!          || |           1: NROM-256 (32 KiB at $8000–$FFFF)
//!          ++-+--------- Bits 1–3 of 16 KiB PRG bank (NROM-128) /
//!                        32 KiB PRG bank selector (NROM-256)
//! ```
//!
//! # PRG banking
//!
//! Let `outer = (addr >> 7) & 0x07` (bits 9:7 of write address, 3 bits).
//!
//! **NROM-256 mode** (`addr & 0x40` is set):
//! - `$8000–$BFFF` → 16 KiB page `outer × 2`
//! - `$C000–$FFFF` → 16 KiB page `outer × 2 + 1`
//!
//! **NROM-128 mode** (`addr & 0x40` is clear):
//! - Both 16 KiB windows map to the same page:
//!   `(outer × 2) | ((addr >> 5) & 1)`
//!
//! # CHR banking
//!
//! 8 KiB CHR-ROM bank selected by `addr & 0x0F` (bits 3:0 of write address).
//!
//! # Mirroring
//!
//! Bit 4 of the write address: `0` = Vertical, `1` = Horizontal.
//!
//! # Power-on / reset state
//!
//! Equivalent to writing address `$8000`: all banks map to page 0,
//! CHR bank 0, vertical mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 261;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 261 – BMC-810544-C-A1
///
/// All banking is controlled by the **address** of any write to `$8000–$FFFF`.
/// The written data byte is ignored.
pub struct Mapper261 {
    base: BaseMapper,
    /// The last write address latched (determines PRG/CHR/mirroring).
    latch: u16,
}

impl Mapper261 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        // Power-on: equivalent to writing address $8000 (all banks = 0).
        let mut mapper = Self {
            base,
            latch: 0x8000,
        };
        mapper.apply_latch();
        mapper
    }

    fn apply_latch(&mut self) {
        let addr = self.latch;
        // (addr >> 6) gives a 16KB page index (with bit 0 sometimes meaningful).
        // Masking to 0xFFFE forces the base to an even page number.
        let bank = (addr >> 6) & 0xFFFE;
        if addr & 0x40 != 0 {
            // NROM-256: consecutive 16 KiB pair
            self.base.select_prg_page(0, bank as i16);
            self.base.select_prg_page(1, (bank + 1) as i16);
        } else {
            // NROM-128: both windows map to the same 16 KiB page
            let inner = (bank | ((addr >> 5) & 0x01)) as i16;
            self.base.select_prg_page(0, inner);
            self.base.select_prg_page(1, inner);
        }
        self.base.select_chr_page(0, (addr & 0x0F) as i16);
        self.base.set_mirroring_hv(addr & 0x10 != 0);
    }
}

impl Mapper for Mapper261 {
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
            self.latch = addr;
            self.apply_latch();
        }
    }

    fn reset(&mut self) {
        self.latch = 0x8000;
        self.apply_latch();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.latch.to_le_bytes().to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        self.latch = u16::from_le_bytes([data[0], data[1]]);
        self.apply_latch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 8; // 8 × 16 KiB = 128 KiB
    const CHR_BANKS: usize = 16; // 16 × 8 KiB = 128 KiB

    fn make_mapper() -> Mapper261 {
        Mapper261::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_261_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 261 must be registered in the factory"
        );
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_both_prg_windows_map_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 → bank 0 (NROM-128 mirror)"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR $0000 → bank 0");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── NROM-128 mode (addr bit 6 clear) ─────────────────────────────────────

    #[test]
    fn nrom128_both_windows_mirror_same_bank() {
        // Write to $8020: bank = (0x8020 >> 6) & 0xFFFE = 0x200; inner = 0x200 | 1 = 0x201
        // 0x201 % 8 = 1 → both windows map to bank 1
        let mut mapper = make_mapper();
        mapper.write_prg(0x8020, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 → bank 1");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 → bank 1 (mirrored)");
    }

    #[test]
    fn nrom128_outer_bank_bits_9_to_7_select_bank_group() {
        // Write to $8080: bits 9:7 = 001, so outer = 1; bank = (0x8080 >> 6) & 0xFFFE = 0x202
        // inner = 0x202 | 0 = 0x202; 0x202 % 8 = 2 → bank 2
        let mut mapper = make_mapper();
        mapper.write_prg(0x8080, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2, "addr[7]=1 → bank 2");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 → bank 2 (mirrored)");
    }

    // ── NROM-256 mode (addr bit 6 set) ───────────────────────────────────────

    #[test]
    fn nrom256_maps_consecutive_16k_pair() {
        // Write to $8040: addr & 0x40 set → NROM-256
        // bank = (0x8040 >> 6) & 0xFFFE = 0x201 & 0xFFFE = 0x200; wraps: 0 and 1
        let mut mapper = make_mapper();
        mapper.write_prg(0x8040, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 → bank 1");
    }

    #[test]
    fn nrom256_outer_bank_selects_32k_block() {
        // Write to $80C0: addr & 0x40 set, addr[7]=1 → outer × 2 = 2, 3
        let mut mapper = make_mapper();
        mapper.write_prg(0x80C0, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 → bank 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 → bank 3");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_by_addr_bits_3_to_0() {
        let mut mapper = make_mapper();
        // addr = $800F: CHR bits 3:0 = 0x0F = 15
        mapper.write_prg(0x800F, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 15 % CHR_BANKS as u8, "CHR bank 15");
    }

    #[test]
    fn chr_bank_5() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR bank 5");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn addr_bit_4_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn addr_bit_4_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 0x00); // set horizontal
        mapper.write_prg(0x8000, 0x00); // clear to vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8080, 0x00);
        mapper.write_prg(0x8010, 0x00);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 after reset");
        assert_eq!(mapper.read_prg(0xC000), 0, "PRG mirror after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "V mirroring after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_latch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8091, 0x00); // outer=1, m=0, H-mirror, CHR=1
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "CHR match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "mirroring match"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8080, 0x00);
        let prev = mapper.read_prg(0x8000);
        mapper.restore_registers(&[0x00]);
        assert_eq!(mapper.read_prg(0x8000), prev, "state unchanged");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "CHR banking");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
