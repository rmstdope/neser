//! Mapper 197 – MMC3 clone with custom CHR granularity
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_197>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_197.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 197 is an MMC3 variant where only the CHR bank mapping logic differs.
//! PRG mapping, mirroring, and IRQ are identical to standard MMC3.
//!
//! ## CHR Mapping
//!
//! The eight 1 KiB CHR slots are filled differently depending on the CHR mode
//! bit (bit 7 of the bank-select register `$8000`):
//!
//! ### Mode 0 (bit 7 = 0)
//!
//! ```text
//! PPU $0000–$0FFF  (4 KiB): reg[0] × 2,  reg[0]×2+1, reg[0]×2+2, reg[0]×2+3
//! PPU $1000–$17FF  (2 KiB): reg[2] × 2,  reg[2]×2+1
//! PPU $1800–$1FFF  (2 KiB): reg[3] × 2,  reg[3]×2+1
//! ```
//!
//! ### Mode 1 (bit 7 = 1)
//!
//! ```text
//! PPU $0000–$0FFF  (4 KiB): reg[2] × 2,  reg[2]×2+1, reg[2]×2+2, reg[2]×2+3
//! PPU $1000–$17FF  (2 KiB): reg[0] × 2,  reg[0]×2+1
//! PPU $1800–$1FFF  (2 KiB): reg[0] × 2,  reg[0]×2+1
//! ```
//!
//! In standard MMC3, `reg[0]` selects a **2 KiB** CHR block; here it selects a
//! **4 KiB** block (four consecutive 1 KiB banks at `reg[0] × 2`).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 197;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;
const PRG_BANK_SIZE: usize = 0x2000;
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;

/// Mapper 197 – MMC3 clone with 4 KiB / 2 KiB CHR granularity.
///
/// See the module-level documentation for hardware details.
pub struct Mapper197 {
    inner: MMC3Mapper,
}

impl Mapper197 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            inner: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
        }
    }

    /// Compute the 1 KiB CHR bank number for the given PPU address using
    /// mapper-197's 4KB/2KB/2KB CHR granularity.
    fn mapped_chr_bank(&self, addr: u16) -> usize {
        let chr_mode = (self.inner.bank_select_reg() & 0x80) != 0;
        let r0 = self.inner.chr_bank_reg(0) as usize;
        let r2 = self.inner.chr_bank_reg(2) as usize;
        let r3 = self.inner.chr_bank_reg(3) as usize;
        let slot_1k = (addr as usize) >> 10; // 0-7

        if !chr_mode {
            // Mode 0: 4KB at $0000, 2KB at $1000, 2KB at $1800
            match slot_1k {
                0..=3 => (r0 << 1) + slot_1k,       // 4KB block from reg[0]
                4..=5 => (r2 << 1) + (slot_1k - 4), // 2KB block from reg[2]
                6..=7 => (r3 << 1) + (slot_1k - 6), // 2KB block from reg[3]
                _ => 0,
            }
        } else {
            // Mode 1: 4KB at $0000, 2KB at $1000, 2KB at $1800 (all from r0 or r2)
            match slot_1k {
                0..=3 => (r2 << 1) + slot_1k,           // 4KB block from reg[2]
                4..=7 => (r0 << 1) + (slot_1k - 4) % 2, // reg[0] for both 2KB halves
                _ => 0,
            }
        }
    }
}

impl Mapper for Mapper197 {
    fn base(&self) -> &BaseMapper {
        &self.inner.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.inner.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.inner.mapped_prg_bank(addr);
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.inner.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.inner.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mapped_chr_bank(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.inner.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mapped_chr_bank(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.inner.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.inner.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.inner.restore_registers(data);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 8;
    const CHR_1K_BANKS: usize = 128;

    fn make_mapper() -> Mapper197 {
        Mapper197::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    /// Set CHR register `index` to `value` via standard MMC3 bank-select / bank-data writes.
    /// Preserves the existing mode bits (bits 7:6) of the bank-select register.
    fn set_chr_reg(mapper: &mut Mapper197, index: usize, value: u8) {
        let mode_bits = mapper.inner.bank_select_reg() & 0xC0;
        mapper.write_prg(0x8000, mode_bits | (index as u8 & 0x07)); // preserve mode, choose reg
        mapper.write_prg(0x8001, value); // bank data
    }

    /// Set the CHR mode bit (bit 7 of $8000).
    fn set_chr_mode(mapper: &mut Mapper197, mode1: bool) {
        let bs = mapper.inner.bank_select_reg();
        let new_bs = if mode1 { bs | 0x80 } else { bs & !0x80 };
        mapper.write_prg(0x8000, new_bs);
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_197_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 197 must be registered in the factory"
        );
    }

    // ── CHR mode 0 (bit 7 = 0) ────────────────────────────────────────────────

    #[test]
    fn mode0_reg0_selects_4kb_block_at_0000() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, false);
        set_chr_reg(&mut mapper, 0, 5); // reg[0] = 5 → base 1K bank = 10

        assert_eq!(mapper.read_chr(0x0000), 10, "$0000 must read 1K bank 10");
        assert_eq!(mapper.read_chr(0x0400), 11, "$0400 must read 1K bank 11");
        assert_eq!(mapper.read_chr(0x0800), 12, "$0800 must read 1K bank 12");
        assert_eq!(mapper.read_chr(0x0C00), 13, "$0C00 must read 1K bank 13");
    }

    #[test]
    fn mode0_reg2_selects_2kb_block_at_1000() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, false);
        set_chr_reg(&mut mapper, 2, 3); // reg[2] = 3 → base 1K bank = 6

        assert_eq!(mapper.read_chr(0x1000), 6, "$1000 must read 1K bank 6");
        assert_eq!(mapper.read_chr(0x1400), 7, "$1400 must read 1K bank 7");
    }

    #[test]
    fn mode0_reg3_selects_2kb_block_at_1800() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, false);
        set_chr_reg(&mut mapper, 3, 4); // reg[3] = 4 → base 1K bank = 8

        assert_eq!(mapper.read_chr(0x1800), 8, "$1800 must read 1K bank 8");
        assert_eq!(mapper.read_chr(0x1C00), 9, "$1C00 must read 1K bank 9");
    }

    #[test]
    fn mode0_reg2_does_not_affect_0000_to_0fff() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, false);
        set_chr_reg(&mut mapper, 0, 2); // reg[0] = 2 → bank 4 at $0000
        set_chr_reg(&mut mapper, 2, 10);
        assert_eq!(mapper.read_chr(0x0000), 4, "$0000 must still follow reg[0]");
    }

    // ── CHR mode 1 (bit 7 = 1) ────────────────────────────────────────────────

    #[test]
    fn mode1_reg2_selects_4kb_block_at_0000() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, true);
        set_chr_reg(&mut mapper, 2, 6); // reg[2] = 6 → base 1K bank = 12

        assert_eq!(mapper.read_chr(0x0000), 12, "$0000 must read 1K bank 12");
        assert_eq!(mapper.read_chr(0x0400), 13, "$0400 must read 1K bank 13");
        assert_eq!(mapper.read_chr(0x0800), 14, "$0800 must read 1K bank 14");
        assert_eq!(mapper.read_chr(0x0C00), 15, "$0C00 must read 1K bank 15");
    }

    #[test]
    fn mode1_reg0_selects_2kb_at_1000_and_1800() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, true);
        set_chr_reg(&mut mapper, 0, 7); // reg[0] = 7 → base 1K bank = 14

        assert_eq!(mapper.read_chr(0x1000), 14, "$1000 must read 1K bank 14");
        assert_eq!(mapper.read_chr(0x1400), 15, "$1400 must read 1K bank 15");
        assert_eq!(
            mapper.read_chr(0x1800),
            14,
            "$1800 must mirror reg[0] block"
        );
        assert_eq!(
            mapper.read_chr(0x1C00),
            15,
            "$1C00 must mirror reg[0] block"
        );
    }

    // ── PRG banking (standard MMC3, unchanged) ────────────────────────────────

    #[test]
    fn prg_e000_is_last_bank_at_power_on() {
        let mapper = make_mapper();
        let last = (PRG_8K_BANKS - 1) as u8;
        assert_eq!(
            mapper.read_prg(0xE000),
            last,
            "$E000 must be fixed to last 8KB bank"
        );
    }

    // ── No IRQ by default ─────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }

    // ── Snapshot round-trip ───────────────────────────────────────────────────

    #[test]
    fn snapshot_round_trips_chr_state() {
        let mut mapper = make_mapper();
        set_chr_mode(&mut mapper, false);
        set_chr_reg(&mut mapper, 0, 5);
        set_chr_reg(&mut mapper, 2, 3);
        set_chr_reg(&mut mapper, 3, 4);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "CHR $0000 must match"
        );
        assert_eq!(
            restored.read_chr(0x1000),
            mapper.read_chr(0x1000),
            "CHR $1000 must match"
        );
        assert_eq!(
            restored.read_chr(0x1800),
            mapper.read_chr(0x1800),
            "CHR $1800 must match"
        );
    }
}
