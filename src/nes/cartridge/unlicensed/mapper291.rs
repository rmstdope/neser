//! Mapper 291 – Kasheng 2-in-1 Multicart (MMC3 + outer bank register)
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/NES_2.0_Mapper_291>
//!
//! Hardware summary:
//! - PCB: Kasheng 2-in-1 multicart (Mortal Kombat 6 + Samurai Spirits)
//! - PRG-ROM: 256 KiB (2 × 128 KiB outer blocks, MMC3-banked within each block)
//! - CHR-ROM: 512 KiB (2 × 256 KiB outer blocks, MMC3-banked within each block)
//! - Mirroring: Dynamic via MMC3 $A000 write
//! - IRQ: Standard MMC3 scanline counter (A12 rising-edge)
//! - PRG-RAM: None
//!
//! Outer Bank Register ($6000), write (mask: $E000):
//! ```text
//! 7654 3210
//! ---------
//! ?OM. .PP.
//! |||   ++-- 32 KiB inner PRG bank (bits 2:1) – used when M=1
//! ||+------- PRG banking mode: 0=MMC3 inner (128 KiB), 1=32 KiB direct (PP)
//! |+-------- Outer PRG+CHR bank: 0=first 128 KiB PRG/256 KiB CHR, 1=second
//! +--------- Unknown
//! ```
//!
//! The Outer Bank Register always responds, even when MMC3's WRAM-enable bit is clear.
//!
//! PRG banking (M=0, MMC3 inner):
//! - Effective 8 KiB bank = `(O << 4) | (mmc3_bank & 0x0F)`
//!
//! PRG banking (M=1, 32 KiB direct):
//! - Effective 8 KiB bank = `(O << 4) | (PP << 2) | slot`  (slot = 0..3 from addr)
//!
//! CHR banking (always MMC3):
//! - Effective 1 KiB bank = `(O << 8) | (mmc3_1k_bank & 0xFF)`
//!
//! Known Limitations:
//! - Bit 7 of the outer register has unknown function and is ignored.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 291 – Kasheng 2-in-1 Multicart
pub struct Mapper291 {
    mmc3: MMC3Mapper,
    /// Last value written to $6000 (outer bank register)
    outer_reg: u8,
}

impl Mapper291 {
    const MAPPER_NUMBER: u16 = 291;

    // Masks for outer register fields
    const PRG_PP_MASK: u8 = 0x06; // bits 2:1 – 32 KiB inner bank select
    const MODE_BIT: u8 = 0x20; // bit 5 – 0=MMC3, 1=32 KiB direct
    const OUTER_BIT: u8 = 0x40; // bit 6 – outer PRG/CHR bank select

    // 8KB PRG slot mask within the outer 128 KiB block (16 × 8 KiB banks → 4-bit mask)
    const PRG_INNER_MASK: usize = 0x0F;
    // 1KB CHR slot mask within the outer 256 KiB block (256 × 1 KiB banks → 8-bit mask)
    const CHR_INNER_MASK: usize = 0xFF;

    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        // No PRG-RAM on this board
        let mmc3 = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            0,
        );
        Self { mmc3, outer_reg: 0 }
    }

    /// Return the outer bank bit (bit 6 of outer_reg).
    fn outer(&self) -> usize {
        ((self.outer_reg & Self::OUTER_BIT) >> 6) as usize
    }

    /// Return true when M=1 (32 KiB direct PRG mode).
    fn prg_32kb_mode(&self) -> bool {
        (self.outer_reg & Self::MODE_BIT) != 0
    }

    /// Return the PP field (bits 2:1) as a 2-bit value.
    fn prg_pp(&self) -> usize {
        ((self.outer_reg & Self::PRG_PP_MASK) >> 1) as usize
    }

    /// Compute the effective 8 KiB PRG bank index for a CPU address.
    fn mapped_prg_bank(&self, addr: u16) -> usize {
        let outer = self.outer();
        if self.prg_32kb_mode() {
            // 32 KiB block selected by PP within the outer 128 KiB block
            let pp = self.prg_pp();
            let slot = ((addr as usize).saturating_sub(0x8000)) >> 13;
            (outer << 4) | (pp << 2) | (slot & 0x03)
        } else {
            // MMC3 handles banking within the outer 128 KiB block
            let mmc3_bank = self.mmc3.mapped_prg_bank(addr);
            (outer << 4) | (mmc3_bank & Self::PRG_INNER_MASK)
        }
    }

    /// Compute the effective 1 KiB CHR bank index for a PPU address.
    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        let outer = self.outer();
        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr);
        (outer << 8) | (mmc3_bank & Self::CHR_INNER_MASK)
    }
}

impl Mapper for Mapper291 {
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

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = self.mapped_prg_bank(addr);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // Outer bank register always responds, regardless of MMC3 WRAM enable
                self.outer_reg = value;
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
        snap.push(self.outer_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let (mmc3_data, tail) = data.split_at(data.len() - 1);
        self.outer_reg = tail[0];
        self.mmc3.restore_registers(mmc3_data);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer_reg = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::MapperContext;

    // PRG: 256 KiB = 32 × 8 KiB banks
    const PRG_BANKS: usize = 32;
    // CHR: 512 KiB = 512 × 1 KiB banks
    const CHR_BANKS: usize = 512;

    fn make_prg_rom() -> Vec<u8> {
        // Fill each 8 KiB bank with its bank number (mod 256)
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
        // Fill each 1 KiB CHR bank with the low byte of its bank index
        let mut chr = vec![0u8; CHR_BANKS * 0x0400];
        for bank in 0..CHR_BANKS {
            let start = bank * 0x0400;
            for b in chr.iter_mut().take(start + 0x0400).skip(start) {
                *b = bank as u8;
            }
        }
        chr
    }

    fn create_mapper() -> Mapper291 {
        Mapper291::new(MapperContext::new_for_test(
            291,
            make_prg_rom(),
            make_chr_rom(),
            NametableLayout::Horizontal,
        ))
    }

    // ── Outer-register defaults ──────────────────────────────────────────────

    #[test]
    fn test_power_on_outer_reg_is_zero() {
        let mapper = create_mapper();
        assert_eq!(mapper.outer_reg, 0, "outer_reg must be 0 at power-on");
    }

    // ── PRG banking: MMC3 inner mode (M=0, outer=0) ─────────────────────────

    #[test]
    fn test_mmc3_mode_prg_default_outer_zero_e000_is_bank31() {
        let mapper = create_mapper();
        // Default state: outer=0, M=0. MMC3 maps $E000..FFFF to the absolute last bank.
        // With outer=0, the last bank in the first 128 KiB block is bank 15.
        // mapped_prg_bank for $E000 = (0 << 4) | (31 & 0x0F) = 15
        let val = mapper.read_prg(0xE000);
        assert_eq!(val, 15, "$E000 should read bank 15 (last in outer block 0)");
    }

    #[test]
    fn test_mmc3_mode_prg_outer_one_e000_is_bank31() {
        let mut mapper = create_mapper();
        // Set outer=1 (bit 6), M=0: $6000 = 0x40
        mapper.write_prg(0x6000, 0x40);
        // Now outer=1; MMC3 fixed last 8KB bank = 31 → (1<<4)|(31&0x0F) = 31
        let val = mapper.read_prg(0xE000);
        assert_eq!(val, 31, "$E000 with outer=1 should read bank 31");
    }

    #[test]
    fn test_mmc3_mode_prg_bank_register_applies_outer_mask() {
        let mut mapper = create_mapper();
        // Set outer=1 via $6000 bit 6
        mapper.write_prg(0x6000, 0x40);
        // Set MMC3 R6 = 3 ($8000..9FFF → 8KB bank 3 within block)
        mapper.write_prg(0x8000, 0x06); // bank_select = R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        // Effective bank = (1<<4) | (3 & 0x0F) = 19
        let val = mapper.read_prg(0x8000);
        assert_eq!(val, 19, "$8000 with outer=1 and MMC3 R6=3 → bank 19");
    }

    #[test]
    fn test_mmc3_mode_prg_outer_bit_limits_inner_to_128kb() {
        let mut mapper = create_mapper();
        // outer=0: Set MMC3 R6 = 17 → bank 17 in full PRG
        // But with outer=0, mask is 0x0F → 17 & 0x0F = 1 → effective bank 1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 17);
        let val = mapper.read_prg(0x8000);
        assert_eq!(
            val, 1,
            "MMC3 R6=17 with outer=0 should use bank 1 (17 & 0x0F)"
        );
    }

    // ── PRG banking: 32 KiB direct mode (M=1) ───────────────────────────────

    #[test]
    fn test_32kb_mode_pp0_outer0_maps_full_32kb_bank0() {
        let mut mapper = create_mapper();
        // M=1 (bit5), PP=0, outer=0 → $6000 = 0x20
        mapper.write_prg(0x6000, 0x20);
        // PP=0, slot range: $8000→0, $A000→1, $C000→2, $E000→3
        // bank = (0<<4)|(0<<2)|slot = slot
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), 3);
    }

    #[test]
    fn test_32kb_mode_pp1_outer0() {
        let mut mapper = create_mapper();
        // M=1, PP=1 (bits 2:1 = 0b10 → value 0x22)
        mapper.write_prg(0x6000, 0x22); // bit5=1, bit2=1, bit1=0 → PP=1
        // bank = (0<<4)|(1<<2)|slot = 4+slot
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 → bank 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 → bank 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 → bank 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 → bank 7");
    }

    #[test]
    fn test_32kb_mode_pp0_outer1() {
        let mut mapper = create_mapper();
        // M=1 (bit5), outer=1 (bit6), PP=0 → $6000 = 0x60
        mapper.write_prg(0x6000, 0x60);
        // bank = (1<<4)|(0<<2)|slot = 16+slot
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "$8000 with outer=1,PP=0 → bank 16"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            18,
            "$C000 with outer=1,PP=0 → bank 18"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            19,
            "$E000 with outer=1,PP=0 → bank 19"
        );
    }

    #[test]
    fn test_32kb_mode_pp3_outer1() {
        let mut mapper = create_mapper();
        // M=1, PP=3 (bits 2:1 = 0b110 → value 0x06 for PP), outer=1 (bit6) → $6000 = 0x66
        mapper.write_prg(0x6000, 0x66); // 0x40|0x20|0x06
        // bank = (1<<4)|(3<<2)|slot = 16+12+slot = 28+slot
        assert_eq!(
            mapper.read_prg(0x8000),
            28,
            "$8000 with outer=1,PP=3 → bank 28"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "$E000 with outer=1,PP=3 → bank 31"
        );
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn test_chr_outer0_mmc3_bank0_reads_chr_bank0() {
        let mut mapper = create_mapper();
        // outer=0, MMC3 default CHR bank 0 → effective = (0<<8)|(0&0xFF) = 0
        let val = mapper.read_chr(0x0000);
        assert_eq!(val, 0, "CHR bank 0 outer=0 → CHR bank 0");
    }

    #[test]
    fn test_chr_outer1_bank_offset_visible_via_block_fill() {
        // Use a ROM where each 1KB CHR bank carries a unique 2-byte signature
        let chr_banks = 512usize;
        let mut chr = vec![0u8; chr_banks * 0x0400];
        for bank in 0..chr_banks {
            chr[bank * 0x0400] = (bank & 0xFF) as u8;
            chr[bank * 0x0400 + 1] = (bank >> 8) as u8;
        }
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = Mapper291::new(MapperContext::new_for_test(
            291,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));

        // R2 maps to PPU $1000 (no CHR inversion): bank_select = 0x02
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10); // R2 = 10

        // outer=0: bank 10 → high byte = 0
        let hi_outer0 = mapper.read_chr(0x1001);

        // outer=1: bank 266 → high byte = 1
        mapper.write_prg(0x6000, 0x40); // outer=1
        let hi_outer1 = mapper.read_chr(0x1001);

        assert_eq!(hi_outer0, 0, "outer=0 CHR bank 10 high byte must be 0");
        assert_eq!(hi_outer1, 1, "outer=1 CHR bank 266 high byte must be 1");
    }

    #[test]
    fn test_chr_outer_block_isolation() {
        // Use a ROM where each bank is uniquely identifiable across 512KB
        // Use a 2-byte fill where byte 0 = bank & 0xFF and byte 1 = bank >> 8
        let chr_banks = 512usize;
        let mut chr = vec![0u8; chr_banks * 0x0400];
        for bank in 0..chr_banks {
            chr[bank * 0x0400] = (bank & 0xFF) as u8;
            chr[bank * 0x0400 + 1] = (bank >> 8) as u8;
        }
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = Mapper291::new(MapperContext::new_for_test(
            291,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));

        // R2 maps to PPU $1000 (no CHR inversion): bank_select = 0x02
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10); // R2 = 10
        let lo_outer0 = mapper.read_chr(0x1000);
        let hi_outer0 = mapper.read_chr(0x1001);

        // outer=1 → bank 256+10 = 266 → byte[0]=266&0xFF=10, byte[1]=266>>8=1
        mapper.write_prg(0x6000, 0x40);
        let lo_outer1 = mapper.read_chr(0x1000);
        let hi_outer1 = mapper.read_chr(0x1001);

        assert_eq!(lo_outer0, 10, "outer=0 CHR bank 10: low byte = 10");
        assert_eq!(hi_outer0, 0, "outer=0 CHR bank 10: high byte = 0");
        assert_eq!(lo_outer1, 10, "outer=1 CHR bank 266: low byte = 10");
        assert_eq!(
            hi_outer1, 1,
            "outer=1 CHR bank 266: high byte = 1 (bank > 255)"
        );
    }

    // ── $6000 always-writeable ───────────────────────────────────────────────

    #[test]
    fn test_outer_reg_write_to_6000_takes_effect() {
        let mut mapper = create_mapper();
        assert_eq!(mapper.outer_reg, 0);
        mapper.write_prg(0x6000, 0x40);
        assert_eq!(mapper.outer_reg, 0x40);
        mapper.write_prg(0x7FFF, 0x20);
        assert_eq!(mapper.outer_reg, 0x20);
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_roundtrip() {
        let mut mapper = create_mapper();
        mapper.write_prg(0x6000, 0x62); // outer=1, M=1, PP=1
        let snap = mapper.registers_snapshot();
        mapper.outer_reg = 0;
        mapper.restore_registers(&snap);
        assert_eq!(
            mapper.outer_reg, 0x62,
            "outer_reg must survive snapshot roundtrip"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_outer_reg() {
        let mut mapper = create_mapper();
        mapper.write_prg(0x6000, 0x7E);
        mapper.reset();
        assert_eq!(mapper.outer_reg, 0, "reset must clear outer_reg");
    }

    // ── Mapper number ────────────────────────────────────────────────────────

    #[test]
    fn test_mapper_number_is_291() {
        let mapper = create_mapper();
        assert_eq!(mapper.mapper_number(), 291);
    }

    // ── IRQ capabilities ─────────────────────────────────────────────────────

    #[test]
    fn test_capabilities_has_irq_and_chr_banking() {
        let mapper = create_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "mapper 291 must report IRQ capability");
        assert!(caps.has_chr_banking, "mapper 291 must report CHR banking");
        assert!(
            caps.has_dynamic_mirroring,
            "mapper 291 must report dynamic mirroring"
        );
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
    }
}
