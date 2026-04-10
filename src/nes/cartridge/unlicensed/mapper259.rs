//! Mapper 259 – BMC-F15 (MMC3-variant multicart)
//!
//! Specifications:
//! - Primary source: NesDev wiki (no dedicated page found at time of implementation)
//! - Fallback source: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_BmcF15.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Mmc3Variants/MMC3_BmcF15.h>
//!
//! Hardware overview:
//! - MMC3-based multi-game cartridge (BMC-F15 board).
//! - PRG-ROM banking is fully controlled by a 4-bit outer register `ex_reg`.
//!   The standard MMC3 PRG bank registers (R6/R7) are completely bypassed.
//! - CHR banking and mirroring: standard MMC3 (CHR regs R0–R5, `$A000` mirroring).
//! - IRQ: standard MMC3 scanline counter (A12 rising-edge).
//!
//! # Outer PRG register
//!
//! Written by CPU writes to `$6000–$7FFF` when the MMC3 PRG-RAM enable bit
//! (`$A001` bit 7) is set.  Only bits 3:0 are stored.
//!
//! ```text
//! ex_reg[3:0]:
//!   bit 3  – mode: 0 = 16 KiB NROM-style (mirrored), 1 = 32 KiB NROM-style
//!   bits 2:0 – outer bank selector
//! ```
//!
//! # PRG banking
//!
//! Let `bank = ex_reg & 0x0F`, `mode = (ex_reg >> 3) & 1`.
//!
//! Mode 0 (`ex_reg` bit 3 = 0):
//! - `$8000–$BFFF` and `$C000–$FFFF` both map to the same 16 KiB block
//!   starting at 8 KiB page `bank * 2`.
//!
//! Mode 1 (`ex_reg` bit 3 = 1):
//! - `$8000–$BFFF` maps to 8 KiB pages `(bank & !1) * 2` and `(bank & !1) * 2 + 1`.
//! - `$C000–$FFFF` maps to 8 KiB pages `((bank & !1) | 1) * 2` and `((bank & !1) | 1) * 2 + 1`.
//! - Together these form a consecutive 32 KiB block.
//!
//! # Known Limitations
//! - No dedicated NesDev wiki page was found; specification derived from Mesen2 source.
//! - No known gameplay-blocking functional limitations documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::MapperContext;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, NametableLayout};

/// Mapper 259 – BMC-F15 (MMC3-variant multicart with outer PRG bank register)
pub struct Mapper259 {
    mmc3: MMC3Mapper,
    /// 4-bit outer PRG bank register written at `$6000–$7FFF` when PRG-RAM is enabled.
    ex_reg: u8,
}

impl Mapper259 {
    const MAPPER_NUMBER: u16 = 259;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;

    pub fn new(ctx: MapperContext) -> Self {
        let mmc3 = MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false);
        Self { mmc3, ex_reg: 0 }
    }

    /// Compute the effective 8 KiB PRG bank index for a CPU address in `$8000–$FFFF`.
    ///
    /// The 32 KiB window is divided into four 8 KiB slots (0–3). The outer
    /// register selects which physical 32 KiB block (mode 1) or 16 KiB block
    /// (mode 0, mirrored) to map.
    fn outer_prg_8k_bank(&self, addr: u16) -> usize {
        let bank = (self.ex_reg & 0x0F) as usize;
        let mode = ((self.ex_reg >> 3) & 1) as usize; // 0 or 1

        // In mode 0 (bit 3 clear) the full bank value is used for both halves.
        // In mode 1 (bit 3 set) bit 0 of bank is cleared for the lower half
        // and set for the upper half, yielding two consecutive 16 KiB pages.
        let banked_lower = bank & !mode; // bank with bit 0 cleared when mode=1
        let banked_upper = banked_lower | mode; // bit 0 set when mode=1

        // Which 8 KiB slot is this address in?  0=$8000, 1=$A000, 2=$C000, 3=$E000
        let slot = ((addr as usize).wrapping_sub(0x8000)) >> 13;

        let base_page = if slot < 2 {
            banked_lower << 1 // lower 16 KiB half
        } else {
            banked_upper << 1 // upper 16 KiB half
        };
        base_page + (slot & 1) // add intra-pair offset
    }
}

impl Mapper for Mapper259 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.outer_prg_8k_bank(addr);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }
    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.mmc3.is_prg_ram_enabled() {
                    self.ex_reg = value & 0x0F;
                }
            }
            0x8000..=0xFFFF => {
                self.mmc3.write_prg(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value)
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mmc3.base.mirroring()
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let mmc3_snap_len = self.mmc3.registers_snapshot().len();
        if data.len() >= mmc3_snap_len {
            self.mmc3.restore_registers(&data[..mmc3_snap_len]);
        }
        if data.len() > mmc3_snap_len {
            self.ex_reg = data[mmc3_snap_len] & 0x0F;
        }
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn reset(&mut self) {
        self.ex_reg = 0;
        self.mmc3.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Create a test mapper with 16 8KiB PRG banks (128 KiB) and 8 1KiB CHR banks.
    fn make_mapper() -> Mapper259 {
        Mapper259::new(MapperContext::new_for_test(
            259,
            banked_data(8 * 1024, 16),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_259_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            259,
            banked_data(8 * 1024, 16),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 259 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // PRG banking – mode 0 (ex_reg bit 3 = 0): 16 KiB mirrored
    // -------------------------------------------------------------------------

    /// At power-on ex_reg = 0.  Both halves of the 32 KiB window must mirror
    /// the same 16 KiB block (8 KiB pages 0 and 1).
    #[test]
    fn prg_mode0_power_on_maps_first_16k_bank_mirrored() {
        let mapper = make_mapper();
        // 8 KiB page 0 is filled with 0, page 1 filled with 1 (banked_data)
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 → bank 1");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 → bank 0 (mirror)");
        assert_eq!(mapper.read_prg(0xE000), 1, "$E000 → bank 1 (mirror)");
    }

    /// ex_reg = 0x03 (mode=0, outer bank=3): 16 KiB block at pages 6,7 mirrored.
    #[test]
    fn prg_mode0_outer_bank3_maps_pages_6_7_mirrored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // ex_reg = 3 (mode=0)
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 → page 6");
        assert_eq!(mapper.read_prg(0xA000), 7, "$A000 → page 7");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 → page 6 (mirror)");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 → page 7 (mirror)");
    }

    // -------------------------------------------------------------------------
    // PRG banking – mode 1 (ex_reg bit 3 = 1): 32 KiB consecutive
    // -------------------------------------------------------------------------

    /// ex_reg = 0x08 (mode=1, outer bank=8): pages 16,17,18,19 across 32 KiB.
    #[test]
    fn prg_mode1_ex_reg_0x08_maps_32k_block_at_pages_16_to_19() {
        let mut mapper = Mapper259::new(MapperContext::new_for_test(
            259,
            banked_data(8 * 1024, 32), // need 32 banks for pages 16-19 to exist
            banked_data(1024, 8),
            NametableLayout::Vertical,
        ));
        mapper.write_prg(0x6000, 0x08); // ex_reg = 8 (mode=1)
        assert_eq!(mapper.read_prg(0x8000), 16, "$8000 → page 16");
        assert_eq!(mapper.read_prg(0xA000), 17, "$A000 → page 17");
        assert_eq!(mapper.read_prg(0xC000), 18, "$C000 → page 18");
        assert_eq!(mapper.read_prg(0xE000), 19, "$E000 → page 19");
    }

    /// ex_reg = 0x09 (mode=1, bank=9): bit 0 of bank is masked out;
    /// same result as ex_reg=0x08.
    #[test]
    fn prg_mode1_bit0_of_exreg_is_ignored() {
        let mut mapper = Mapper259::new(MapperContext::new_for_test(
            259,
            banked_data(8 * 1024, 32),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        ));
        mapper.write_prg(0x6000, 0x09); // same block as 0x08
        assert_eq!(mapper.read_prg(0x8000), 16, "$8000 → page 16");
        assert_eq!(mapper.read_prg(0xA000), 17, "$A000 → page 17");
        assert_eq!(mapper.read_prg(0xC000), 18, "$C000 → page 18");
        assert_eq!(mapper.read_prg(0xE000), 19, "$E000 → page 19");
    }

    /// ex_reg = 0x0A (mode=1, bank=10): pages 20,21,22,23.
    #[test]
    fn prg_mode1_ex_reg_0x0a_maps_32k_block_at_pages_20_to_23() {
        let mut mapper = Mapper259::new(MapperContext::new_for_test(
            259,
            banked_data(8 * 1024, 32),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        ));
        mapper.write_prg(0x6000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 20);
        assert_eq!(mapper.read_prg(0xA000), 21);
        assert_eq!(mapper.read_prg(0xC000), 22);
        assert_eq!(mapper.read_prg(0xE000), 23);
    }

    // -------------------------------------------------------------------------
    // Outer register gating: only written when PRG-RAM is enabled (A001 bit 7)
    // -------------------------------------------------------------------------

    /// When A001 bit 7 is clear (prg_ram disabled), writes to $6000-$7FFF must
    /// not update ex_reg.
    #[test]
    fn outer_reg_not_updated_when_prg_ram_disabled() {
        let mut mapper = make_mapper();

        // Disable PRG-RAM via $A001 (bit 7 = 0)
        mapper.write_prg(0xA001, 0x00);

        // Now try to write outer reg – should be ignored
        mapper.write_prg(0x6000, 0x03);

        // ex_reg must still be 0 → pages 0,1,0,1
        assert_eq!(mapper.read_prg(0x8000), 0, "ex_reg must remain 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "ex_reg must remain 0 (mirror)");
    }

    /// When A001 bit 7 is set (prg_ram enabled), writes to $6000-$7FFF update ex_reg.
    #[test]
    fn outer_reg_is_updated_when_prg_ram_enabled() {
        let mut mapper = make_mapper();

        // PRG-RAM is enabled by default; also confirm via explicit write
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x03);

        assert_eq!(mapper.read_prg(0x8000), 6, "ex_reg = 3, page 6 expected");
    }

    /// Only bits 3:0 of the written value are stored.
    #[test]
    fn outer_reg_only_stores_lower_4_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF); // only 0x0F should be stored
        // 0x0F → mode=1, bank=15, banked_lower = 14, page = 14*2 = 28
        // With 16 banks, page 28 wraps to 28 % 16 = 12
        assert_eq!(mapper.read_prg(0x8000), 12 as u8);
    }

    // -------------------------------------------------------------------------
    // CHR banking: standard MMC3
    // -------------------------------------------------------------------------

    #[test]
    fn chr_banking_uses_standard_mmc3() {
        let mut mapper = make_mapper();

        // Select R2 (1 KiB CHR bank for slot 4 in MMC3 mode 0)
        mapper.write_prg(0x8000, 0x02); // bank_select = R2
        mapper.write_prg(0x8001, 5); // R2 = 5

        // CHR slot 4 at $1000 should read bank 5 (each byte = bank index)
        assert_eq!(mapper.read_chr(0x1000), 5, "CHR slot 4 → bank 5");
    }

    // -------------------------------------------------------------------------
    // Mirroring: standard MMC3 via $A000
    // -------------------------------------------------------------------------

    #[test]
    fn mirroring_controlled_by_mmc3_a000() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------------
    // Save state: registers_snapshot / restore_registers
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_roundtrip_preserves_ex_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x05); // ex_reg = 5

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // ex_reg = 5, mode=0 → pages 10, 11 mirrored
        assert_eq!(
            restored.read_prg(0x8000),
            10,
            "restored ex_reg=5 → page 10 at $8000"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            10,
            "restored ex_reg=5 → page 10 at $C000 (mirror)"
        );
    }

    // -------------------------------------------------------------------------
    // Reset
    // -------------------------------------------------------------------------

    #[test]
    fn reset_clears_ex_reg_to_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x07);
        mapper.reset();

        // After reset, ex_reg=0 → pages 0,1 mirrored
        assert_eq!(mapper.read_prg(0x8000), 0, "ex_reg reset to 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "ex_reg reset to 0 (mirror)");
    }

    // -------------------------------------------------------------------------
    // PRG wrapping: bank index wraps modulo physical bank count
    // -------------------------------------------------------------------------

    /// With 16 banks (0..15), ex_reg=0x08 (mode=1) → page 16 wraps to 0.
    #[test]
    fn prg_bank_wraps_modulo_physical_bank_count() {
        let mut mapper = make_mapper(); // 16 banks
        mapper.write_prg(0x6000, 0x08); // mode=1, page 16 % 16 = 0
        assert_eq!(mapper.read_prg(0x8000), 0, "page 16 wraps to bank 0");
    }
}
