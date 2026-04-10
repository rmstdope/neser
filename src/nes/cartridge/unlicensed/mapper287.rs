//! Mapper 287 – BMC-411120C (also used for K-3088 boards)
//!
//! Specifications:
//! - Primary source: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_Bmc411120C.h`
//!   (NesDev wiki page was inaccessible; Mesen2 used as primary reference)
//!
//! Hardware summary:
//! - PCB: BMC-411120C / K-3088 multicart
//! - PRG-ROM: up to 512 KiB (64 × 8 KiB banks)
//! - CHR-ROM: up to 512 KiB (512 × 1 KiB banks)
//! - Mirroring: Dynamic via MMC3 $A000 write
//! - IRQ: Standard MMC3 scanline counter (A12 rising-edge)
//! - PRG-RAM: None
//!
//! Outer Bank Register ($6000–$7FFF), write:
//! - The **address low byte** (not the data byte) is latched as the outer register.
//! ```text
//! 7654 3210
//! ---------
//! ..NN.Boo
//! |||  +++-- outer CHR bank bits [8:7] and outer PRG bank bits [5:4]
//! ||+------- NROM mode when set (bit 3): all 4 PRG slots fixed
//! ++-------- inner NROM bank select bits [5:4] (used only in NROM mode)
//! ```
//!
//! PRG banking (normal, bit 3 = 0):
//! - Effective 8 KiB bank = `(mmc3_bank & 0x0F) | ((exReg & 0x03) << 4)`
//!
//! PRG banking (NROM, bit 3 = 1):
//! - inner = `(exReg >> 4) & 0x03`
//! - All 4 PRG 8 KiB slots → banks `(inner | 0x0C) * 4 + slot` (slot = 0..3)
//!
//! CHR banking (always MMC3):
//! - Effective 1 KiB bank = `(mmc3_1k_bank) | ((exReg & 0x03) << 7)`
//!
//! Known Limitations:
//! - DIP switch not supported (treated as 0); NROM mode activates only when bit 3 is set.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 287 – BMC-411120C multicart
pub struct Mapper287 {
    mmc3: MMC3Mapper,
    /// Low byte of the last address written to $6000–$7FFF
    ex_reg: u8,
}

impl Mapper287 {
    const MAPPER_NUMBER: u16 = 287;

    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    /// Outer CHR/PRG bank bits (bits 1:0 of ex_reg).
    const OUTER_MASK: u8 = 0x03;
    /// NROM mode flag (bit 3 of ex_reg).
    const NROM_BIT: u8 = 0x08;
    /// NROM inner bank bits (bits 5:4 of ex_reg).
    const NROM_INNER_SHIFT: u8 = 4;
    const NROM_INNER_MASK: u8 = 0x03;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mmc3 = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            0,
        );
        Self { mmc3, ex_reg: 0 }
    }

    fn nrom_mode(&self) -> bool {
        (self.ex_reg & Self::NROM_BIT) != 0
    }

    fn outer(&self) -> usize {
        (self.ex_reg & Self::OUTER_MASK) as usize
    }

    /// Compute the effective 8 KiB PRG bank for a CPU address.
    fn mapped_prg_bank(&self, addr: u16) -> usize {
        let outer = self.outer();
        if self.nrom_mode() {
            let inner = ((self.ex_reg >> Self::NROM_INNER_SHIFT) & Self::NROM_INNER_MASK) as usize;
            let base = (inner | 0x0C) * 4;
            let slot = ((addr as usize).saturating_sub(0x8000)) >> 13;
            base + (slot & 0x03)
        } else {
            let mmc3_bank = self.mmc3.mapped_prg_bank(addr);
            (outer << 4) | (mmc3_bank & 0x0F)
        }
    }

    /// Compute the effective 1 KiB CHR bank for a PPU address.
    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        let outer = self.outer();
        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr);
        mmc3_bank | (outer << 7)
    }
}

impl Mapper for Mapper287 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..MapperCapabilities::default()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if addr >= 0x8000 {
            let bank = self.mapped_prg_bank(addr);
            let offset = (addr as usize) & Self::PRG_BANK_MASK;
            self.mmc3.read_prg_at_bank(bank, offset)
        } else {
            self.mmc3.read_prg(addr)
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // Uses address low byte, NOT data
                self.ex_reg = (addr & 0xFF) as u8;
            }
            0x8000..=0xFFFF => {
                self.mmc3.write_prg(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let (mmc3_data, tail) = data.split_at(data.len() - 1);
        self.ex_reg = tail[0];
        self.mmc3.restore_registers(mmc3_data);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.ex_reg = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::MapperContext;

    // PRG: 512 KiB = 64 × 8 KiB banks
    const PRG_BANKS: usize = 64;
    // CHR: 512 KiB = 512 × 1 KiB banks
    const CHR_BANKS: usize = 512;

    fn make_prg_rom() -> Vec<u8> {
        let mut prg = vec![0u8; PRG_BANKS * 0x2000];
        for bank in 0..PRG_BANKS {
            let start = bank * 0x2000;
            for b in prg.iter_mut().take(start + 0x2000).skip(start) {
                *b = bank as u8;
            }
        }
        prg
    }

    fn make_chr_rom() -> Vec<u8> {
        let mut chr = vec![0u8; CHR_BANKS * 0x0400];
        for bank in 0..CHR_BANKS {
            let start = bank * 0x0400;
            chr[start] = (bank & 0xFF) as u8;
            chr[start + 1] = (bank >> 8) as u8;
        }
        chr
    }

    fn create_mapper() -> Mapper287 {
        Mapper287::new(MapperContext::new_for_test(
            287,
            make_prg_rom(),
            make_chr_rom(),
            NametableLayout::Horizontal,
        ))
    }

    // ── Outer register capture (address low byte, not data) ──────────────────

    #[test]
    fn test_outer_reg_uses_address_low_byte_not_data() {
        let mut mapper = create_mapper();
        // Write $6003 with data 0xFF → ex_reg must be 0x03 (addr low byte), not 0xFF
        mapper.write_prg(0x6003, 0xFF);
        assert_eq!(
            mapper.ex_reg, 0x03,
            "ex_reg must latch address low byte 0x03"
        );
        // Write $7008 with data 0x00 → ex_reg must be 0x08
        mapper.write_prg(0x7008, 0x00);
        assert_eq!(
            mapper.ex_reg, 0x08,
            "ex_reg must latch address low byte 0x08"
        );
    }

    #[test]
    fn test_outer_reg_responds_across_6000_to_7fff() {
        let mut mapper = create_mapper();
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.ex_reg, 0x00);
        mapper.write_prg(0x7FFF, 0x00);
        assert_eq!(mapper.ex_reg, 0xFF);
    }

    // ── PRG banking – normal (MMC3) mode ──────────────────────────────────────

    #[test]
    fn test_prg_mmc3_mode_outer0_default_banks() {
        let mapper = create_mapper();
        // Default MMC3: $8000 = bank 0 (second-to-last), $C000 = bank 62 (second-to-last actual)
        // With outer=0, bank = (mmc3_bank & 0x0F) | 0x00
        // Just verify $8000 reads the first PRG bank byte (default MMC3 bank 0)
        let val = mapper.read_prg(0x8000);
        // bank 0 → reads 0x00 (first bank)
        assert_eq!(val, 0, "default MMC3 mode outer=0 reads bank 0 at $8000");
    }

    #[test]
    fn test_prg_mmc3_mode_outer1_shifts_bank_by_16() {
        let mut mapper = create_mapper();
        // outer=1 (bits 1:0 of ex_reg = 0x01) via address $6001
        mapper.write_prg(0x6001, 0x00);
        assert_eq!(mapper.ex_reg, 0x01);

        // MMC3 R6 = 0 (bank_select = 0x06, then 0x8001 = 0)
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        // Effective PRG bank = (0 & 0x0F) | (1 << 4) = 16
        let val = mapper.read_prg(0x8000);
        assert_eq!(val, 16, "outer=1 MMC3 bank0 → effective bank 16");
    }

    #[test]
    fn test_prg_mmc3_mode_outer2_shifts_bank_by_32() {
        let mut mapper = create_mapper();
        // outer=2 via address $6002
        mapper.write_prg(0x6002, 0x00);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        // Effective PRG bank = (0 & 0x0F) | (2 << 4) = 32
        let val = mapper.read_prg(0x8000);
        assert_eq!(val, 32, "outer=2 MMC3 bank0 → effective bank 32");
    }

    #[test]
    fn test_prg_mmc3_mode_inner_bank_masked_to_4_bits() {
        let mut mapper = create_mapper();
        // outer=1, MMC3 R6 = 5 → effective = (5 & 0x0F) | (1 << 4) = 5 | 16 = 21
        mapper.write_prg(0x6001, 0x00);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        let val = mapper.read_prg(0x8000);
        assert_eq!(val, 21, "outer=1 MMC3 bank5 → effective bank 21");
    }

    // ── PRG banking – NROM mode ───────────────────────────────────────────────

    #[test]
    fn test_prg_nrom_mode_inner0_maps_banks_48_to_51() {
        let mut mapper = create_mapper();
        // bit3=1 → $6008; inner bits 5:4 = 0 → NROM inner=0 → base=(0|0x0C)*4=48
        mapper.write_prg(0x6008, 0x00);
        assert_eq!(mapper.ex_reg, 0x08);
        assert_eq!(mapper.read_prg(0x8000), 48, "$8000 → bank 48");
        assert_eq!(mapper.read_prg(0xA000), 49, "$A000 → bank 49");
        assert_eq!(mapper.read_prg(0xC000), 50, "$C000 → bank 50");
        assert_eq!(mapper.read_prg(0xE000), 51, "$E000 → bank 51");
    }

    #[test]
    fn test_prg_nrom_mode_inner1_maps_banks_52_to_55() {
        let mut mapper = create_mapper();
        // bit3=1, bits 5:4=01 → ex_reg via addr $6018 (bits[5:4]=01, bit3=1 → 0b0001_1000=0x18)
        mapper.write_prg(0x6018, 0x00);
        assert_eq!(mapper.ex_reg, 0x18);
        assert_eq!(mapper.read_prg(0x8000), 52);
        assert_eq!(mapper.read_prg(0xA000), 53);
        assert_eq!(mapper.read_prg(0xC000), 54);
        assert_eq!(mapper.read_prg(0xE000), 55);
    }

    #[test]
    fn test_prg_nrom_mode_inner2_maps_banks_56_to_59() {
        let mut mapper = create_mapper();
        // bits 5:4=10, bit3=1 → 0b0010_1000 = 0x28
        mapper.write_prg(0x6028, 0x00);
        assert_eq!(mapper.ex_reg, 0x28);
        assert_eq!(mapper.read_prg(0x8000), 56);
        assert_eq!(mapper.read_prg(0xA000), 57);
        assert_eq!(mapper.read_prg(0xC000), 58);
        assert_eq!(mapper.read_prg(0xE000), 59);
    }

    #[test]
    fn test_prg_nrom_mode_inner3_maps_banks_60_to_63() {
        let mut mapper = create_mapper();
        // bits 5:4=11, bit3=1 → 0b0011_1000 = 0x38
        mapper.write_prg(0x6038, 0x00);
        assert_eq!(mapper.ex_reg, 0x38);
        assert_eq!(mapper.read_prg(0x8000), 60);
        assert_eq!(mapper.read_prg(0xA000), 61);
        assert_eq!(mapper.read_prg(0xC000), 62);
        assert_eq!(mapper.read_prg(0xE000), 63);
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn test_chr_outer0_reads_mmc3_bank() {
        let mut mapper = create_mapper();
        // ex_reg = 0 (outer=0) – MMC3 R2 maps to PPU $1000
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10); // R2 = 10
        // Effective CHR bank = 10 | (0 << 7) = 10
        let lo = mapper.read_chr(0x1000);
        assert_eq!(lo, 10, "CHR outer=0 bank 10 low byte must be 10");
    }

    #[test]
    fn test_chr_outer1_adds_128_to_mmc3_bank() {
        let mut mapper = create_mapper();
        // outer=1 via $6001
        mapper.write_prg(0x6001, 0x00);
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10); // R2 = 10
        // Effective CHR bank = 10 | (1 << 7) = 10 | 128 = 138
        // chr[138 * 0x400] = 138 & 0xFF = 138; chr[...+1] = 138 >> 8 = 0
        let lo = mapper.read_chr(0x1000);
        assert_eq!(lo, 138, "CHR outer=1 bank 138 low byte");
        let hi = mapper.read_chr(0x1001);
        assert_eq!(hi, 0, "CHR outer=1 bank 138 high byte");
    }

    #[test]
    fn test_chr_outer2_adds_256_to_mmc3_bank() {
        let mut mapper = create_mapper();
        // outer=2 via $6002
        mapper.write_prg(0x6002, 0x00);
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10);
        // Effective CHR bank = 10 | (2 << 7) = 10 | 256 = 266
        // chr[266 * 0x400 + 0] = 266 & 0xFF = 10; chr[...+1] = 266 >> 8 = 1
        let lo = mapper.read_chr(0x1000);
        assert_eq!(lo, 10, "CHR outer=2 bank 266 low byte must be 10");
        let hi = mapper.read_chr(0x1001);
        assert_eq!(hi, 1, "CHR outer=2 bank 266 high byte must be 1");
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn test_snapshot_roundtrip_preserves_ex_reg() {
        let mut mapper = create_mapper();
        mapper.write_prg(0x6038, 0x00); // ex_reg = 0x38
        let snap = mapper.registers_snapshot();
        mapper.ex_reg = 0;
        mapper.restore_registers(&snap);
        assert_eq!(
            mapper.ex_reg, 0x38,
            "ex_reg must survive snapshot roundtrip"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_ex_reg() {
        let mut mapper = create_mapper();
        mapper.write_prg(0x6038, 0x00);
        mapper.reset();
        assert_eq!(mapper.ex_reg, 0, "reset must clear ex_reg");
    }

    // ── Mapper number ─────────────────────────────────────────────────────────

    #[test]
    fn test_mapper_number_is_287() {
        let mapper = create_mapper();
        assert_eq!(mapper.mapper_number(), 287);
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn test_capabilities() {
        let mapper = create_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn test_mapper_is_registered_in_factory() {
        use crate::nes::cartridge::mapper::create_mapper;
        create_mapper(MapperContext::new_for_test(
            287,
            make_prg_rom(),
            make_chr_rom(),
            NametableLayout::Horizontal,
        ))
        .expect("mapper 287 must be creatable from factory");
    }
}
