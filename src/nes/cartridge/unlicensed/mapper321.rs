//! Mapper 321 – BMC-820310-C (MMC3 multicart)
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_321>
//!
//! ## Hardware overview
//!
//! A BMC-820310-C multicart board extending MMC3 with an outer-bank register.
//! Similar to Mapper 287, but the outer register is encoded in the **address**
//! (not the data byte) of writes to `$6000-$7FFF`.
//!
//! ## Outer register (`$6000-$7FFF` writes)
//!
//! Any write to `$6000-$7FFF` latches the address low byte into `ex_reg`:
//!
//! ```text
//! Address bits   Field
//! [5:4]          PP — PRG A17:A16 / CHR A18:A17 outer bank bits
//! [3]            M  — mode: 0=normal MMC3, 1=NROM-256
//! ```
//!
//! (Data byte and WRAM write enable are still handled via `$A001` as per MMC3.)
//!
//! ## Banking
//!
//! **Normal mode (M=0):**
//! - PRG 8 KiB bank = `(PP << 4) | (mmc3_8k_page & 0x0F)`
//! - CHR 1 KiB bank = `(PP << 7) | (mmc3_1k_page & 0x7F)`
//!
//! **NROM-256 mode (M=1):**
//! - All four PRG 8 KiB slots fixed: bank = `(PP << 2) | slot` (slot = CPU A14:A13)
//! - CHR banking unchanged (still uses normal formula above)
//!
//! ## IRQ
//!
//! Standard MMC3 scanline counter (A12 rising-edge).
//!
//! ## Mirroring
//!
//! Software-controlled via MMC3 `$A000`.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;
const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
const CHR_BANK_MASK: usize = CHR_BANK_SIZE - 1;

/// Mapper 321 – BMC-820310-C multicart.
pub struct Mapper321 {
    mmc3: MMC3Mapper,
    /// Address low byte of the most recent `$6000-$7FFF` write.
    /// Bits [5:4] = PP, bit [3] = M.
    ex_reg: u8,
}

impl Mapper321 {
    const MAPPER_NUMBER: u16 = 321;

    pub fn new(ctx: MapperContext) -> Self {
        let mmc3 = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            1, // 8 KiB PRG-RAM for $A001 register
        );
        Self { mmc3, ex_reg: 0 }
    }

    fn pp(&self) -> usize {
        ((self.ex_reg >> 4) & 0x03) as usize
    }

    fn nrom_mode(&self) -> bool {
        (self.ex_reg & 0x08) != 0
    }

    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let pp = self.pp();
        if self.nrom_mode() {
            // CPU A14:A13 = slot index (0-3).
            let slot = (addr as usize - 0x8000) >> 13;
            (pp << 2) | (slot & 0x03)
        } else {
            let mmc3_page = self.mmc3.raw_prg_8k_page_number(addr) as usize;
            (pp << 4) | (mmc3_page & 0x0F)
        }
    }

    fn chr_bank_for_addr(&mut self, addr: u16) -> usize {
        let pp = self.pp();
        let inner = self.mmc3.raw_chr_1k_bank(addr);
        (pp << 7) | (inner & 0x7F)
    }
}

impl Mapper for Mapper321 {
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
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 8,
            ..Default::default()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for_addr(addr);
                let count = self.mmc3.base.prg_rom().len() / PRG_BANK_SIZE;
                let wrapped = if count > 0 { bank % count } else { 0 };
                let offset = (addr as usize) & PRG_BANK_MASK;
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
            // Outer register: address low byte is the data; data byte is WRAM.
            0x6000..=0x7FFF => {
                self.ex_reg = (addr & 0xFF) as u8;
                // Pass through to MMC3 so that WRAM enable ($A001) still works.
                self.mmc3.write_prg(addr, value);
            }
            0x8000..=0xFFFF => self.mmc3.write_prg(addr, value),
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.chr_bank_for_addr(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(wrapped, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.chr_bank_for_addr(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(wrapped, offset, value);
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

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.ex_reg = data[data.len() - 1];
            self.mmc3.restore_registers(&data[..data.len() - 1]);
        } else {
            self.mmc3.restore_registers(data);
            self.ex_reg = 0;
        }
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
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts.
    const PRG_BANKS: usize = 48; // 48 × 8 KiB = 384 KiB (non-power-of-two)
    const CHR_BANKS: usize = 96; // 96 × 1 KiB = 96 KiB (non-power-of-two)

    fn make_mapper() -> Mapper321 {
        Mapper321::new(MapperContext::new_for_test(
            Mapper321::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_321_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            Mapper321::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 321 must be registered in the factory"
        );
    }

    // ── Outer register ───────────────────────────────────────────────

    #[test]
    fn outer_reg_uses_address_low_byte_not_data() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6030, 0xFF);
        assert_eq!(
            mapper.ex_reg, 0x30,
            "ex_reg must latch address low byte, not data byte"
        );
    }

    #[test]
    fn outer_reg_responds_across_6000_to_7fff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7F10, 0x00);
        assert_eq!(mapper.ex_reg, 0x10);
    }

    // ── PRG banking – normal mode (M=0) ─────────────────────────────

    #[test]
    fn normal_mode_pp0_mmc3_bank0_reads_prg_bank0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00); // PP=0, M=0
        mapper.write_prg(0x8000, 0x06); // MMC3: select R6
        mapper.write_prg(0x8001, 0); // R6 = 0 → $8000 = mmc3 bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Normal mode PP=0 inner=0 → PRG bank 0"
        );
    }

    #[test]
    fn normal_mode_pp1_shifts_prg_bank_by_16() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6010, 0x00); // PP=1 (addr bits 5:4 = 01), M=0
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner=0
        // PRG bank = (1<<4)|(0&0x0F) = 16
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "PP=1 normal mode inner=0 → PRG bank 16"
        );
    }

    #[test]
    fn normal_mode_pp2_shifts_prg_bank_by_32() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6020, 0x00); // PP=2, M=0
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "PP=2 normal mode inner=0 → PRG bank 32"
        );
    }

    #[test]
    fn normal_mode_inner_bank_masked_to_4_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6010, 0x00); // PP=1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5); // inner=5
        // PRG bank = (1<<4)|(5&0x0F) = 16|5 = 21
        assert_eq!(mapper.read_prg(0x8000), 21, "PP=1 inner=5 → PRG bank 21");
    }

    // ── PRG banking – NROM mode (M=1) ───────────────────────────────

    #[test]
    fn nrom_mode_pp0_maps_four_consecutive_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6008, 0x00); // PP=0, M=1 (bit3=1 → addr 0x08)
        // PP=0: banks (0<<2)|0..3 = banks 0-3
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 → bank 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 → bank 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 → bank 3");
    }

    #[test]
    fn nrom_mode_pp1_maps_banks_4_to_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6018, 0x00); // PP=1 (addr bits 5:4 = 01), M=1
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 → bank 4");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 → bank 7");
    }

    #[test]
    fn nrom_mode_pp3_maps_banks_12_to_15() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6038, 0x00); // PP=3 (addr bits 5:4 = 11), M=1
        assert_eq!(mapper.read_prg(0x8000), 12, "$8000 → bank 12");
        assert_eq!(mapper.read_prg(0xE000), 15, "$E000 → bank 15");
    }

    // ── CHR banking ─────────────────────────────────────────────────

    #[test]
    fn chr_pp0_uses_mmc3_bank_directly() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00); // PP=0
        mapper.write_prg(0x8000, 0x00); // bank select R0
        mapper.write_prg(0x8001, 10); // R0 = 10 (even-aligned → 1KiB bank 10 at $0000)
        let val = mapper.read_chr(0x0000);
        assert_eq!(
            val,
            10 % CHR_BANKS as u8,
            "PP=0 R0=10 → CHR bank 10 at $0000"
        );
    }

    #[test]
    fn chr_pp1_shifts_chr_bank_by_128() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6010, 0x00); // PP=1
        mapper.write_prg(0x8000, 0x00); // R0
        mapper.write_prg(0x8001, 0); // R0 = 0 → raw inner = 0 → effective = (1<<7)|0 = 128
        // 128 % 96 = 32 (mod CHR_BANKS)
        let val = mapper.read_chr(0x0000);
        assert_eq!(
            val,
            128 % CHR_BANKS as u8,
            "PP=1 inner=0 → CHR bank 128 mod 96"
        );
    }

    // ── Reset ───────────────────────────────────────────────────────

    #[test]
    fn reset_clears_outer_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6020, 0x00); // PP=2
        mapper.reset();
        assert_eq!(mapper.ex_reg, 0, "ex_reg must be 0 after reset");
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "After reset, PRG bank at $8000 must be 0"
        );
    }
}
