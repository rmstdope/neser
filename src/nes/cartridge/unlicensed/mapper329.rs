//! Mapper 329 – EDU2000 board
//!
//! Specifications:
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//! - Fallback: Mesen2 `Edu2000.h` (primary reference used).
//!
//! # Hardware overview
//!
//! Used for EDU2000 educational cartridges.
//!
//! - PRG-ROM: 32 KiB pages; bank selected by bits 4:0 of the control register
//! - CHR-ROM: 8 KiB fixed (bank 0 always)
//! - PRG-RAM (Work RAM): 32 KiB total; 8 KiB window at $6000–$7FFF;
//!   page selected by bits 7:6 of the control register (4 pages of 8 KiB)
//! - Control register: any write to $8000–$FFFF latches the written byte
//! - Mirroring: fixed from header (not programmable)
//! - IRQ: none
//! - Bus conflicts: none
//!
//! # Register format (written to $8000–$FFFF)
//!
//! | Bits | Effect                                         |
//! |------|------------------------------------------------|
//! | 4:0  | PRG-ROM 32 KiB bank select                     |
//! | 7:6  | PRG-RAM 8 KiB page select (from 32 KiB pool)  |
//!
//! # Power-on state
//!
//! Register = 0: PRG-ROM bank 0 at $8000–$FFFF; PRG-RAM page 0 at $6000–$7FFF.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Total PRG-RAM size: 32 KiB (4 pages × 8 KiB).
const WRAM_SIZE: usize = 32 * 1024;
/// PRG-RAM page size: 8 KiB.
const WRAM_PAGE_SIZE: usize = 8 * 1024;

/// Mapper 329 – EDU2000 board
///
/// Control register layout:
/// - Bits 4:0 → PRG-ROM 32 KiB bank
/// - Bits 7:6 → PRG-RAM 8 KiB page (from 32 KiB pool)
pub struct Mapper329 {
    base: BaseMapper,
    reg: u8,
    wram: [u8; WRAM_SIZE],
}

impl Mapper329 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 32,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            reg: 0,
            wram: [0; WRAM_SIZE],
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let prg_bank = (self.reg & 0x1F) as i16;
        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, 0);
    }

    fn wram_page(&self) -> usize {
        ((self.reg >> 6) & 0x03) as usize
    }

    fn wram_offset(&self, addr: u16) -> usize {
        let page_base = self.wram_page() * WRAM_PAGE_SIZE;
        page_base + (addr - 0x6000) as usize
    }
}

impl Mapper for Mapper329 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.wram[self.wram_offset(addr)],
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let offset = self.wram_offset(addr);
                self.wram[offset] = value;
            }
            0x8000..=0xFFFF => {
                self.reg = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn wram_size(&self) -> usize {
        WRAM_SIZE
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.wram.to_vec()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(WRAM_SIZE);
        self.wram[..len].copy_from_slice(&data[..len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&r) = data.first() {
            self.reg = r;
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 11; // 11 × 32 KB = 352 KB
    const CHR_BANKS: usize = 7; // 7 × 8 KB = 56 KB

    fn make_mapper() -> Mapper329 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper329::new(MapperContext::new_for_test(
            329,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_329_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            329,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 329 must be registered in the factory"
        );
    }

    // --- PRG-ROM banking ---

    #[test]
    fn initial_prg_bank_is_zero() {
        let mapper = make_mapper();
        // Bank 0 is filled with 0x00 by banked_data
        assert_eq!(
            mapper.read_prg(0x8000),
            0x00,
            "initial PRG bank should be 0"
        );
    }

    #[test]
    fn prg_banking_selects_32kb_bank_via_low_5_bits() {
        let mut mapper = make_mapper();
        // Bank 3 is filled with 0x03
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            0x03,
            "PRG bank 3 should be mapped at $8000"
        );
    }

    #[test]
    fn prg_bank_register_ignores_upper_bits() {
        let mut mapper = make_mapper();
        // bits 7:6 are WRAM page bits, bit 5 is unused; PRG bank = value & 0x1F
        // Write 0xC3: bits 7:6 = 0b11 (WRAM page 3), bits 4:0 = 0x03 (PRG bank 3)
        mapper.write_prg(0x8000, 0xC3);
        // PRG should be bank 3 (0x03)
        assert_eq!(
            mapper.read_prg(0x8000),
            0x03,
            "PRG bank should use only bits 4:0"
        );
    }

    #[test]
    fn prg_register_write_at_any_address_in_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x05);
        assert_eq!(
            mapper.read_prg(0x8000),
            0x05,
            "write to $FFFF should set PRG bank 5"
        );
    }

    // --- CHR banking ---

    #[test]
    fn chr_is_fixed_at_bank_0_initially() {
        let mapper = make_mapper();
        // CHR bank 0 is filled with 0x00
        assert_eq!(
            mapper.base.read_chr(0x0000),
            0x00,
            "CHR should be at bank 0"
        );
    }

    #[test]
    fn chr_remains_at_bank_0_after_register_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06);
        // CHR is always fixed at bank 0
        assert_eq!(
            mapper.base.read_chr(0x0000),
            0x00,
            "CHR must remain at bank 0"
        );
    }

    // --- PRG-RAM (Work RAM) banking ---

    #[test]
    fn initial_wram_page_is_zero() {
        let mut mapper = make_mapper();
        // Write a marker to page 0 addr $6000
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "initial WRAM page 0 should be accessible at $6000"
        );
    }

    #[test]
    fn wram_page_selected_by_bits_7_6() {
        let mut mapper = make_mapper();
        // Write distinct values to pages 0..3 via register switching
        // Page 0 (bits 7:6 = 00)
        mapper.write_prg(0x8000, 0x00); // page 0
        mapper.write_prg(0x6000, 0xAA);
        // Page 1 (bits 7:6 = 01)
        mapper.write_prg(0x8000, 0x40); // bits 7:6=01, PRG bank 0
        mapper.write_prg(0x6000, 0xBB);
        // Page 2 (bits 7:6 = 10)
        mapper.write_prg(0x8000, 0x80); // bits 7:6=10, PRG bank 0
        mapper.write_prg(0x6000, 0xCC);
        // Page 3 (bits 7:6 = 11)
        mapper.write_prg(0x8000, 0xC0); // bits 7:6=11, PRG bank 0
        mapper.write_prg(0x6000, 0xDD);

        // Verify each page holds its own value
        mapper.write_prg(0x8000, 0x00); // back to page 0
        assert_eq!(mapper.read_prg(0x6000), 0xAA, "page 0 should hold 0xAA");
        mapper.write_prg(0x8000, 0x40);
        assert_eq!(mapper.read_prg(0x6000), 0xBB, "page 1 should hold 0xBB");
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(mapper.read_prg(0x6000), 0xCC, "page 2 should hold 0xCC");
        mapper.write_prg(0x8000, 0xC0);
        assert_eq!(mapper.read_prg(0x6000), 0xDD, "page 3 should hold 0xDD");
    }

    #[test]
    fn wram_pages_are_independent_8kb_windows() {
        let mut mapper = make_mapper();
        // Write the full 8 KB window in page 0
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x7FFF, 0x22);

        // Switch to page 1 – values in page 0 should not be visible
        mapper.write_prg(0x8000, 0x40);
        assert_ne!(
            mapper.read_prg(0x6000),
            0x11,
            "page 1 $6000 should differ from page 0"
        );
        assert_ne!(
            mapper.read_prg(0x7FFF),
            0x22,
            "page 1 $7FFF should differ from page 0"
        );

        // Switch back to page 0 and verify values are preserved
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x11,
            "page 0 $6000 should be preserved"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0x22,
            "page 0 $7FFF should be preserved"
        );
    }

    // --- WRAM persistence / save-state ---

    #[test]
    fn wram_size_is_32kb() {
        let mapper = make_mapper();
        assert_eq!(mapper.wram_size(), 32 * 1024, "WRAM must be 32 KiB");
    }

    #[test]
    fn wram_snapshot_and_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x8000, 0x40);
        mapper.write_prg(0x6000, 0x99);

        let snapshot = mapper.wram_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.load_wram_snapshot(&snapshot);

        mapper2.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper2.read_prg(0x6000),
            0x42,
            "page 0 value should survive snapshot"
        );
        mapper2.write_prg(0x8000, 0x40);
        assert_eq!(
            mapper2.read_prg(0x6000),
            0x99,
            "page 1 value should survive snapshot"
        );
    }

    #[test]
    fn registers_snapshot_and_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xC5); // PRG bank 5, WRAM page 3
        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);
        // After restore, PRG bank should be 5
        assert_eq!(
            mapper2.read_prg(0x8000),
            0x05,
            "PRG bank should be restored to 5"
        );
    }
}
