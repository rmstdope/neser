//! Mapper 322 – BMC-K-3033 (35-in-1 multicart)
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki `INES_Mapper_322` (HTTP 403 at implementation time)
//! - Fallback source: FCEUMM `src/mappers/mapper322.c` (K-3033 board)
//!
//! ## Overview
//!
//! K-3033 is an MMC3-based multicart board ("35-in-1"). It wraps the standard MMC3
//! with a single outer bank register that selects a block of PRG/CHR ROM pages for
//! the inner MMC3 bank registers to address. Writes to `$6000–$7FFF` store the
//! **address low byte** (A[7:0]) as the outer register value (not the data byte).
//!
//! ## Outer bank register (`$6000–$7FFF`, write)
//!
//! ```text
//! reg = addr & 0xFF   (address low byte)
//! ```
//!
//! Bit layout of `reg`:
//! ```text
//! Bit 7: Size flag – 0 = inner 128 KB PRG/1 MB CHR window, 1 = inner 256 KB PRG/2 MB CHR window
//! Bit 6: Outer PRG/CHR bank bit 2
//! Bit 5: MMC3 bypass flag – 0 = fixed PRG window, 1 = pass-through MMC3 PRG banking
//! Bit 4: Outer PRG/CHR bank bit 1
//! Bit 3: Outer PRG/CHR bank bit 0
//! Bits[2:0]: Inner PRG page select (used in bypass mode)
//! ```
//!
//! Outer bank index: `outer = ((reg >> 4) & 0x04) | ((reg >> 3) & 0x03)` → 3-bit value 0–7
//!
//! ## PRG banking
//!
//! ### Bypass mode (bit 5 = 0)
//!
//! The inner MMC3 PRG registers are ignored for $8000–$FFFF:
//!
//! ```text
//! bank_16k = (outer << 3) | (reg & 0x07)
//! ```
//!
//! If `reg & 0x03 != 0` → 32 KB fixed: both halves of $8000–$FFFF map to `bank_16k >> 1`
//! in 32 KB units.
//!
//! If `reg & 0x03 == 0` → 16 KB mirror: both halves of $8000–$FFFF map to the same
//! `bank_16k` in 16 KB units.
//!
//! ### MMC3 mode (bit 5 = 1)
//!
//! The MMC3's raw 8 KB page number is masked by the outer bank:
//!
//! ```text
//! prg_base = outer << 4      (8 KB units)
//! prg_mask = reg[7] ? 0x1F : 0x0F  (inner window: 256 KB or 128 KB)
//! bank_8k  = (prg_base & ~prg_mask) | (mmc3_raw_page & prg_mask)
//! ```
//!
//! ## CHR banking
//!
//! Always uses MMC3 CHR registers with outer bank masking (same formula as MMC3 mode PRG):
//!
//! ```text
//! chr_base = outer << 7      (1 KB units)
//! chr_mask = reg[7] ? 0xFF : 0x7F  (inner window: 2 MB or 1 MB)
//! bank_1k  = (chr_base & ~chr_mask) | (mmc3_raw_chr_page & chr_mask)
//! ```
//!
//! ## IRQ
//!
//! Inherited from the inner MMC3 unmodified.
//!
//! ## Mirroring
//!
//! Controlled by the inner MMC3 ($A000 register), same as standard MMC3.
//!
//! ## Save state
//!
//! Snapshot = 16 bytes (MMC3 core) + 1 byte (`reg`) = 17 bytes total.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::nintendo::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 322;
const MMC3_SNAPSHOT_LEN: usize = 16;
const REGISTERS_SNAPSHOT_LEN: usize = MMC3_SNAPSHOT_LEN + 1;

/// Mapper 322 – BMC-K-3033 multicart.
///
/// Wraps MMC3 with an outer bank register written at `$6000–$7FFF` (address low
/// byte used, not the data byte). Outer bank masking is applied to PRG and CHR.
pub struct Mapper322 {
    mmc3: MMC3Mapper,
    reg: u8,
}

impl Mapper322 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
                ctx.prg_rom,
                ctx.chr_rom,
                ctx.mirroring,
                false, // standard MMC3 IRQ
                0,     // no PRG-RAM; $6000–$7FFF is the outer-bank register
            ),
            reg: 0,
        }
    }

    /// Outer 3-bit bank index derived from `reg`:
    /// `outer = ((reg >> 4) & 0x04) | ((reg >> 3) & 0x03)`
    fn outer_bank(&self) -> usize {
        (((self.reg >> 4) & 0x04) | ((self.reg >> 3) & 0x03)) as usize
    }

    /// Read one PRG byte from $8000–$FFFF applying outer bank logic.
    fn read_prg_banked(&self, addr: u16) -> u8 {
        let outer = self.outer_bank();

        if self.reg & 0x20 == 0 {
            // Bypass mode: ignore MMC3 PRG registers
            let bank_16k = (outer << 3) | (self.reg & 0x07) as usize;
            let offset = (addr & 0x1FFF) as usize;
            if self.reg & 0x03 != 0 {
                // 32 KB fixed
                let bank_32k = bank_16k >> 1;
                let slot = ((addr & 0x7FFF) >> 13) as usize; // 0–3
                self.mmc3.read_prg_at_bank(bank_32k * 4 + slot, offset)
            } else {
                // 16 KB mirror: both halves point to the same 16 KB bank
                let slot = ((addr & 0x2000) >> 13) as usize; // 0 or 1
                self.mmc3.read_prg_at_bank(bank_16k * 2 + slot, offset)
            }
        } else {
            // MMC3 mode: apply outer bank masking
            let base = outer << 4; // in 8 KB units
            let mask: usize = if self.reg & 0x80 != 0 { 0x1F } else { 0x0F };
            let raw = self.mmc3.raw_prg_8k_page_number(addr) as usize;
            let bank = (base & !mask) | (raw & mask);
            let offset = (addr & 0x1FFF) as usize;
            self.mmc3.read_prg_at_bank(bank, offset)
        }
    }
}

impl Mapper for Mapper322 {
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
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.read_prg_banked(addr),
            _ => self.mmc3.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.reg = (addr & 0xFF) as u8;
            }
            0x8000..=0xFFFF => {
                self.mmc3.write_prg(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let outer = self.outer_bank();
        let base = outer << 7; // in 1 KB units
        let mask: usize = if self.reg & 0x80 != 0 { 0xFF } else { 0x7F };
        let raw = self.mmc3.raw_chr_1k_bank(addr);
        let bank = (base & !mask) | (raw & mask);
        let offset = (addr & 0x3FF) as usize;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= REGISTERS_SNAPSHOT_LEN {
            self.mmc3.restore_registers(&data[..MMC3_SNAPSHOT_LEN]);
            self.reg = data[MMC3_SNAPSHOT_LEN];
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
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn reset(&mut self) {
        self.reg = 0;
        self.mmc3.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANK_SIZE: usize = 8 * 1024;
    const CHR_BANK_SIZE: usize = 1024;

    fn make_mapper(prg_banks: usize, chr_banks: usize) -> Mapper322 {
        Mapper322::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, prg_banks),
            banked_data(CHR_BANK_SIZE, chr_banks),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_322_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, 8),
            banked_data(CHR_BANK_SIZE, 8),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 322 must be registered in factory");
    }

    // ── Outer bank register ──────────────────────────────────────────────────

    /// Writing to $6000–$7FFF stores addr & 0xFF as the outer register, not the value.
    #[test]
    fn write_6000_7fff_stores_address_low_byte_as_reg() {
        let mut m = make_mapper(32, 8);
        // Write addr $60AB (value ignored)
        m.write_prg(0x60AB, 0xFF);
        assert_eq!(m.reg, 0xAB, "outer reg should be addr low byte $AB");
    }

    #[test]
    fn write_different_addresses_use_addr_low_byte() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x6000, 0xFF);
        assert_eq!(m.reg, 0x00);
        m.write_prg(0x6020, 0x00);
        assert_eq!(m.reg, 0x20);
        m.write_prg(0x7FFF, 0x00);
        assert_eq!(m.reg, 0xFF);
    }

    #[test]
    fn writes_below_6000_are_ignored() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x5FFF, 0x55);
        assert_eq!(m.reg, 0x00, "writes below $6000 should not change reg");
    }

    // ── outer_bank() derivation ──────────────────────────────────────────────

    /// outer = ((reg >> 4) & 0x04) | ((reg >> 3) & 0x03)
    ///
    /// | reg  | reg[6] | reg[4] | reg[3] | outer |
    /// |------|--------|--------|--------|-------|
    /// | 0x00 |   0    |   0    |   0    |   0   |
    /// | 0x08 |   0    |   0    |   1    |   1   |
    /// | 0x10 |   0    |   1    |   0    |   2   |
    /// | 0x18 |   0    |   1    |   1    |   3   |
    /// | 0x40 |   1    |   0    |   0    |   4   |
    /// | 0x48 |   1    |   0    |   1    |   5   |
    /// | 0x58 |   1    |   1    |   1    |   7   |
    #[test]
    fn outer_bank_decodes_bits_correctly() {
        let cases: &[(u8, usize)] = &[
            (0x00, 0),
            (0x08, 1),
            (0x10, 2),
            (0x18, 3),
            (0x40, 4),
            (0x48, 5),
            (0x50, 6),
            (0x58, 7),
        ];
        for &(reg, expected) in cases {
            let mut m = make_mapper(32, 8);
            // Set reg via write (addr low byte = reg)
            m.write_prg(0x6000 | reg as u16, 0);
            assert_eq!(
                m.outer_bank(),
                expected,
                "reg={:#04X} → outer should be {}",
                reg,
                expected
            );
        }
    }

    // ── Bypass mode (bit 5 = 0) PRG banking ─────────────────────────────────

    /// When reg[5]=0 and reg[1:0]=0, both $8000–$BFFF and $C000–$FFFF mirror the
    /// same 16 KB bank determined by (outer<<3)|(reg[2:0]).
    #[test]
    fn bypass_16kb_mirror_mode_both_halves_same_bank() {
        // 32 8KB PRG banks = 16 16KB banks
        // Use 32 8KB banks = 256 KB
        let mut m = make_mapper(32, 8);
        // reg = 0x04 → outer=0, bit5=0, bits[1:0]=0 (16KB mirror), bits[2:0]=4
        // bank_16k = (0 << 3) | 4 = 4
        // $8000-$9FFF → 8KB bank 8, $A000-$BFFF → 8KB bank 9
        // $C000-$DFFF → 8KB bank 8, $E000-$FFFF → 8KB bank 9
        m.write_prg(0x6004, 0);
        assert_eq!(m.read_prg(0x8000), 8, "$8000 should read bank 8");
        assert_eq!(m.read_prg(0xA000), 9, "$A000 should read bank 9");
        assert_eq!(m.read_prg(0xC000), 8, "$C000 should mirror to bank 8");
        assert_eq!(m.read_prg(0xE000), 9, "$E000 should mirror to bank 9");
    }

    /// When reg[5]=0 and reg[1:0]!=0, the full 32 KB $8000–$FFFF maps to a fixed
    /// 32 KB bank determined by bank_16k >> 1.
    #[test]
    fn bypass_32kb_fixed_mode_all_windows_same_block() {
        // 32 8KB PRG banks
        let mut m = make_mapper(32, 8);
        // reg = 0x01 → outer=0, bit5=0, bits[1:0]=1 (32KB mode), bits[2:0]=1
        // bank_16k = (0 << 3) | 1 = 1
        // bank_32k = 0 (bank_16k >> 1 = 0)
        // $8000-$9FFF → 8KB bank 0
        // $A000-$BFFF → 8KB bank 1
        // $C000-$DFFF → 8KB bank 2
        // $E000-$FFFF → 8KB bank 3
        m.write_prg(0x6001, 0);
        assert_eq!(m.read_prg(0x8000), 0, "$8000 should be bank 0");
        assert_eq!(m.read_prg(0xA000), 1, "$A000 should be bank 1");
        assert_eq!(m.read_prg(0xC000), 2, "$C000 should be bank 2");
        assert_eq!(m.read_prg(0xE000), 3, "$E000 should be bank 3");
    }

    #[test]
    fn bypass_32kb_mode_with_outer_1() {
        // 64 8KB banks
        let mut m = make_mapper(64, 8);
        // reg = 0x09 → outer = ((9>>4)&4)|((9>>3)&3) = 0|(1&3) = 1,
        //                 bit5=0, bits[1:0]=1 (32KB), bits[2:0]=1
        // outer=1: bank_16k = (1<<3)|1 = 9, bank_32k = 4
        // $8000-$9FFF → 8KB bank 16
        // $A000-$BFFF → 8KB bank 17
        // $C000-$DFFF → 8KB bank 18
        // $E000-$FFFF → 8KB bank 19
        m.write_prg(0x6009, 0);
        assert_eq!(m.outer_bank(), 1);
        assert_eq!(m.read_prg(0x8000), 16, "$8000 should be bank 16");
        assert_eq!(m.read_prg(0xA000), 17, "$A000 should be bank 17");
        assert_eq!(m.read_prg(0xC000), 18, "$C000 should be bank 18");
        assert_eq!(m.read_prg(0xE000), 19, "$E000 should be bank 19");
    }

    // ── MMC3 pass-through mode (bit 5 = 1) PRG banking ──────────────────────

    /// When bit 5 set, MMC3's raw 8KB page is OR-masked with the outer bank.
    #[test]
    fn mmc3_mode_outer_0_passes_through_mmc3_pages() {
        // 16 8KB banks
        let mut m = make_mapper(16, 8);
        // reg = 0x20 → outer=0, bit5=1, bit7=0 (mask=0x0F), bits[2:0]=0
        m.write_prg(0x6020, 0);
        // Default MMC3 maps $E000-$FFFF to last bank (0xFFFF page = last = 15 for 16 banks)
        // raw_prg_8k_page_number for $E000 = 0xFF
        // bank = (0 & ~0x0F) | (0xFF & 0x0F) = 0x0F = 15
        assert_eq!(m.read_prg(0xE000), 15, "last fixed bank should be 15");
        // raw page for $C000 in default MMC3 mode = 0xFE
        // bank = 0 | (0xFE & 0x0F) = 0x0E = 14
        assert_eq!(
            m.read_prg(0xC000),
            14,
            "second-to-last fixed bank should be 14"
        );
    }

    #[test]
    fn mmc3_mode_outer_1_shifts_prg_by_128kb() {
        // 32 8KB banks = 256 KB
        let mut m = make_mapper(32, 8);
        // reg = 0x28 → outer = ((0x28>>4)&4) | ((0x28>>3)&3) = ((2)&4) | ((5)&3) = 0 | (1) = 1,
        //               bit5=1, bit7=0 (mask=0x0F)
        // outer=1 → base = 1 << 4 = 16
        m.write_prg(0x6028, 0);
        assert_eq!(m.outer_bank(), 1);
        // $E000: raw=0xFF, bank = (16 & ~0x0F) | (0xFF & 0x0F) = 16 | 15 = 31
        assert_eq!(m.read_prg(0xE000), 31, "outer=1 last bank = 31");
        // $C000: raw=0xFE, bank = 16 | 14 = 30
        assert_eq!(m.read_prg(0xC000), 30, "outer=1 second-to-last = 30");
    }

    #[test]
    fn mmc3_mode_mmc3_bank_reg_select_works_with_outer() {
        // 32 8KB banks
        let mut m = make_mapper(32, 8);
        // Set outer=1, bit5=1, bit7=0 (mask=0x0F) via reg=0x28
        m.write_prg(0x6028, 0);
        // Set MMC3 R6 to 5 (banked at $8000 in non-prg_mode)
        m.write_prg(0x8000, 6); // select R6
        m.write_prg(0x8001, 5); // R6 = 5
        // outer=1, base=16, mask=0x0F
        // raw_page for $8000 in non-prg_mode = R6 = 5
        // bank = (16 & ~0x0F) | (5 & 0x0F) = 16 | 5 = 21
        assert_eq!(
            m.read_prg(0x8000),
            21,
            "$8000 with R6=5, outer=1 should be bank 21"
        );
    }

    #[test]
    fn mmc3_mode_large_window_bit7_expands_mask_to_0x1f() {
        // 64 8KB banks = 512 KB
        let mut m = make_mapper(64, 8);
        // reg = 0x30 = 0011 0000: bit7=0, bit5=1, bit4=1, bit3=0
        // outer = ((0x30>>4)&4)|((0x30>>3)&3) = (0x03&4)|(0x06&3) = 0|2 = 2
        // base = 2 << 4 = 32, mask = 0x0F (bit7=0)
        // $E000: raw=0xFF, bank = (32 & ~0x0F) | (0xFF & 0x0F) = 32 | 15 = 47
        // $C000: raw=0xFE, bank = 32 | 14 = 46
        m.write_prg(0x6030, 0);
        assert_eq!(m.outer_bank(), 2);
        assert_eq!(m.read_prg(0xE000), 47, "bit7=0 mask=0x0F: last bank = 47");
        assert_eq!(m.read_prg(0xC000), 46, "bit7=0: second-to-last = 46");

        // reg = 0xB0 = 1011 0000: bit7=1, bit5=1, bit4=1, bit3=0 (outer still=2)
        // base = 32, mask = 0x1F (bit7=1)
        // $E000: raw=0xFF, bank = (32 & ~0x1F) | (0xFF & 0x1F) = 32 | 31 = 63
        // $C000: raw=0xFE, bank = 32 | 30 = 62
        m.write_prg(0x60B0, 0);
        assert_eq!(m.outer_bank(), 2);
        assert_eq!(m.read_prg(0xE000), 63, "bit7=1 mask=0x1F: last bank = 63");
        assert_eq!(m.read_prg(0xC000), 62, "bit7=1: second-to-last = 62");
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn chr_outer_0_passes_through_mmc3_chr() {
        // 8 1KB CHR banks (standard)
        let mut m = make_mapper(16, 8);
        // reg = 0x20 → outer=0, bit5=1, bit7=0 (chr_mask=0x7F)
        m.write_prg(0x6020, 0);
        // Default MMC3 CHR registers: R0=0, so CHR bank 0 at $0000-$03FF
        assert_eq!(m.read_chr(0x0000), 0, "CHR bank 0 at $0000");
    }

    #[test]
    fn chr_outer_1_shifts_chr_base_by_128kb() {
        // 256 1KB CHR banks = 256 KB
        let mut m = make_mapper(16, 256);
        // reg = 0x28 → outer=1, bit5=1, bit7=0 (chr_mask=0x7F)
        // chr_base = 1 << 7 = 128
        // R0 = 0 (default) → raw=0, bank = (128 & ~0x7F) | (0 & 0x7F) = 128 | 0 = 128
        // banked_data bank 128 contains byte value 128
        m.write_prg(0x6028, 0);
        assert_eq!(
            m.read_chr(0x0000),
            128u8,
            "CHR outer=1 base shifted to bank 128"
        );
    }

    #[test]
    fn chr_outer_1_with_mmc3_r0_set() {
        // 256 1KB CHR banks
        let mut m = make_mapper(16, 256);
        // Set outer=1, chr_mask=0x7F via reg=0x28
        m.write_prg(0x6028, 0);
        // Set MMC3 R0 = 4 (selects 2KB block at $0000 in CHR mode 0)
        m.write_prg(0x8000, 0); // select R0
        m.write_prg(0x8001, 4); // R0 = 4 (even-aligned 2KB, so $0000-$03FF = bank 4)
        // raw CHR bank for $0000 in CHR mode 0: r0 & 0xFE = 4 & 0xFE = 4
        // bank = (128 & ~0x7F) | (4 & 0x7F) = 128 | 4 = 132
        assert_eq!(m.read_chr(0x0000), 132, "CHR outer=1, R0=4 → bank 132");
    }

    #[test]
    fn chr_bit7_controls_chr_inner_window_size() {
        // 256 1KB CHR banks
        let mut m = make_mapper(16, 256);
        // Set R0=0x82 so raw CHR for $0000 = r0 & 0xFE = 0x82 (bit7 set)
        m.write_prg(0x8000, 0); // select R0
        m.write_prg(0x8001, 0x82); // R0 = 0x82

        // reg=0x20: outer=0, bit5=1, bit7=0 → chr_mask=0x7F
        // bank = (0 & ~0x7F) | (0x82 & 0x7F) = 0 | 0x02 = 2
        m.write_prg(0x6020, 0);
        assert_eq!(
            m.read_chr(0x0000),
            2,
            "bit7=0: mask 0x7F clips CHR bit7 → bank 2"
        );

        // reg=0xA0 = 1010 0000: bit7=1, bit6=0, bit5=1, bit4=0, bit3=0
        // outer = ((0xA0>>4)&4)|((0xA0>>3)&3) = (0x0A&4)|(0x14&3) = 0|0 = 0
        // chr_mask=0xFF, bank = (0 & ~0xFF) | (0x82 & 0xFF) = 0 | 0x82 = 130
        m.write_prg(0x60A0, 0);
        assert_eq!(
            m.read_chr(0x0000),
            130,
            "bit7=1: mask 0xFF passes CHR bit7 → bank 130"
        );
    }

    // ── MMC3 IRQ passthrough ─────────────────────────────────────────────────

    #[test]
    fn mmc3_irq_registers_are_forwarded() {
        let mut m = make_mapper(16, 8);
        // Set IRQ latch to 5 via $C000
        m.write_prg(0xC000, 5);
        // $C001 reloads IRQ counter (no assertion needed, just checking it doesn't crash)
        m.write_prg(0xC001, 0);
        // $E000 disables IRQ
        m.write_prg(0xE000, 0);
        // $E001 enables IRQ
        m.write_prg(0xE001, 0);
        // No assertion needed – test that writes don't panic and don't corrupt outer reg
        assert_eq!(m.reg, 0, "outer reg should still be 0 after MMC3 writes");
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_follows_mmc3_a000_register() {
        use crate::cartridge::NametableLayout;
        let mut m = make_mapper(16, 8);
        // MMC3 default mirroring matches header (Vertical)
        assert_eq!(m.get_mirroring(), NametableLayout::Vertical);
        // $A000 bit0 = 1 → Horizontal
        m.write_prg(0xA000, 1);
        assert_eq!(m.get_mirroring(), NametableLayout::Horizontal);
        // $A000 bit0 = 0 → Vertical
        m.write_prg(0xA000, 0);
        assert_eq!(m.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_outer_reg() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x6028, 0); // reg = 0x28
        let snap = m.registers_snapshot();
        assert_eq!(
            snap.len(),
            17,
            "snapshot should be 17 bytes (16 MMC3 + 1 reg)"
        );
        assert_eq!(snap[16], 0x28, "last byte should be outer reg");

        let mut m2 = make_mapper(32, 8);
        m2.restore_registers(&snap);
        assert_eq!(m2.reg, 0x28, "reg should be restored");
        // With outer=1, bit5=1, mask=0x0F: $E000 bank = 16 | 15 = 31
        assert_eq!(m2.read_prg(0xE000), 31, "PRG bank should be restored");
    }

    #[test]
    fn snapshot_restore_preserves_mmc3_state() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x6020, 0); // outer=0, MMC3 mode
        m.write_prg(0x8000, 6); // select R6
        m.write_prg(0x8001, 3); // R6 = 3
        let snap = m.registers_snapshot();

        let mut m2 = make_mapper(32, 8);
        m2.restore_registers(&snap);
        // outer=0, mask=0x0F, R6=3 → bank = 0 | (3 & 0x0F) = 3
        assert_eq!(m2.read_prg(0x8000), 3, "MMC3 PRG bank should be restored");
    }

    #[test]
    fn legacy_restore_16_bytes_resets_outer_reg_to_zero() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x6028, 0); // reg = 0x28
        m.write_prg(0x8000, 6);
        m.write_prg(0x8001, 5);

        let snap = m.registers_snapshot();
        let legacy = snap[..16].to_vec();

        let mut m2 = make_mapper(32, 8);
        m2.write_prg(0x6028, 0); // pre-set reg to non-zero
        m2.restore_registers(&legacy);
        assert_eq!(m2.reg, 0, "legacy restore should reset outer reg to 0");
        // With reg=0 (bypass mode, outer=0, bits[1:0]=0): 16KB mirror
        // bank_16k=0, $8000-$9FFF → bank 0
        // But MMC3 state from legacy snap: R6=5, which is ignored in bypass mode
        assert_eq!(
            m2.read_prg(0x8000),
            0,
            "bypass mode with reg=0: bank 0 at $8000"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_outer_reg_to_zero() {
        let mut m = make_mapper(32, 8);
        m.write_prg(0x6028, 0); // reg = 0x28
        m.reset();
        assert_eq!(m.reg, 0, "reset should clear outer reg");
    }

    #[test]
    fn power_on_state_is_outer_0_bypass_mode_16kb_mirror() {
        // With reg=0: outer=0, bit5=0, bits[1:0]=0 → 16KB mirror, bank_16k=0
        // $8000-$9FFF → bank 0, $C000-$DFFF → bank 0 (mirror)
        let m = make_mapper(32, 8);
        assert_eq!(m.read_prg(0x8000), 0, "power-on $8000 = bank 0");
        assert_eq!(m.read_prg(0xC000), 0, "power-on $C000 mirrors to bank 0");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_mmc3_variant() {
        let m = make_mapper(16, 8);
        let caps = m.capabilities();
        assert!(caps.has_irq, "should have IRQ");
        assert!(caps.has_chr_banking, "should have CHR banking");
        assert!(caps.has_dynamic_mirroring, "should have dynamic mirroring");
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
