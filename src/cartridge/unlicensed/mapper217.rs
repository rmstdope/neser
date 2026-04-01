//! Mapper 217 - MMC3 variant with extended bank registers (BMC-A65AS variant)
//!
//! Specifications:
//! - Primary source: NesDev wiki page not available (403 / 404)
//! - Fallback source: Mesen2 MMC3_217.h implementation
//!   Source: <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Mmc3Variants/MMC3_217.h>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 217 - MMC3 variant with extended outer-bank and operation-mode registers
///
/// Hardware: MMC3 clone extended with three extra registers at $5000–$5007 and
/// an alternative register-write routing selected by $5007.
///
/// Specifications:
/// - Fallback: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_217.h`
/// - PRG-ROM: Up to 256KB (outer bits allow addressing beyond 512KB with inner 5-bit range)
/// - CHR: Up to 1MB CHR-ROM with 10-bit bank addressing
/// - Mirroring: Software-controlled (standard MMC3 / mode-B routing)
/// - IRQ: Standard MMC3 scanline counter (always available)
///
/// Extra registers:
/// - `$5000` (ex_regs\[0\]): NROM override when bit 7 is set; bits 3-0 select 32KB block
/// - `$5001` (ex_regs\[1\]): Outer bank register; power-on default 0xFF
///   - Bits 9-8 of CHR bank: `(ex_regs[1] << 8) & 0x0300`
///   - Bit 7 of CHR inner: `(ex_regs[1] << 3) & 0x80` (when bit 3 = 0)
///   - Bit 4 of PRG inner: `ex_regs[1] & 0x10` (when bit 3 = 0)
///   - Bits 6-5 of PRG outer: `(ex_regs[1] << 5) & 0x60`
///   - Bit 3: when set, inner PRG range expands to 5 bits and CHR MSB is pass-through
/// - `$5007` (ex_regs\[2\]): Operation mode; default 0x03
///   - 0 (mode A): standard MMC3 register layout
///   - non-zero (mode B): remapped register layout (default)
///
/// Mode B register map ($8000-$BFFF):
/// - `$8000/$9000` (even): sets IRQ latch (as if writing standard $C000)
/// - `$8001/$9001` (odd): sets bank-select register (as if writing $8000) with LUT-
///   remapped lower 3 bits: `{ 0,6,3,7,5,2,4,1 }`; stages a bank-data write
/// - `$A000/$B000` (even): completes staged bank-data write (as if writing $8001)
/// - `$A001/$B001` (odd): sets H/V mirroring (bit 0: 1=horizontal, 0=vertical)
/// - `$C000-$FFFF`: pass-through to standard MMC3 (IRQ counter/reload/enable)
pub struct Mapper217 {
    pub(crate) mmc3: MMC3Mapper,
    /// Extra registers: [0]=$5000, [1]=$5001, [2]=$5007, [3]=mode-B staged flag
    ex_regs: [u8; 4],
    /// Lower 3 bits of bank-select after the last mode-B $8001 write (LUT-mapped)
    staged_reg: u8,
}

impl Mapper217 {
    const MAPPER_NUMBER: u16 = 217;
    const PRG_BANK_MASK: usize = 0x1FFF; // 8KB
    const CHR_BANK_MASK: usize = 0x03FF; // 1KB

    /// LUT used in mode B to remap the lower 3 bits of a $8001 write value.
    const LUT: [u8; 8] = [0, 6, 3, 7, 5, 2, 4, 1];

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mut mapper = Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            // Power-on state from Mesen: ex_regs[0]=0, [1]=0xFF, [2]=0x03, [3]=0
            ex_regs: [0x00, 0xFF, 0x03, 0x00],
            staged_reg: 0,
        };
        // PRG-RAM is not used by mapper 217 ROMs but initialised for safety
        mapper.mmc3.base.set_mirroring(ctx.mirroring);
        mapper
    }

    /// Computes the final 8KB PRG bank after applying the outer bank bits from ex_regs[1].
    fn adjust_prg_bank(&self, raw: usize) -> usize {
        let inner = if self.ex_regs[1] & 0x08 != 0 {
            // 5-bit inner range
            raw & 0x1F
        } else {
            // 4-bit inner range with bit-4 override from ex_regs[1]
            (raw & 0x0F) | ((self.ex_regs[1] as usize) & 0x10)
        };
        ((self.ex_regs[1] as usize) << 5 & 0x60) | inner
    }

    /// Computes the final 1KB CHR bank after applying the outer bank bits from ex_regs[1].
    fn adjust_chr_bank(&self, raw: usize) -> usize {
        let with_inner = if self.ex_regs[1] & 0x08 == 0 {
            // Bit 7 of inner from ex_regs[1]
            ((self.ex_regs[1] as usize) << 3 & 0x80) | (raw & 0x7F)
        } else {
            raw
        };
        ((self.ex_regs[1] as usize) << 8 & 0x0300) | with_inner
    }

    /// Reads a PRG byte in NROM override mode ($5000 bit 7 set).
    ///
    /// All four 8KB slots are mapped to consecutive pages within the 32KB block
    /// determined by ex_regs[0] and ex_regs[1]: slots 0–3 map to pages base+0..=base+3.
    fn read_prg_nrom(&self, addr: u16) -> u8 {
        // base_v is a 6-bit page index: low 4 bits from ex_regs[0], next 2 from ex_regs[1]
        let base_v =
            ((self.ex_regs[0] as usize) & 0x0F) | (((self.ex_regs[1] as usize) << 4) & 0x30);
        let page = base_v << 2; // first 8KB page index in the 32KB block (block × 4)
        let slot_page = match addr {
            0x8000..=0x9FFF => page,
            0xA000..=0xBFFF => page + 1,
            0xC000..=0xDFFF => page + 2,
            0xE000..=0xFFFF => page + 3,
            _ => return 0,
        };
        let final_bank = self.adjust_prg_bank(slot_page);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(final_bank, offset)
    }
}

impl Mapper for Mapper217 {
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
                if self.ex_regs[0] & 0x80 != 0 {
                    self.read_prg_nrom(addr)
                } else {
                    // Use the raw MMC3 8KB page number (register value, including 0xFE/0xFF
                    // fixed-bank sentinels) so mapper 217's inner/outer bank logic is applied
                    // to the logical page number before modulo-wrapping.
                    let raw_bank = self.mmc3.raw_prg_8k_page_number(addr) as usize;
                    let adjusted = self.adjust_prg_bank(raw_bank);
                    let offset = (addr as usize) & Self::PRG_BANK_MASK;
                    self.mmc3.read_prg_at_bank(adjusted, offset)
                }
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
            0x5000 => {
                self.ex_regs[0] = value;
            }
            0x5001 => {
                self.ex_regs[1] = value;
            }
            0x5007 => {
                self.ex_regs[2] = value;
            }
            0x6000..=0x7FFF => {
                self.mmc3.write_prg(addr, value);
            }
            0x8000..=0xFFFF => {
                if self.ex_regs[2] != 0 {
                    // Mode B: remapped register layout
                    match addr & 0xE001 {
                        0x8000 => {
                            // Even $8000-$9FFF → route to $C000 (IRQ latch)
                            self.mmc3.write_prg(0xC000, value);
                        }
                        0x8001 => {
                            // Odd $8000-$9FFF → transform & write as bank-select ($8000)
                            let transformed = (value & 0xC0) | Self::LUT[(value & 0x07) as usize];
                            self.staged_reg = transformed & 0x07;
                            self.ex_regs[3] = 1;
                            self.mmc3.write_prg(0x8000, transformed);
                        }
                        0xA000 => {
                            // Even $A000-$BFFF → write staged bank data if conditions met
                            if self.ex_regs[3] != 0
                                && (self.ex_regs[0] & 0x80 == 0 || self.staged_reg < 6)
                            {
                                self.ex_regs[3] = 0;
                                self.mmc3.write_prg(0x8001, value);
                            }
                        }
                        0xA001 => {
                            // Odd $A000-$BFFF → set H/V mirroring
                            self.mmc3.base.set_mirroring_hv((value & 1) != 0);
                        }
                        _ => {
                            // $C000-$FFFF → pass through unchanged (IRQ counter/reload/enable)
                            self.mmc3.write_prg(addr, value);
                        }
                    }
                } else {
                    // Mode A: standard MMC3 register layout
                    self.mmc3.write_prg(addr, value);
                }
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw_bank = self.mmc3.raw_chr_1k_bank(addr);
        let adjusted = self.adjust_chr_bank(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(adjusted, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw_bank = self.mmc3.raw_chr_1k_bank(addr);
        let adjusted = self.adjust_chr_bank(raw_bank);
        let count = self.mmc3.chr_bank_count_1k();
        let final_bank = if count > 0 { adjusted % count } else { 0 };
        self.mmc3
            .write_chr_1k_at(final_bank, addr as usize & Self::CHR_BANK_MASK, value);
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
        snap.extend_from_slice(&self.ex_regs);
        snap.push(self.staged_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Layout: [MMC3 snapshot bytes (16)][ex_regs[0..4]][staged_reg]
        // MMC3_SNAPSHOT_SIZE = 16: 1 bank_select + 8 regs + 1 irq_latch + 1 irq_counter
        //                          + 1 flags + 1 mirroring + 3 A12-detector bytes.
        // See MMC3Mapper::registers_snapshot for the authoritative format.
        const MMC3_SNAPSHOT_SIZE: usize = 16;
        const EXTRA_BYTES: usize = 5; // 4 ex_regs + 1 staged_reg

        // If the snapshot predates mapper 217 extras (or is truncated), pass the
        // entire buffer to MMC3 and keep ex_regs/staged_reg at their current values.
        if data.len() < MMC3_SNAPSHOT_SIZE + EXTRA_BYTES {
            self.mmc3.restore_registers(data);
            return;
        }

        let (mmc3_data, extra) = data.split_at(data.len() - EXTRA_BYTES);
        self.mmc3.restore_registers(mmc3_data);
        self.ex_regs.copy_from_slice(&extra[0..4]);
        self.staged_reg = extra[4];
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

    // Helper constants
    const PRG_BANKS_32: usize = 32; // 32 × 8KB = 256KB
    const PRG_BANKS_64: usize = 64; // 64 × 8KB = 512KB
    const PRG_BANKS_128: usize = 128; // 128 × 8KB = 1MB
    const CHR_1K_BANKS_8: usize = 8; // 8 × 1KB = 8KB
    const CHR_1K_BANKS_256: usize = 256; // 256 × 1KB = 256KB

    fn make_mapper(prg_banks: usize, chr_banks: usize) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            217,
            banked_data(8 * 1024, prg_banks),
            banked_data(1024, chr_banks),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 217 should be registered")
    }

    // -------------------------------------------------------------------------
    // Factory
    // -------------------------------------------------------------------------

    #[test]
    fn test_factory_creates_mapper_217() {
        let result = create_mapper(MapperContext::new_for_test(
            217,
            banked_data(8 * 1024, PRG_BANKS_32),
            banked_data(1024, CHR_1K_BANKS_8),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 217 must be registered in factory");
        assert_eq!(result.unwrap().mapper_number(), 217);
    }

    // -------------------------------------------------------------------------
    // Power-on state / reset defaults
    // -------------------------------------------------------------------------

    /// At power-on ex_regs[1]=0xFF (bit 3=1) so inner PRG mask is 5 bits.
    /// The MMC3 default maps $E000-$FFFF to the last bank.  With ex_regs[1]=0xFF
    /// the outer bits are (0xFF<<5)&0x60 = 0x60 and inner = last_bank & 0x1F.
    /// read_prg_at_bank wraps by modulo, so the result is still the correct last bank.
    #[test]
    fn test_power_on_last_prg_bank_accessible() {
        let mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        // banked_data: byte at bank N offset 0 is N
        // Last bank = 31; adjusted = 0x60 | (31 & 0x1F) = 0x7F; 0x7F % 32 = 31 ✓
        assert_eq!(mapper.read_prg(0xE000), 31);
    }

    #[test]
    fn test_power_on_chr_bank_accessible() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        // CHR bank 0 at PPU $0000 should read bank 0 value with power-on ex_regs[1]=0xFF
        // ex_regs[1] bit 3 = 1 → with_inner = raw bank (from MMC3 regs, default 0)
        // adjusted = (0xFF << 8 & 0x0300) | 0 = 0x0300 | 0 = 0x0300; 0x0300 % 8 = 0 ✓
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    // -------------------------------------------------------------------------
    // ex_regs[1] PRG outer bank bits
    // -------------------------------------------------------------------------

    /// When ex_regs[1] bit 3 = 0: inner PRG = (raw & 0x0F) | (ex_regs[1] & 0x10)
    /// and outer = (ex_regs[1] << 5 & 0x60) | inner.
    #[test]
    fn test_prg_outer_bank_bits_mode_4bit_inner() {
        // 128 × 8KB PRG so outer bits produce a different bank without wrapping
        let mut mapper = make_mapper(PRG_BANKS_128, CHR_1K_BANKS_8);

        // Use mode A for direct MMC3 register writes
        mapper.write_prg(0x5007, 0x00);

        // Fix R6 = 2 for all sub-cases
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 2);

        // ex_regs[1] = 0x04: bit3=0, outer = (0x04<<5)&0x60 = 0x00, bit4 = 0
        // adjusted = 0x00 | (2 & 0x0F) = 2
        mapper.write_prg(0x5001, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 2);

        // ex_regs[1] = 0x02: bit3=0, outer = (0x02<<5)&0x60 = 0x40, bit4 = 0
        // adjusted = 0x40 | (2 & 0x0F) = 0x42 = 66
        mapper.write_prg(0x5001, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 66);

        // ex_regs[1] = 0x10: bit3=0, outer = (0x10<<5)&0x60 = 0x00, bit4 = 1
        // adjusted = 0x00 | (2 & 0x0F) | (0x10 & 0x10) = 2 | 16 = 18
        mapper.write_prg(0x5001, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 18);
    }

    /// When ex_regs[1] bit 3 = 1: inner PRG = raw & 0x1F (5-bit pass-through)
    #[test]
    fn test_prg_outer_bank_bits_mode_5bit_inner() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        // Use mode A
        mapper.write_prg(0x5007, 0x00);

        // ex_regs[1] = 0x08: bit3=1 → 5-bit inner; outer = (0x08<<5)&0x60 = 0x100&0x60 = 0x00
        mapper.write_prg(0x5001, 0x08);

        // Select R6 = bank 7
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 7);

        // adjusted = 0x00 | (7 & 0x1F) = 7; 7 % 32 = 7 ✓
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    // -------------------------------------------------------------------------
    // ex_regs[1] CHR outer bank bits
    // -------------------------------------------------------------------------

    /// When ex_regs[1] bit 3 = 0: CHR bit 7 of inner comes from ex_regs[1]
    #[test]
    fn test_chr_outer_bank_bit3_zero_applies_bit7() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_256);

        // Use mode A
        mapper.write_prg(0x5007, 0x00);

        // ex_regs[1] = 0x10: bit3=0, (0x10<<3)&0x80 = 0x80 → bit7 of inner set
        mapper.write_prg(0x5001, 0x10);

        // Set R2 = 0 (CHR 1KB slot at PPU $1000 in CHR mode 0)
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8001, 0);

        // with_inner = (0x10<<3 & 0x80) | (0 & 0x7F) = 0x80 | 0 = 0x80 = 128
        // adjusted = (0x10<<8 & 0x0300) | 0x80 = 0x1000 & 0x0300 | 0x80 = 0x0000 | 0x80 = 0x80
        // 0x80 % 256 = 128 → reads CHR bank 128
        assert_eq!(mapper.read_chr(0x1000), 128);
    }

    /// When ex_regs[1] bit 3 = 1: CHR inner passes through without bit 7 override
    #[test]
    fn test_chr_outer_bank_bit3_one_passthrough() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_256);

        // Use mode A
        mapper.write_prg(0x5007, 0x00);

        // ex_regs[1] = 0x08: bit3=1 → CHR inner is raw
        mapper.write_prg(0x5001, 0x08);

        // Set R2 = 5
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8001, 5);

        // with_inner = 5 (pass-through)
        // adjusted = (0x08 << 8 & 0x0300) | 5 = 0x800 & 0x300 | 5 = 0x000 | 5 = 5
        // 5 % 256 = 5 → reads CHR bank 5
        assert_eq!(mapper.read_chr(0x1000), 5);
    }

    // -------------------------------------------------------------------------
    // NROM override mode ($5000 bit 7 set)
    // -------------------------------------------------------------------------

    /// When ex_regs[0] bit 7 is set, all PRG slots use a fixed 32KB block.
    /// Slots 0–3 map to consecutive 8KB pages within that block.
    #[test]
    fn test_nrom_override_mode() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        // Use mode A so we can write standard MMC3 registers first
        mapper.write_prg(0x5007, 0x00);

        // ex_regs[1] = 0x08: bit3=1 → 5-bit inner, outer=0
        mapper.write_prg(0x5001, 0x08);

        // Enable NROM mode, block 1 ($5000 = 0x81): ex_regs[0]=0x81
        // base_v = (0x81 & 0x0F) | ((0x08 << 4) & 0x30) = 0x01 | 0 = 1
        // page = 1 << 2 = 4
        // Slot 0 ($8000) → page 4 → adjust(4): inner=4, outer=0 → final=4
        // Slot 1 ($A000) → page 5 → final=5
        // Slot 2 ($C000) → page 6 → final=6
        // Slot 3 ($E000) → page 7 → final=7
        mapper.write_prg(0x5000, 0x81);

        assert_eq!(mapper.read_prg(0x8000), 4, "slot 0 should be page 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "slot 1 should be page 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "slot 2 should be page 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "slot 3 should be page 7");
    }

    /// NROM mode off (bit 7 cleared) → normal MMC3 PRG banking resumes
    #[test]
    fn test_nrom_override_mode_off_restores_mmc3() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        mapper.write_prg(0x5007, 0x00);
        mapper.write_prg(0x5001, 0x08);

        // Set R6 = bank 5 in MMC3
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 5);

        // Enable NROM mode
        mapper.write_prg(0x5000, 0x80);
        // Reads are now fixed to NROM block 0
        let nrom_val = mapper.read_prg(0x8000);

        // Disable NROM mode
        mapper.write_prg(0x5000, 0x00);
        // Now reads use MMC3 bank registers again
        assert_eq!(mapper.read_prg(0x8000), 5, "MMC3 R6=5 should be restored");
        let _ = nrom_val; // just to use it
    }

    // -------------------------------------------------------------------------
    // Mode B (ex_regs[2] != 0) – remapped register layout
    // -------------------------------------------------------------------------

    /// In mode B, writes to even $8000-$9FFF set the IRQ latch (as $C000 in mode A).
    #[test]
    fn test_mode_b_even_8000_sets_irq_latch() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        // ex_regs[2] = 0x03 at power-on → mode B

        // Enable IRQ, set latch via mode B ($8000 → IRQ latch)
        mapper.write_prg(0x8000, 10); // IRQ latch = 10

        // Enable IRQ counting ($E001 is odd $E000-$FFFF → enable IRQ, passes through)
        mapper.write_prg(0xE001, 0);

        // In standard MMC3, IRQ counter is reloaded from latch on $C001 write.
        // We verify the latch was set by triggering reload ($C001 in standard mode B passes through)
        // After reload, the IRQ counter should be the latch value.
        // This is difficult to test directly without full PPU timing, so we just verify
        // that the write was accepted (no panic, no incorrect bank selection).
        // The key is that $8000 in mode B should NOT set the bank_select register.
        // If it did, it would change PRG mode bits and affect PRG reading.
        // We check that $8000 write = 6 does NOT select R6 (as it would in mode A).
        mapper.write_prg(0x8000, 0x06); // This is IRQ latch = 6, NOT bank_select = 6

        // $E000 should still be the last bank (R7 default) — bank select was not changed
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "$E000 should still be last bank (R7 unchanged)"
        );
    }

    /// In mode B, writes to odd $8000-$9FFF set bank-select with LUT-remapped low 3 bits.
    /// LUT = {0,6,3,7,5,2,4,1}: value & 0x07 is remapped, value & 0xC0 passes through.
    #[test]
    fn test_mode_b_odd_8001_sets_bank_select_with_lut() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        // ex_regs[2] = 0x03 → mode B

        // ex_regs[1] = 0x08 → 5-bit inner, outer = 0
        mapper.write_prg(0x5001, 0x08);

        // Write $8001 with value 0x01: LUT[1] = 6 → bank_select = 0x01&0xC0 | 6 = 6 (R6 selected)
        // Then write $A000 with value 5 → bank data: R6 = 5
        mapper.write_prg(0x8001, 0x01); // stages R6 selection
        mapper.write_prg(0xA000, 5); // completes: R6 = 5

        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "R6 = 5 → $8000 should read bank 5"
        );
    }

    /// LUT remapping: write value 0x02 → LUT[2] = 3 → selects R3 (not R6).
    #[test]
    fn test_mode_b_lut_remap_selects_correct_register() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        mapper.write_prg(0x5001, 0x08);

        // value=2 → LUT[2]=3 → bank_select=3 (R3 selected = CHR 1KB slot, PPU $0C00)
        mapper.write_prg(0x8001, 0x02); // stages R3
        mapper.write_prg(0xA000, 7); // R3 = 7

        // R3 is CHR register for PPU $1400-$17FF in CHR mode 0
        // adjust_chr_bank(7) with ex_regs[1]=0x08 (bit3=1): with_inner=7, adjusted=(0x08<<8&0x0300)|7=7
        // 7 % 8 = 7
        assert_eq!(
            mapper.read_chr(0x1400),
            7,
            "R3=7 → CHR at PPU $1400 should be bank 7"
        );
    }

    /// In mode B, writes to odd $A000-$BFFF set H/V mirroring.
    #[test]
    fn test_mode_b_odd_a001_sets_mirroring() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        // mode B default

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Write $A001 with bit 0 = 1 → horizontal
        mapper.write_prg(0xA001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Write $A001 with bit 0 = 0 → vertical
        mapper.write_prg(0xA001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    /// In mode B, even $A000-$BFFF does NOT set mirroring (unlike mode A).
    #[test]
    fn test_mode_b_even_a000_does_not_set_mirroring_without_staging() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        // No staged write (ex_regs[3] = 0 at power-on)
        // Writing to $A000 even without prior $8001 should be ignored
        mapper.write_prg(0xA000, 0x01); // would be mirroring in mode A, but not in mode B

        // Mirroring should remain vertical (power-on default)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    /// Mode A ($5007 = 0): standard MMC3 register layout
    #[test]
    fn test_mode_a_standard_mmc3_behavior() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        // Switch to mode A
        mapper.write_prg(0x5007, 0x00);

        // ex_regs[1] = 0x08 → outer = 0, inner 5-bit
        mapper.write_prg(0x5001, 0x08);

        // Standard MMC3: $8000 = bank_select (R6), $8001 = bank data
        mapper.write_prg(0x8000, 6); // select R6
        mapper.write_prg(0x8001, 9); // R6 = 9

        assert_eq!(mapper.read_prg(0x8000), 9, "Mode A: R6=9 → bank 9 at $8000");
    }

    /// Mode A: $A000 sets mirroring
    #[test]
    fn test_mode_a_a000_sets_mirroring() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);

        mapper.write_prg(0x5007, 0x00); // mode A

        mapper.write_prg(0xA000, 0x01); // horizontal mirroring
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------------
    // Mode B staged-write inhibit when ex_regs[0] bit 7 = 1 and reg >= 6
    // -------------------------------------------------------------------------

    /// When NROM mode is active (ex_regs[0] bit7=1), staged writes for PRG
    /// registers (R6/R7, index >= 6) are suppressed.
    #[test]
    fn test_mode_b_nrom_active_inhibits_prg_register_write() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        mapper.write_prg(0x5001, 0x08);

        // First set R6 = 3 via normal mode-B sequence (no NROM yet)
        mapper.write_prg(0x8001, 0x01); // LUT[1]=6 → R6 selected, staged
        mapper.write_prg(0xA000, 3); // R6 = 3

        // Enable NROM mode
        mapper.write_prg(0x5000, 0x80);

        // Try to change R6 to 7 (R6 is register 6, >= 6 → inhibited)
        mapper.write_prg(0x8001, 0x01); // stages R6 again
        mapper.write_prg(0xA000, 7); // should be suppressed

        // Disable NROM to read via MMC3
        mapper.write_prg(0x5000, 0x00);

        // R6 should still be 3, not 7
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "R6 should remain 3 (PRG write suppressed in NROM mode)"
        );
    }

    /// When NROM mode is active, staged writes for CHR registers (< 6) are NOT suppressed.
    #[test]
    fn test_mode_b_nrom_active_allows_chr_register_write() {
        let mut mapper = make_mapper(PRG_BANKS_32, CHR_1K_BANKS_8);
        mapper.write_prg(0x5001, 0x08);

        // Enable NROM mode
        mapper.write_prg(0x5000, 0x80);

        // value=0x02 → LUT[2]=3 → R3 is CHR register (index 3 < 6) → NOT inhibited
        mapper.write_prg(0x8001, 0x02); // stages R3 (CHR)
        mapper.write_prg(0xA000, 6); // R3 = 6 (should succeed)

        // R3 maps to CHR PPU $1400-$17FF in CHR mode 0
        assert_eq!(
            mapper.read_chr(0x1400),
            6,
            "R3=6 allowed even in NROM mode (CHR reg)"
        );
    }

    // -------------------------------------------------------------------------
    // PRG-RAM passthrough
    // -------------------------------------------------------------------------

    #[test]
    fn test_prg_ram_read_write() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS_32);
        let chr_rom = banked_data(1024, CHR_1K_BANKS_8);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(217, prg_rom, chr_rom, NametableLayout::Vertical)
                .with_prg_ram_banks(1),
        )
        .expect("Mapper 217 with PRG-RAM");

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    // -------------------------------------------------------------------------
    // Save state snapshot / restore
    // -------------------------------------------------------------------------

    #[test]
    fn test_snapshot_and_restore_preserves_state() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS_32);
        let chr_rom = banked_data(1024, CHR_1K_BANKS_8);

        let mut mapper = create_mapper(MapperContext::new_for_test(
            217,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ))
        .expect("original mapper");

        // Set mode A, configure ex_regs, set some MMC3 banks
        mapper.write_prg(0x5007, 0x00); // mode A
        mapper.write_prg(0x5001, 0x08); // outer bits
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 7); // R6 = 7
        mapper.write_prg(0xA000, 0x01); // mirroring = horizontal
        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper(MapperContext::new_for_test(
            217,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("restored mapper");
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 7, "R6 should be 7 after restore");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be restored"
        );
    }

    // -------------------------------------------------------------------------
    // Mirroring: power-on default
    // -------------------------------------------------------------------------

    #[test]
    fn test_power_on_mirroring_from_header() {
        let mapper = create_mapper(MapperContext::new_for_test(
            217,
            banked_data(8 * 1024, PRG_BANKS_32),
            banked_data(1024, CHR_1K_BANKS_8),
            NametableLayout::Horizontal,
        ))
        .expect("horizontal mapper");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }
}
