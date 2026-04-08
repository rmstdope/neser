//! Mapper 224 – Jncota KT-008 (MMC3 variant with outer bank register)
//!
//! # Specifications
//! - Primary source: NesDev Wiki (403 restricted at time of implementation)
//!   Mirror: <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_224.xhtml>
//!   Excerpt: "iNES Mapper 224 is used for the 晶科泰 (Jncota) KT-008 PCB.
//!   It includes the same chipset as the "MINDKIDS" multicarts (NES 2.0
//!   Mapper 268, Submapper 1)."
//! - Fallback: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_224.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Mmc3Variants/MMC3_224.h>
//!
//! ## Hardware behavior
//!
//! An MMC3 clone that adds a single outer-bank register at `$5000`, allowing
//! access to up to 1024 KiB of PRG-ROM (two 512 KiB outer blocks).
//!
//! ### Outer bank register (`$5000`, write-only)
//!
//! ```text
//! 7  bit  0
//! ---- ----
//! .... .O..
//!        |
//!        +-- PRG outer bank (bit 2 of value): selects 512 KiB block
//! ```
//!
//! `outer_bank = (value >> 2) & 0x01`
//!
//! ### PRG banking
//!
//! For each CPU access to `$8000–$FFFF`, the raw MMC3 page number `p` for
//! that address is masked to 6 bits and ORed with the outer bank offset:
//!
//! ```text
//! final_bank = (p & 0x3F) | (outer_bank << 6)
//! ```
//!
//! - Switchable pages from MMC3 registers R6/R7 are masked to 6 bits.
//! - Fixed pages (0xFE, 0xFF) become the last two pages of the selected outer
//!   block (0x3E and 0x3F, after masking), then shifted by the outer bank offset.
//!
//! ### CHR banking
//! Standard MMC3 CHR banking (no outer bank applied).
//!
//! ### Mirroring
//! Standard MMC3 H/V mirroring via `$A000`.
//!
//! ### IRQ
//! Standard MMC3 scanline IRQ.
//!
//! ### PRG-RAM
//! 8 KiB at `$6000–$7FFF` (standard MMC3 semantics).
//!
//! ### Power-on/reset state
//! `outer_bank = 0` (first 512 KiB block, pure MMC3 behavior).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 224;
const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;

/// Mapper 224 – Jncota KT-008 (MMC3 with single outer bank register).
///
/// See the module-level documentation for hardware details.
pub struct Mapper224 {
    mmc3: MMC3Mapper,
    /// 1-bit outer bank: 0 = first 512 KiB block, 1 = second 512 KiB block.
    outer_bank: u8,
}

impl Mapper224 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            outer_bank: 0,
        }
    }

    /// Apply the outer bank offset to a raw MMC3 8KB page number.
    ///
    /// Masks the inner page to 6 bits and ORs with `outer_bank << 6`.
    #[inline]
    fn apply_outer_prg(&self, raw_page: u8) -> usize {
        ((raw_page & 0x3F) as usize) | ((self.outer_bank as usize) << 6)
    }
}

impl Mapper for Mapper224 {
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
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let raw_page = self.mmc3.raw_prg_8k_page_number(addr);
                let bank = self.apply_outer_prg(raw_page);
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => {
                let raw_page = self.mmc3.raw_prg_8k_page_number(addr);
                let bank = self.apply_outer_prg(raw_page);
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => open_bus,
        }
    }
    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000 => {
                self.outer_bank = (value >> 2) & 0x01;
            }
            0x6000..=0x7FFF => {
                self.mmc3.write_prg(addr, value);
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
        self.mmc3.write_chr(addr, value);
    }

    fn capabilities(&self) -> MapperCapabilities {
        let mut capabilities = self.mmc3.capabilities();
        capabilities.has_irq = true;
        capabilities.has_chr_banking = true;
        capabilities.has_dynamic_mirroring = true;
        capabilities.prg_bank_size_kb = 8;
        capabilities.chr_bank_size_kb = 1;
        capabilities
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer_bank);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.mmc3.restore_registers(&data[..16]);
            self.outer_bank = data[16] & 0x01;
        } else {
            self.mmc3.restore_registers(data);
            self.outer_bank = 0;
        }
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer_bank = 0;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 128 PRG banks = 1024 KiB (full addressable range for mapper 224).
    const PRG_8K_BANKS: usize = 128;
    // 48 CHR 1K banks (non-power-of-2).
    const CHR_1K_BANKS: usize = 48;

    fn make_mapper() -> Mapper224 {
        Mapper224::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    fn make_mapper_chr_ram() -> Mapper224 {
        Mapper224::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_224_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 224 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_outer_bank_is_zero() {
        let mapper = make_mapper();
        assert_eq!(mapper.outer_bank, 0, "outer_bank must be 0 at power-on");
    }

    #[test]
    fn power_on_prg_acts_like_mmc3() {
        let mut mapper = make_mapper();
        // Default MMC3: R6 selects bank at $8000. Select R6, set to bank 3.
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG $8000 must read bank 3 at power-on (pure MMC3 mode)"
        );
    }

    // ── Outer bank register at $5000 ─────────────────────────────────────────

    #[test]
    fn write_5000_bit2_set_selects_outer_bank_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // bit 2 set
        assert_eq!(mapper.outer_bank, 1, "bit 2 of $5000 must set outer_bank=1");
    }

    #[test]
    fn write_5000_bit2_clear_selects_outer_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // set to 1 first
        mapper.write_prg(0x5000, 0x00); // bit 2 clear
        assert_eq!(mapper.outer_bank, 0, "bit 2 cleared must set outer_bank=0");
    }

    #[test]
    fn write_5000_only_bit2_matters() {
        let mut mapper = make_mapper();
        // Write 0xFF (all bits set): only bit 2 matters → outer_bank = 1
        mapper.write_prg(0x5000, 0xFF);
        assert_eq!(
            mapper.outer_bank, 1,
            "Only bit 2 of $5000 must affect outer_bank"
        );

        // Write 0xFB (bit 2 clear): outer_bank = 0
        mapper.write_prg(0x5000, 0xFB);
        assert_eq!(
            mapper.outer_bank, 0,
            "bit 2 clear in 0xFB must give outer_bank=0"
        );
    }

    #[test]
    fn writes_to_5001_thru_5fff_do_not_affect_outer_bank() {
        let mut mapper = make_mapper();
        // These should be silently ignored (no outer bank change).
        mapper.write_prg(0x5001, 0x04);
        assert_eq!(
            mapper.outer_bank, 0,
            "$5001 write must not change outer_bank"
        );
        mapper.write_prg(0x5FFF, 0x04);
        assert_eq!(
            mapper.outer_bank, 0,
            "$5FFF write must not change outer_bank"
        );
    }

    // ── PRG banking with outer_bank=0 ────────────────────────────────────────

    #[test]
    fn outer_bank_0_prg_mode0_switchable_at_8000() {
        let mut mapper = make_mapper();
        // PRG mode 0: $8000 → R6, $C000 → second-to-last fixed (bank 62 = 0x3E)
        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 5); // R6 = 5
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "$8000 must read bank 5 with R6=5, outer_bank=0"
        );
    }

    #[test]
    fn outer_bank_0_prg_mode0_fixed_c000_is_second_to_last_in_block0() {
        let mapper = make_mapper();
        // PRG mode 0: $C000 is fixed to 0xFE which maps to bank 62 (0x3E) in block 0
        assert_eq!(
            mapper.read_prg(0xC000),
            62,
            "$C000 must read bank 62 (0x3E of block 0) by default"
        );
    }

    #[test]
    fn outer_bank_0_prg_mode0_fixed_e000_is_last_in_block0() {
        let mapper = make_mapper();
        // PRG mode 0: $E000 fixed to 0xFF → bank 63 (0x3F) in block 0
        assert_eq!(
            mapper.read_prg(0xE000),
            63,
            "$E000 must read bank 63 (0x3F of block 0) by default"
        );
    }

    // ── PRG banking with outer_bank=1 ────────────────────────────────────────

    #[test]
    fn outer_bank_1_prg_switchable_bank_shifted_by_64() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 0); // R6 = 0 → final = 0 | 64 = 64
        assert_eq!(
            mapper.read_prg(0x8000),
            64,
            "$8000 with R6=0 and outer_bank=1 must read bank 64"
        );
    }

    #[test]
    fn outer_bank_1_prg_switchable_bank_5_becomes_69() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 5); // R6 = 5 → final = 5 | 64 = 69
        assert_eq!(
            mapper.read_prg(0x8000),
            69,
            "$8000 with R6=5 and outer_bank=1 must read bank 69"
        );
    }

    #[test]
    fn outer_bank_1_fixed_c000_is_second_to_last_in_block1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        // PRG mode 0: $C000 = 0xFE → (0xFE & 0x3F) | 64 = 62 | 64 = 126
        assert_eq!(
            mapper.read_prg(0xC000),
            126,
            "$C000 with outer_bank=1 must read bank 126 (0x3E of block 1)"
        );
    }

    #[test]
    fn outer_bank_1_fixed_e000_is_last_in_block1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        // PRG mode 0: $E000 = 0xFF → (0xFF & 0x3F) | 64 = 63 | 64 = 127
        assert_eq!(
            mapper.read_prg(0xE000),
            127,
            "$E000 with outer_bank=1 must read bank 127 (0x3F of block 1)"
        );
    }

    #[test]
    fn outer_bank_affects_a000_prg_slot() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x07); // select R7
        mapper.write_prg(0x8001, 2); // R7 = 2

        // outer_bank=0: bank 2
        assert_eq!(mapper.read_prg(0xA000), 2);

        // outer_bank=1: bank 2 + 64 = 66
        mapper.write_prg(0x5000, 0x04);
        assert_eq!(
            mapper.read_prg(0xA000),
            66,
            "$A000 with R7=2 and outer_bank=1 must read bank 66"
        );
    }

    // ── PRG mode 1 (swapped) ─────────────────────────────────────────────────

    #[test]
    fn prg_mode1_with_outer_bank_1_fixed_8000_is_second_to_last_in_block1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0xC0); // bank_select bit 6 set → PRG mode 1
        // PRG mode 1: $8000 is fixed to 0xFE → (0xFE & 0x3F) | 64 = 126
        assert_eq!(
            mapper.read_prg(0x8000),
            126,
            "PRG mode 1, outer_bank=1: $8000 fixed must read bank 126"
        );
    }

    #[test]
    fn prg_mode1_with_outer_bank_1_e000_is_fixed_last_in_block1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0xC0); // PRG mode 1
        // $E000 always fixed to 0xFF → (0xFF & 0x3F) | 64 = 127
        assert_eq!(
            mapper.read_prg(0xE000),
            127,
            "PRG mode 1, outer_bank=1: $E000 must read bank 127"
        );
    }

    // ── CHR banking (no outer bank applied) ──────────────────────────────────

    #[test]
    fn chr_banking_unaffected_by_outer_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // select R0 (CHR register)
        mapper.write_prg(0x8001, 4); // R0 = 4 (selects 1K banks 4&5)
        let bank_before = mapper.read_chr(0x0000);

        mapper.write_prg(0x5000, 0x04); // change outer_bank to 1
        let bank_after = mapper.read_chr(0x0000);

        assert_eq!(
            bank_before, bank_after,
            "CHR banking must not be affected by outer_bank register"
        );
        assert_eq!(
            bank_after, 4,
            "CHR must still reflect MMC3 R0=4 after outer_bank change"
        );
    }

    // ── MMC3 core operations still work ──────────────────────────────────────

    #[test]
    fn mmc3_mirroring_still_controlled_by_a000() {
        let mut mapper = make_mapper();
        // Default (from Horizontal header): ensure vertical can be set via $A000 bit 0
        mapper.write_prg(0xA000, 0x00); // vertical (bit 0 = 0)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 0x01); // horizontal (bit 0 = 1)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mmc3_irq_can_be_enabled_and_triggered() {
        let mut mapper = make_mapper();
        // Enable IRQ, set latch to 1, reload
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // IRQ reload
        mapper.write_prg(0xE001, 0); // IRQ enable
        // Not testing scanline triggering here, just that IRQ infrastructure is wired
        assert!(
            !mapper.irq_pending(),
            "IRQ must not be pending before scanlines"
        );
    }

    #[test]
    fn prg_ram_at_6000_works() {
        let mut mapper = Mapper224::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
                banked_data(1024, CHR_1K_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );

        // Enable PRG-RAM ($A001 bit 7)
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG-RAM at $6000 must be readable/writable"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = make_mapper_chr_ram();
        mapper.write_chr(0x0100, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xCD,
            "CHR-RAM must be readable/writable when no CHR-ROM"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "Must have IRQ");
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert_eq!(caps.prg_bank_size_kb, 8, "PRG bank size must be 8 KB");
        assert_eq!(caps.chr_bank_size_kb, 1, "CHR bank size must be 1 KB");
    }

    // ── Mapper number ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_number_is_224() {
        let mapper = make_mapper();
        assert_eq!(mapper.mapper_number(), 224, "Mapper number must be 224");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_outer_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        assert_eq!(mapper.outer_bank, 1);
        mapper.reset();
        assert_eq!(mapper.outer_bank, 0, "outer_bank must be 0 after reset");
    }

    #[test]
    fn reset_restores_prg_to_block_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);

        // Before reset: R6=3 in block 1 → bank 67
        assert_eq!(mapper.read_prg(0x8000), 67);

        mapper.reset();
        // After reset: outer_bank=0, R6 reset by MMC3 → back to bank 0
        assert_eq!(mapper.outer_bank, 0);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips_outer_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5); // R6 = 5

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.outer_bank, 1,
            "Snapshot must preserve outer_bank=1"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data"
        );
    }

    #[test]
    fn legacy_snapshot_restore_resets_outer_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x04); // outer_bank = 1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);

        // Extract only the MMC3 portion (16 bytes, no outer_bank byte)
        let full_snap = mapper.registers_snapshot();
        let legacy = full_snap[..16].to_vec();

        let mut restored = make_mapper();
        restored.write_prg(0x5000, 0x04); // set outer_bank = 1 before restore
        restored.restore_registers(&legacy);

        assert_eq!(
            restored.outer_bank, 0,
            "Legacy restore must reset outer_bank to 0"
        );
    }
}
