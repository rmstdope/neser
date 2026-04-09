//! Mapper 158 - Tengen 800037 (RAMBO-1 with CHR-bit7 CIRAM selection)
//!
//! Specifications:
//! - Main: <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_158.xhtml>
//! - Base behavior: <https://nesdev-wiki.nes.science/wikipages/RAMBO_1.xhtml>
//!
//! Mapper 158 is RAMBO-1 (mapper 64) with one change: CHR A17 is wired to CIRAM A10 instead
//! of using the $A000 mirroring register. This allows per-quadrant nametable selection via
//! bit 7 of CHR bank data writes ($8001).
//!
//! The $A000 mirroring register is ignored entirely.
//!
//! The only known game using this mapper is Alien Syndrome (Tengen).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::cartridge::tengen::tengen_rambo1::TengenRambo1Mapper;

const CIRAM_SIZE: usize = 0x0800;
const NT_PAGE_SIZE: usize = 0x0400;

/// Mapper 158 - Tengen 800037 (RAMBO-1 with CHR-bit7 CIRAM A10 routing)
pub struct Mapper158 {
    rambo1: TengenRambo1Mapper,
    /// CIRAM page (0 or 1) for each of the four nametable quadrants.
    nt_pages: [u8; 4],
    ciram: [u8; CIRAM_SIZE],
}

impl Mapper158 {
    pub fn new(ctx: MapperContext) -> Self {
        Self {
            rambo1: TengenRambo1Mapper::new(MapperContext { mapper: 64, ..ctx }),
            nt_pages: [0; 4],
            ciram: [0; CIRAM_SIZE],
        }
    }

    /// Update nametable page assignments after a $8001 bank data write.
    ///
    /// Bit 7 of the bank value selects the CIRAM page (0 or 1).
    /// Which nametable quadrant(s) are affected depends on the currently selected register
    /// and the C bit (CHR A12 inversion) from the last $8000 write.
    fn update_nt_pages(&mut self, value: u8) {
        let page = value >> 7;
        let (reg, c_bit) = self.rambo1.bank_select_state();
        if c_bit {
            // C=1: registers 2-5 each control one nametable quadrant
            match reg & 0x0F {
                2 => self.nt_pages[0] = page,
                3 => self.nt_pages[1] = page,
                4 => self.nt_pages[2] = page,
                5 => self.nt_pages[3] = page,
                _ => {}
            }
        } else {
            // C=0: register 0 controls quadrants 0+1, register 1 controls quadrants 2+3
            match reg & 0x0F {
                0 => {
                    self.nt_pages[0] = page;
                    self.nt_pages[1] = page;
                }
                1 => {
                    self.nt_pages[2] = page;
                    self.nt_pages[3] = page;
                }
                _ => {}
            }
        }
    }

    fn ciram_offset(&self, addr: u16) -> usize {
        // addr is in $2000-$2FFF; mask to $0FFF, then split into quadrant + offset
        let rel = (addr as usize) & 0x0FFF;
        let quadrant = rel / NT_PAGE_SIZE;
        let page = self.nt_pages[quadrant & 3] as usize;
        page * NT_PAGE_SIZE + (rel & (NT_PAGE_SIZE - 1))
    }
}

impl Mapper for Mapper158 {
    fn base(&self) -> &BaseMapper {
        self.rambo1.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.rambo1.base_mut()
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.rambo1.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let even = (addr & 1) == 0;
        // $A000 even: mirroring register — ignored on mapper 158
        if (addr & 0xE000) == 0xA000 && even {
            return;
        }
        // $8001: bank data write — update nametable assignments, then update banks
        if (addr & 0xE000) == 0x8000 && !even {
            self.update_nt_pages(value);
        }
        self.rambo1.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.rambo1.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.rambo1.write_chr(addr, value);
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }
        Some(self.ciram[self.ciram_offset(addr)])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }
        let offset = self.ciram_offset(addr);
        self.ciram[offset] = value;
        true
    }

    fn irq_pending(&self) -> bool {
        self.rambo1.irq_pending()
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.rambo1.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.rambo1.cpu_cycle();
    }

    fn reset(&mut self) {
        self.rambo1.reset();
        self.nt_pages = [0; 4];
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.rambo1.initialize_ram(mode);
        crate::nes::console::initialize_ram(&mut self.ciram, mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut v = self.rambo1.registers_snapshot();
        v.extend_from_slice(&self.nt_pages);
        v.extend_from_slice(&self.ciram);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let extra = 4 + CIRAM_SIZE;
        if data.len() < extra {
            return;
        }
        let (rambo1_data, rest) = data.split_at(data.len() - extra);
        self.rambo1.restore_registers(rambo1_data);
        self.nt_pages.copy_from_slice(&rest[..4]);
        self.ciram.copy_from_slice(&rest[4..]);
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.rambo1.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;
    use crate::nes::console::RamInitMode;

    const PRG_BANKS: usize = 32;
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            158,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 158 must be registered")
    }

    fn select_bank(m: &mut Box<dyn Mapper>, reg: u8, bank: u8) {
        m.write_prg(0x8000, reg);
        m.write_prg(0x8001, bank);
    }

    // ── Registration ────────────────────────────────────────────────────────

    #[test]
    fn mapper_158_is_registered() {
        let _ = make_mapper();
    }

    // ── PRG banking (same as mapper 64) ─────────────────────────────────────

    #[test]
    fn prg_r6_selects_8000_bank() {
        let mut m = make_mapper();
        select_bank(&mut m, 6, 5);
        assert_eq!(m.read_prg(0x8000), 5);
    }

    #[test]
    fn prg_r7_selects_a000_bank() {
        let mut m = make_mapper();
        select_bank(&mut m, 7, 9);
        assert_eq!(m.read_prg(0xA000), 9);
    }

    #[test]
    fn prg_last_bank_is_fixed() {
        let m = make_mapper();
        assert_eq!(m.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    // ── CHR banking (same as mapper 64) ─────────────────────────────────────

    #[test]
    fn chr_r0_2k_fills_slots_0_and_1() {
        let mut m = make_mapper();
        select_bank(&mut m, 0, 10);
        assert_eq!(m.read_chr(0x0000), 10);
        assert_eq!(m.read_chr(0x0400), 11);
    }

    // ── Nametable: $A000 mirroring register is ignored ──────────────────────

    #[test]
    fn a000_writes_are_ignored() {
        let mut m = make_mapper();
        // Write a value to nametable 0 with NT page 0 selected (default)
        assert!(m.write_nametable(0x2000, 0x42));
        // Changing mirroring via $A000 should have no effect
        m.write_prg(0xA000, 0x01);
        // The nametable data should still be in page 0
        assert_eq!(m.read_nametable(0x2000), Some(0x42));
    }

    // ── Nametable: NT page via CHR bank bit 7 (C=0 mode) ────────────────────

    #[test]
    fn r0_bit7_sets_nt_pages_0_and_1() {
        let mut m = make_mapper();
        // Default R0 bit7=0: quadrants 0 and 1 → CIRAM page 0
        assert!(m.write_nametable(0x2000, 0x11));
        // Set R0 bit7=1: quadrants 0 and 1 → CIRAM page 1
        select_bank(&mut m, 0, 0x80);
        assert!(m.write_nametable(0x2000, 0xAA));
        // Both quadrants now read page 1 (they mirror each other)
        assert_eq!(m.read_nametable(0x2000), Some(0xAA));
        assert_eq!(m.read_nametable(0x2400), Some(0xAA)); // mirrors $2000 (same page+offset)
        // Switch back to page 0: old data intact
        select_bank(&mut m, 0, 0x00);
        assert_eq!(m.read_nametable(0x2000), Some(0x11));
    }

    #[test]
    fn r1_bit7_sets_nt_pages_2_and_3() {
        let mut m = make_mapper();
        assert!(m.write_nametable(0x2800, 0x33));
        select_bank(&mut m, 1, 0x80);
        assert!(m.write_nametable(0x2800, 0xCC));
        assert_eq!(m.read_nametable(0x2800), Some(0xCC));
        assert_eq!(m.read_nametable(0x2C00), Some(0xCC)); // mirrors $2800
        select_bank(&mut m, 1, 0x00);
        assert_eq!(m.read_nametable(0x2800), Some(0x33));
    }

    // ── Nametable: NT page via CHR bank bit 7 (C=1 mode) ────────────────────

    #[test]
    fn c1_mode_r2_and_r3_independently_control_quadrants_0_and_1() {
        let mut m = make_mapper();
        // Enable C=1, R2 → quadrant 0 = CIRAM page 1
        m.write_prg(0x8000, 0x80 | 2);
        m.write_prg(0x8001, 0x80);
        // Enable C=1, R3 → quadrant 1 = CIRAM page 0
        m.write_prg(0x8000, 0x80 | 3);
        m.write_prg(0x8001, 0x00);

        // Quadrant 0 (page 1) and quadrant 1 (page 0) are independent
        assert!(m.write_nametable(0x2000, 0x56)); // → page 1
        assert!(m.write_nametable(0x2400, 0x78)); // → page 0
        assert_eq!(m.read_nametable(0x2000), Some(0x56));
        assert_eq!(m.read_nametable(0x2400), Some(0x78));

        // Switch R2 to page 0: quadrant 0 now mirrors quadrant 1
        m.write_prg(0x8000, 0x80 | 2);
        m.write_prg(0x8001, 0x00);
        assert_eq!(m.read_nametable(0x2000), Some(0x78));
    }

    #[test]
    fn c1_mode_all_four_quadrants_use_interleaved_mirroring() {
        let mut m = make_mapper();
        // R2=page1, R3=page0, R4=page1, R5=page0 → interleaved mirroring
        m.write_prg(0x8000, 0x80 | 2);
        m.write_prg(0x8001, 0x80); // NT0 → page 1
        m.write_prg(0x8000, 0x80 | 3);
        m.write_prg(0x8001, 0x00); // NT1 → page 0
        m.write_prg(0x8000, 0x80 | 4);
        m.write_prg(0x8001, 0x80); // NT2 → page 1 (mirrors NT0)
        m.write_prg(0x8000, 0x80 | 5);
        m.write_prg(0x8001, 0x00); // NT3 → page 0 (mirrors NT1)

        assert!(m.write_nametable(0x2000, 0x56)); // page 1
        assert!(m.write_nametable(0x2400, 0x78)); // page 0
        // NT2/NT3 mirror NT0/NT1 (same CIRAM page and offset)
        assert_eq!(m.read_nametable(0x2000), Some(0x56));
        assert_eq!(m.read_nametable(0x2800), Some(0x56)); // mirrors $2000
        assert_eq!(m.read_nametable(0x2400), Some(0x78));
        assert_eq!(m.read_nametable(0x2C00), Some(0x78)); // mirrors $2400
    }

    // ── IRQ (delegates to RAMBO-1) ──────────────────────────────────────────

    #[test]
    fn irq_cpu_mode_fires() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 4);
        m.write_prg(0xC001, 1); // cpu mode
        m.write_prg(0xE001, 0); // enable
        let mut fired = false;
        for _ in 0..50 {
            m.cpu_cycle();
            if m.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(fired, "IRQ must fire in CPU cycle mode");
    }

    // ── Save state ───────────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_nametable_state() {
        let mut m = make_mapper();
        // Write to CIRAM page 0
        assert!(m.write_nametable(0x2000, 0x11));
        // Set R0 → CIRAM page 1
        select_bank(&mut m, 0, 0x80);
        assert!(m.write_nametable(0x2000, 0xAA));

        let snapshot = m.registers_snapshot();
        let mut m2 = make_mapper();
        m2.restore_registers(&snapshot);

        // After restore, page 1 should be active (R0=0x80 was set)
        assert_eq!(m2.read_nametable(0x2000), Some(0xAA));
        // Switch to page 0 and verify old data
        select_bank(&mut m2, 0, 0x00);
        assert_eq!(m2.read_nametable(0x2000), Some(0x11));
    }

    #[test]
    fn initialize_ram_initializes_mapper_owned_ciram() {
        let mut m = make_mapper();
        m.initialize_ram(RamInitMode::SeededRandom(0x158));
        let snapshot = m.registers_snapshot();
        let ciram = &snapshot[snapshot.len() - CIRAM_SIZE..];
        assert!(
            ciram.iter().any(|&b| b != 0),
            "initialize_ram should initialize mapper-owned CIRAM"
        );
    }
}
