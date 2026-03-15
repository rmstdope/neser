//! Mapper 315 – BMC-830134C (MMC3 multicart variant)
//!
//! Specifications:
//! - Primary source: NesDev wiki (https://www.nesdev.org/wiki/NES_2.0_Mapper_315)
//! - Fallback source: libretro-fceumm mapper315.c
//!   <https://github.com/negativeExponent/libretro-fceumm_next/blob/master/src/mappers/mapper315.c>
//!
//! # Hardware overview
//!
//! BMC-830134C is an MMC3-based multicart PCB used in 4-in-1 collections such as
//! _4-in-1 Street Blaster 5_.  It adds a single outer bank register that extends
//! both PRG and CHR bank selection beyond the standard MMC3 range.
//!
//! # Outer Bank Register ($6800–$68FF, write)
//!
//! The register value is taken from the **low byte of the write address**:
//! writing to $6800 stores 0x00, $6801 → 0x01, … $68FF → 0xFF.
//!
//! ```text
//! D~7654 3210
//!   ---------
//!   .... CPpC
//!        |||+- CHR A18 (bit 0): extends CHR 1 KB bank to bit 8
//!        |++-- PRG A17–A18 (bits 1–2): replace MMC3's upper PRG bank bits
//!        | +-- CHR A17 (bit 1): OR'd into CHR bank bit 7
//!        +---- CHR A16 / GNROM mode (bit 3): OR'd into CHR bank bit 6;
//!              also activates GNROM-like PRG layout when set
//! ```
//!
//! # PRG banking
//!
//! `outer = (outer_reg << 3) & 0xF0`  (bits 1–3 of reg → bits 4–6 of 8 KB bank)
//!
//! **Normal mode** (`outer_reg & 0x08 == 0`):
//! - `bank = outer | (mmc3_inner & 0x0F)`
//!
//! **GNROM-like mode** (`outer_reg & 0x08 != 0`):
//! - $8000–$9FFF: `bank = outer | (mmc3_reg6 & 0x0D)`  (bit 1 cleared)
//! - $A000–$BFFF: `bank = outer | (mmc3_reg7 & 0x0D)`  (bit 1 cleared)
//! - $C000–$DFFF: `bank = outer | (mmc3_reg6 & 0x0D | 0x02)`  (bit 1 forced set)
//! - $E000–$FFFF: `bank = outer | (mmc3_reg7 & 0x0D | 0x02)`  (bit 1 forced set)
//!
//! # CHR banking
//!
//! ```text
//! physical_bank[8]   = outer_reg[0]
//! physical_bank[7]   = outer_reg[1] | mmc3_inner[7]
//! physical_bank[6]   = outer_reg[3] | mmc3_inner[6]
//! physical_bank[5:0] = mmc3_inner[5:0]
//! ```
//!
//! # Mirroring / IRQ
//!
//! Fully standard MMC3 (controlled via $A000 and $C000–$E001).
//!
//! # Known limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 315;

/// Mapper 315 – BMC-830134C
pub struct Mapper315 {
    mmc3: MMC3Mapper,
    /// Outer bank register; value = low byte of write address ($6800 → 0x00)
    outer_reg: u8,
}

impl Mapper315 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            outer_reg: 0,
        }
    }

    fn gnrom_mode(&self) -> bool {
        self.outer_reg & 0x08 != 0
    }

    /// Outer PRG bank bits: reg bits 1–3 → bank bits 4–6
    fn outer_prg_bits(&self) -> usize {
        ((self.outer_reg as usize) << 3) & 0xF0
    }

    fn mapped_prg_bank_for_addr(&self, addr: u16) -> usize {
        let outer = self.outer_prg_bits();
        if self.gnrom_mode() {
            let raw_low = self.mmc3.raw_prg_8k_page_number(0x8000) as usize;
            let raw_high = self.mmc3.raw_prg_8k_page_number(0xA000) as usize;
            match addr {
                0x8000..=0x9FFF => outer | (raw_low & 0x0D),
                0xA000..=0xBFFF => outer | (raw_high & 0x0D),
                0xC000..=0xDFFF => outer | ((raw_low & 0x0D) | 0x02),
                0xE000..=0xFFFF => outer | ((raw_high & 0x0D) | 0x02),
                _ => 0,
            }
        } else {
            let raw = self.mmc3.raw_prg_8k_page_number(addr) as usize;
            outer | (raw & 0x0F)
        }
    }

    fn mapped_chr_bank_and_offset(&self, addr: u16) -> (usize, usize) {
        let raw = self.mmc3.raw_chr_1k_bank(addr);
        // bit 8: outer_reg[0]
        // bit 7: outer_reg[1] OR raw[7]
        // bit 6: outer_reg[3] OR raw[6]
        // bits 0–5: raw[5:0]
        let bit8 = ((self.outer_reg as usize) & 0x01) << 8;
        let bit7 = (((self.outer_reg as usize) << 6) & 0x80) | (raw & 0x80);
        let bit6 = (((self.outer_reg as usize) << 3) & 0x40) | (raw & 0x40);
        let low = raw & 0x3F;
        let bank = bit8 | bit7 | bit6 | low;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        (bank, offset)
    }
}

impl Mapper for Mapper315 {
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
        if (0x6000..=0x7FFF).contains(&addr) {
            // Delegate WRAM window directly to MMC3 so PRG-RAM and $A001 work correctly.
            return self.mmc3.read_prg(addr);
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        // Use Mapper 315's custom outer/inner banking for PRG-ROM above $8000.
        let bank = self.mapped_prg_bank_for_addr(addr);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6800..=0x68FF).contains(&addr) {
            // Writes in this subrange select the outer bank based on the low address byte.
            self.outer_reg = (addr & 0xFF) as u8;
            // Also forward to MMC3 so any WRAM/control behavior remains visible.
            self.mmc3.write_prg(addr, value);
        } else if (0x6000..=0x7FFF).contains(&addr) {
            // Forward all other WRAM-window writes to MMC3.
            self.mmc3.write_prg(addr, value);
        } else if (0x8000..=0xFFFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let (bank, offset) = self.mapped_chr_bank_and_offset(addr);
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let (bank, offset) = self.mapped_chr_bank_and_offset(addr);
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer, mmc3_data)) = data.split_last() {
            self.outer_reg = outer;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn reset(&mut self) {
        self.outer_reg = 0;
        self.mmc3.reset();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    // Use non-power-of-two counts to prevent modulo-wrap false-passes.
    const PRG_BANKS_8K: usize = 96;
    const CHR_BANKS_1K: usize = 384;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            315,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 315 should be implemented")
    }

    #[test]
    fn mapper_315_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            315,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 315 must be registered in the mapper factory"
        );
    }

    #[test]
    fn outer_reg_prg_bits_1_and_2_extend_prg_bank_selection() {
        // PRG banks: 96 × 8KB, each bank N filled with byte N.
        // Set MMC3 reg6 to 1 (bank at $8000 = 1).
        // With outer_reg = 0 (write to $6800): physical bank = 0 | 1 = 1, marker = 1.
        // With outer_reg = 2 (write to $6802): outer = (2 << 3) & 0xF0 = 0x10,
        //   physical bank = 0x10 | 1 = 17, marker = 17.
        let mut mapper = make_mapper();

        // Select reg6 = 1
        mapper.write_prg(0x8000, 0x06); // bank register select → reg 6
        mapper.write_prg(0x8001, 0x01); // reg6 = 1

        // outer_reg = 0 → bank 1
        mapper.write_prg(0x6800, 0x00); // addr & 0xFF = 0x00 → outer_reg = 0
        let prg_no_outer = mapper.read_prg(0x8000);
        assert_eq!(prg_no_outer, 1, "Without outer bits, bank should be 1");

        // outer_reg = 2 (write to addr $6802) → outer = 0x10, bank = 17
        mapper.write_prg(0x6802, 0x00); // addr & 0xFF = 0x02 → outer_reg = 2
        let prg_with_outer = mapper.read_prg(0x8000);
        assert_eq!(
            prg_with_outer, 17,
            "With outer_reg=2, bank should be 0x10|1=17"
        );
    }

    #[test]
    fn outer_reg_bit_0_extends_chr_bank_to_bit_8() {
        // CHR banks: 384 × 1KB, using banked_data_with_upper_marker for banks > 255.
        // Bank 0 → marker 0; bank 256 → marker 1.
        // Set MMC3 chr reg0 = 0 (bank at CHR $0000 = 0).
        // With outer_reg = 0: physical CHR bank = 0, marker = 0.
        // With outer_reg = 1 (write to $6801): bit 8 added, bank = 256, marker = 1.
        let mut mapper = create_mapper(MapperContext::new_for_test(
            315,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data_with_upper_marker(1024, 384),
            NametableLayout::Vertical,
        ))
        .unwrap();

        // MMC3 chr reg 0 = 0 (already default, but be explicit)
        mapper.write_prg(0x8000, 0x80); // select chr_mode bank 0 → reg 0
        mapper.write_prg(0x8001, 0x00); // chr reg0 = 0

        // outer_reg = 0 → bank 0, marker 0
        mapper.write_prg(0x6800, 0x00); // outer_reg = 0
        let chr_no_outer = mapper.read_chr(0x0000);
        assert_eq!(
            chr_no_outer, 0,
            "Without outer bits, CHR bank 0 has marker 0"
        );

        // outer_reg = 1 (write to $6801) → bank 256, marker 1
        mapper.write_prg(0x6801, 0x00); // addr & 0xFF = 1 → outer_reg = 1
        let chr_with_outer = mapper.read_chr(0x0000);
        assert_eq!(
            chr_with_outer, 1,
            "With outer_reg=1 (bit 0), CHR bank should extend to bit 8 (bank 256), marker=1"
        );
    }

    #[test]
    fn outer_reg_bit_1_ors_chr_bank_bit_7() {
        // CHR banks: 384 × 1KB.
        // Set MMC3 to map CHR $0000 to bank 5 (via chr reg2 in CHR mode 0).
        // Wait - in CHR mode 0, $0000 uses reg0. Let us use $1000 which uses reg2.
        // Set MMC3 chr reg 2 = 5 → CHR $1000 bank = 5.
        // With outer_reg = 0: bank = 5.
        // With outer_reg = 2 (write to $6802): bit 7 added via bit 1, bank = 128 | 5 = 133.
        let mut mapper = make_mapper();

        // Select MMC3 chr reg 2 = 5 (reg 2 controls CHR $1000-$13FF in mode 0)
        mapper.write_prg(0x8000, 0x02); // bank register select → reg 2
        mapper.write_prg(0x8001, 0x05); // chr reg2 = 5

        // outer_reg = 0 → bank 5
        mapper.write_prg(0x6800, 0x00); // outer_reg = 0
        let chr_no_outer = mapper.read_chr(0x1000);
        assert_eq!(
            chr_no_outer, 5,
            "Without outer bits, CHR $1000 bank should be 5"
        );

        // outer_reg = 2 (write to $6802) → bit 1 set → bit 7 OR'd, bank = 128|5 = 133
        mapper.write_prg(0x6802, 0x00); // addr & 0xFF = 2 → outer_reg = 2
        let chr_with_outer = mapper.read_chr(0x1000);
        assert_eq!(
            chr_with_outer, 133,
            "With outer_reg=2 (bit 1 set), CHR bank should have bit 7 OR'd: 128|5=133"
        );
    }

    #[test]
    fn outer_reg_bit_3_ors_chr_bank_bit_6() {
        // Set MMC3 chr reg 2 = 5 → CHR $1000 bank = 5.
        // With outer_reg = 8 (write to $6808): bit 3 set → bit 6 OR'd, bank = 64|5 = 69.
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x02); // select chr reg 2
        mapper.write_prg(0x8001, 0x05); // chr reg2 = 5

        mapper.write_prg(0x6800, 0x00); // outer_reg = 0
        let chr_no_outer = mapper.read_chr(0x1000);
        assert_eq!(
            chr_no_outer, 5,
            "Without outer bits, CHR $1000 bank should be 5"
        );

        // outer_reg = 8 (write to $6808) → bit 3 set → bit 6 OR'd
        mapper.write_prg(0x6808, 0x00); // addr & 0xFF = 8 → outer_reg = 8
        let chr_with_outer = mapper.read_chr(0x1000);
        assert_eq!(
            chr_with_outer, 69,
            "With outer_reg=8 (bit 3 set), CHR bank should have bit 6 OR'd: 64|5=69"
        );
    }

    #[test]
    fn gnrom_mode_c000_mirrors_8000_bank_with_bit_1_set() {
        // In GNROM mode (outer_reg bit 3), $C000-$DFFF mirrors $8000-$9FFF bank
        // but with bit 1 of the inner bank forced to 1.
        // Set MMC3 reg6 = 5 (bit 1 = 0 → no change from clearing), reg7 = 5.
        // With outer_reg = 8 (GNROM mode): outer = (8 << 3) & 0xF0 = 0x40.
        // $8000 bank = 0x40 | (5 & 0x0D) = 0x40 | 5 = 69, marker = 69.
        // $C000 bank = 0x40 | ((5 & 0x0D) | 0x02) = 0x40 | 7 = 71, marker = 71.
        let mut mapper = make_mapper();

        // Set reg6 = 5
        mapper.write_prg(0x8000, 0x06); // select reg 6
        mapper.write_prg(0x8001, 0x05); // reg6 = 5

        // Enable GNROM mode
        mapper.write_prg(0x6808, 0x00); // addr & 0xFF = 8 → outer_reg = 8

        let prg_8000 = mapper.read_prg(0x8000);
        let prg_c000 = mapper.read_prg(0xC000);

        assert_eq!(prg_8000, 69, "$8000 should use bank 0x40|(5&0x0D)=69");
        assert_eq!(
            prg_c000, 71,
            "$C000 should use bank 0x40|((5&0x0D)|0x02)=71 in GNROM mode"
        );
        assert_ne!(
            prg_8000, prg_c000,
            "$8000 and $C000 should map to different banks in GNROM mode"
        );
    }

    #[test]
    fn gnrom_mode_e000_mirrors_a000_bank_with_bit_1_set() {
        // Set MMC3 reg7 = 9 (bit 1 = 0).
        // With outer_reg = 8 (GNROM): outer = 0x40.
        // $A000 bank = 0x40 | (9 & 0x0D) = 0x40 | 9 = 73, marker = 73.
        // $E000 bank = 0x40 | ((9 & 0x0D) | 0x02) = 0x40 | 11 = 75, marker = 75.
        let mut mapper = make_mapper();

        // Set reg7 = 9
        mapper.write_prg(0x8000, 0x07); // select reg 7
        mapper.write_prg(0x8001, 0x09); // reg7 = 9

        mapper.write_prg(0x6808, 0x00); // outer_reg = 8, GNROM mode

        let prg_a000 = mapper.read_prg(0xA000);
        let prg_e000 = mapper.read_prg(0xE000);

        assert_eq!(prg_a000, 73, "$A000 should use bank 0x40|(9&0x0D)=73");
        assert_eq!(
            prg_e000, 75,
            "$E000 should use bank 0x40|((9&0x0D)|0x02)=75 in GNROM mode"
        );
    }

    #[test]
    fn mirroring_and_irq_follow_mmc3_conventions() {
        let mut mapper = make_mapper();

        // Default mirroring should be vertical (from MapperContext)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // $A000 bit 0 controls mirroring
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        let caps = mapper.capabilities();
        assert!(caps.has_irq, "Mapper 315 must have IRQ capability");
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert!(!caps.has_expansion_audio);
    }

    #[test]
    fn outer_reg_value_comes_from_address_low_byte_not_data() {
        // Writing to $6806 with value 0xFF should set outer_reg to 6, not 0xFF.
        // outer = (6 << 3) & 0xF0 = 0x30.
        // Set reg6 = 1, expect bank = 0x30 | 1 = 49, marker = 49.
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x01); // reg6 = 1

        mapper.write_prg(0x6806, 0xFF); // addr & 0xFF = 6 → outer_reg = 6
        let prg = mapper.read_prg(0x8000);
        assert_eq!(
            prg, 49,
            "outer_reg=6 → outer=0x30, bank=0x30|1=49; register value must be addr&0xFF not data"
        );
    }

    #[test]
    fn reset_clears_outer_register() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x01); // reg6 = 1

        mapper.write_prg(0x6802, 0x00); // outer_reg = 2 → outer = 0x10, bank = 17
        let before_reset = mapper.read_prg(0x8000);
        assert_eq!(before_reset, 17, "Before reset, bank should be 17");

        mapper.reset();
        // After reset, outer_reg = 0 → bank = 0|1 = 1... but MMC3 regs also reset!
        // After MMC3 reset, reg6 = 0, so bank = 0.
        let after_reset = mapper.read_prg(0x8000);
        assert_ne!(
            after_reset, 17,
            "After reset, outer_reg should be cleared so bank changes"
        );
    }
}
