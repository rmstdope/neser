//! Mapper 333 – BMC-8-in-1 (MMC3 variant with outer bank register)
//!
//! # Specifications
//! - Primary source: NesDev Wiki (no dedicated page found for mapper 333)
//! - Fallback source: Mesen2 `Core/NES/Mappers/Mmc3Variants/Bmc8in1.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Mmc3Variants/Bmc8in1.h>
//!
//! ## Hardware overview
//! BMC-8-in-1 is an MMC3 clone found on multicart boards (also known as
//! NEWSTAR-GRM070-8IN1). It extends standard MMC3 with an outer bank register
//! that selects a 128 KiB PRG/CHR window into a larger ROM.
//!
//! ## Outer register (address bit 12 set, $8000–$FFFF)
//! Any write to $8000–$FFFF where **address bit 12 is set** (i.e., $9xxx, $Bxxx,
//! $Dxxx, $Fxxx) stores the written byte in the outer register.
//!
//! Writes where bit 12 is **clear** ($8xxx, $Axxx, $Cxxx, $Exxx) are forwarded
//! to the standard MMC3 register handler.
//!
//! ## Outer register layout
//! ```text
//! D~7654 3210
//!   ...M OOOO
//!      | ++++- OOOO: outer 32 KiB PRG block (NROM mode) / outer CHR/PRG high bits (MMC3 mode)
//!      +------ M: 0=NROM mode, 1=MMC3 mode
//! ```
//! Power-up: `$00`.
//!
//! Bits 3:2 (OO) are shared between PRG and CHR outer banking in MMC3 mode.
//!
//! ## PRG banking
//!
//! **NROM mode** (`M=0`, outer bit 4 clear):
//! All four 8 KiB CPU slots ($8000–$FFFF) are fixed to four consecutive banks:
//! ```text
//! base = (reg & 0x0F) << 2
//! $8000–$9FFF → bank base+0
//! $A000–$BFFF → bank base+1
//! $C000–$DFFF → bank base+2
//! $E000–$FFFF → bank base+3
//! ```
//!
//! **MMC3 mode** (`M=1`, outer bit 4 set):
//! Standard MMC3 PRG banking applies, with the outer register providing high bits:
//! ```text
//! actual_bank = ((reg & 0x0C) << 2) | (mmc3_8k_page & 0x0F)
//! ```
//!
//! ## CHR banking
//! Always active (both modes), the outer register provides high bits:
//! ```text
//! actual_1k_bank = ((reg & 0x0C) << 5) | (mmc3_1k_page & 0x7F)
//! ```
//!
//! ## IRQ
//! Standard MMC3 scanline IRQ (A12 rising-edge).
//!
//! ## Mirroring
//! Software-controlled via MMC3's $A000 register.
//!
//! ## PRG-RAM
//! Optional 8 KiB PRG-RAM at $6000–$7FFF, controlled by MMC3's $A001 register.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 333 – BMC-8-in-1, an MMC3 variant with an outer bank register.
pub struct Mapper333 {
    pub(crate) mmc3: MMC3Mapper,
    /// Outer register. Written when any $8000–$FFFF address has bit 12 set.
    pub(crate) reg: u8,
}

impl Mapper333 {
    const MAPPER_NUMBER: u16 = 333;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            reg: 0,
        }
    }

    /// Returns `true` when the outer register selects MMC3 mode (bit 4 set).
    fn mmc3_mode(&self) -> bool {
        (self.reg & 0x10) != 0
    }

    /// Resolves the physical PRG bank for `addr` applying the outer register.
    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        if self.mmc3_mode() {
            let mmc3_page = self.mmc3.raw_prg_8k_page_number(addr) as usize;
            ((self.reg as usize & 0x0C) << 2) | (mmc3_page & 0x0F)
        } else {
            let slot = (addr as usize - 0x8000) >> 13;
            ((self.reg as usize & 0x0F) << 2) | slot
        }
    }

    /// Resolves the physical 1 KiB CHR bank for `addr` applying the outer register.
    fn chr_bank_for_addr(&mut self, addr: u16) -> usize {
        let inner = self.mmc3.raw_chr_1k_bank(addr);
        ((self.reg as usize & 0x0C) << 5) | (inner & 0x7F)
    }
}

impl Mapper for Mapper333 {
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

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for_addr(addr);
                let prg_count = self.mmc3.base.prg_rom().len() / Self::PRG_BANK_SIZE;
                let wrapped = if prg_count > 0 { bank % prg_count } else { 0 };
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(wrapped, offset)
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
            0x8000..=0xFFFF => {
                if (addr & 0x1000) != 0 {
                    self.reg = value;
                } else {
                    self.mmc3.write_prg(addr, value);
                }
            }
            _ => self.mmc3.write_prg(addr, value),
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.chr_bank_for_addr(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(wrapped, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.chr_bank_for_addr(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(wrapped, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
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
        snap.push(self.reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.mmc3.restore_registers(&data[..16]);
            self.reg = data[16];
        } else {
            self.mmc3.restore_registers(data);
            self.reg = 0;
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to expose modulo-wrap issues.
    const PRG_8K_BANKS: usize = 9;
    const CHR_1K_BANKS: usize = 48;

    fn make_mapper() -> Mapper333 {
        Mapper333::new(MapperContext::new_for_test(
            333,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ------------------------------------------------------------------
    // Factory registration
    // ------------------------------------------------------------------

    #[test]
    fn mapper_333_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            333,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 333 must be registered in factory");
    }

    // ------------------------------------------------------------------
    // PRG banking – NROM mode (default, reg=0)
    // ------------------------------------------------------------------

    #[test]
    fn nrom_mode_default_maps_sequential_banks() {
        let mapper = make_mapper();
        // reg=0: NROM mode, base=(0 & 0x0F)<<2=0
        // slot 0 ($8000) → bank 0
        // slot 1 ($A000) → bank 1
        // slot 2 ($C000) → bank 2
        // slot 3 ($E000) → bank 3
        assert_eq!(mapper.read_prg(0x8000), 0, "slot 0 → bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "slot 1 → bank 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "slot 2 → bank 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "slot 3 → bank 3");
    }

    #[test]
    fn nrom_mode_outer_reg_selects_32kb_block() {
        let mut mapper = make_mapper();
        // reg = 1: base = (1 & 0x0F) << 2 = 4
        // slots: 4, 5, 6, 7; 7 % 9 = 7
        mapper.write_prg(0x9000, 1); // bit 12 set → outer reg
        assert_eq!(mapper.read_prg(0x8000), 4, "slot 0 → bank 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "slot 1 → bank 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "slot 2 → bank 6");
        assert_eq!(
            mapper.read_prg(0xE000),
            7 % PRG_8K_BANKS as u8,
            "slot 3 → bank 7%9"
        );
    }

    #[test]
    fn nrom_mode_outer_reg_wraps_when_exceeds_prg_count() {
        let mut mapper = make_mapper();
        // reg = 2: base = 8; slots: 8,9,10,11; 8%9=8, 9%9=0, 10%9=1, 11%9=2
        mapper.write_prg(0xB000, 2); // bit 12 set → outer reg
        assert_eq!(mapper.read_prg(0x8000), 8 % PRG_8K_BANKS as u8);
        assert_eq!(mapper.read_prg(0xA000), 9 % PRG_8K_BANKS as u8);
        assert_eq!(mapper.read_prg(0xC000), 10 % PRG_8K_BANKS as u8);
        assert_eq!(mapper.read_prg(0xE000), 11 % PRG_8K_BANKS as u8);
    }

    // ------------------------------------------------------------------
    // PRG banking – MMC3 mode (reg bit 4 set)
    // ------------------------------------------------------------------

    #[test]
    fn mmc3_mode_prg_uses_mmc3_registers_with_outer_high_bits() {
        let mut mapper = make_mapper();
        // reg = 0x10 (M=1, OO=0): mmc3_mode, outer=(0 & 0x0C)<<2=0
        // Set R6=3 via standard MMC3 write ($8000 = bank_select, $8001 = bank_data)
        mapper.write_prg(0x9000, 0x10); // outer reg
        mapper.write_prg(0x8000, 0x06); // bank_select → R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        // actual = 0 | (3 & 0x0F) = 3; 3 % 9 = 3
        assert_eq!(mapper.read_prg(0x8000), 3 % PRG_8K_BANKS as u8);
    }

    #[test]
    fn mmc3_mode_outer_bits_shift_prg_window() {
        let mut mapper = make_mapper();
        // reg = 0x14 (M=1, OO=01): outer = (0x04 & 0x0C) << 2 = 0x10 = 16
        mapper.write_prg(0x9000, 0x14); // outer reg
        mapper.write_prg(0x8000, 0x06); // bank_select → R6
        mapper.write_prg(0x8001, 0); // R6 = 0
        // actual = 16 | 0 = 16; 16 % 9 = 7
        assert_eq!(
            mapper.read_prg(0x8000),
            16usize.wrapping_rem(PRG_8K_BANKS) as u8,
            "outer OO=1 → bank 16"
        );
    }

    #[test]
    fn mmc3_mode_inner_bank_masked_to_4_bits() {
        let mut mapper = make_mapper();
        // reg = 0x10 (MMC3 mode, outer=0)
        mapper.write_prg(0x9000, 0x10);
        // R6 = 0x1F = 31; inner = 31 & 0x0F = 15
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x1F);
        // actual = 0 | 15 = 15; 15 % 9 = 6
        assert_eq!(
            mapper.read_prg(0x8000),
            15usize.wrapping_rem(PRG_8K_BANKS) as u8,
            "inner masked to 4 bits"
        );
    }

    // ------------------------------------------------------------------
    // Writes to $8xxx, $Axxx, $Cxxx, $Exxx forwarded to MMC3
    // ------------------------------------------------------------------

    #[test]
    fn writes_without_bit12_go_to_mmc3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x10); // MMC3 mode
        // Write to $8000/$8001 (no bit 12): goes to MMC3
        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 5); // R6 = 5
        // actual = (0x10 & 0x0C) << 2 | (5 & 0x0F) = 0 | 5 = 5; 5 % 9 = 5
        assert_eq!(mapper.read_prg(0x8000), 5 % PRG_8K_BANKS as u8);
    }

    #[test]
    fn writes_with_bit12_do_not_go_to_mmc3() {
        let mut mapper = make_mapper();
        // $9000 write should NOT be treated as MMC3's $A000 (mirroring)
        // Write 0 to $9000: sets reg=0 (NROM mode), doesn't change mirroring
        mapper.write_prg(0xA000, 0x01); // MMC3 mirroring = horizontal
        mapper.write_prg(0x9000, 0xFF); // outer reg, should NOT affect MMC3 mirroring
        // Mirroring should still be horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ------------------------------------------------------------------
    // CHR banking
    // ------------------------------------------------------------------

    #[test]
    fn chr_default_outer_zero_uses_mmc3_inner_bank() {
        let mut mapper = make_mapper();
        // reg=0: CHR outer=0; inner=mmc3_1k_page
        // Write R2=5 (CHR slot 4, $1000 in CHR mode 0)
        mapper.write_prg(0x8000, 0x02); // bank_select → R2
        mapper.write_prg(0x8001, 5); // R2 = 5
        // actual = 0 | (5 & 0x7F) = 5; 5 % 48 = 5
        assert_eq!(mapper.read_chr(0x1000), 5 % CHR_1K_BANKS as u8);
    }

    #[test]
    fn chr_outer_bits_from_reg_bits_3_2() {
        let mut mapper = make_mapper();
        // reg = 0x04: CHR outer = (0x04 & 0x0C) << 5 = 4 << 5 = 128
        mapper.write_prg(0x9000, 0x04); // outer reg
        // R2=0 → actual = 128 | 0 = 128; 128 % 48 = 32
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_chr(0x1000),
            128usize.wrapping_rem(CHR_1K_BANKS) as u8,
            "CHR outer OO=1 → bank 128"
        );
    }

    #[test]
    fn chr_inner_bank_masked_to_7_bits() {
        let mut mapper = make_mapper();
        // reg=0; R2=0x80 (bit 7 set) → inner = 0x80 & 0x7F = 0; actual = 0
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x80);
        assert_eq!(mapper.read_chr(0x1000), 0, "CHR inner masked to 7 bits");
    }

    // ------------------------------------------------------------------
    // Mirroring via MMC3 $A000
    // ------------------------------------------------------------------

    #[test]
    fn mirroring_vertical_via_a000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x00); // bit 0=0 → vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_via_a000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x01); // bit 0=1 → horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ------------------------------------------------------------------
    // PRG-RAM
    // ------------------------------------------------------------------

    #[test]
    fn prg_ram_read_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    // ------------------------------------------------------------------
    // Save state
    // ------------------------------------------------------------------

    #[test]
    fn registers_snapshot_and_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x14); // outer reg
        mapper.write_prg(0x8000, 0x06); // bank_select R6
        mapper.write_prg(0x8001, 3); // R6 = 3

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // Check outer reg restored
        assert_eq!(restored.reg, 0x14, "outer reg must be restored");
        // Check PRG banking: reg=0x14 (M=1, OO=01), R6=3 → actual=16|3=19; 19%9=1
        assert_eq!(
            restored.read_prg(0x8000),
            19usize.wrapping_rem(PRG_8K_BANKS) as u8
        );
    }

    #[test]
    fn restore_registers_with_legacy_snapshot_resets_outer_reg() {
        let mut mapper = make_mapper();
        // A legacy snapshot (only MMC3 data, 13 bytes minimum, no outer reg appended)
        let short_snap = vec![0u8; 13];
        mapper.restore_registers(&short_snap);
        assert_eq!(
            mapper.reg, 0,
            "outer reg must default to 0 for legacy snapshots"
        );
    }

    #[test]
    fn restore_registers_with_16byte_snapshot_does_not_misparse_as_outer_reg() {
        let mut mapper = make_mapper();
        // A 16-byte MMC3-only snapshot must NOT be split_last'd into 15 bytes of MMC3
        // state + 1 byte of outer reg. The last byte (formerly a12_low_cycles) must
        // remain part of the MMC3 snapshot, and outer reg must be reset to 0.
        let mut snap16 = vec![0u8; 16];
        // Set the last byte to a sentinel value; old split_last() code would have
        // incorrectly stored it as `reg`.
        snap16[15] = 0xAB;
        mapper.restore_registers(&snap16);
        assert_eq!(
            mapper.reg, 0,
            "16-byte snapshot must not pollute outer reg (legacy MMC3-only snapshot)"
        );
    }
}
