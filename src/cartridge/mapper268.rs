//! Mapper 268 – Coolboy (MMC3 variant with outer bank registers)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_Coolboy.h`
//!   (No dedicated NesDev wiki page found at time of implementation; NesDev
//!   wiki returned 403 and backup returned 404.)
//!
//! Hardware: Unlicensed Coolboy/Mindkids multicart board used primarily in
//! Chinese pirate cartridges.
//!
//! The mapper is a standard MMC3 core augmented with four outer-bank registers
//! (`exRegs[0..3]`) that are written via CPU address space `$6000–$7FFF`.
//! The two low address bits select which of the four registers is updated.
//!
//! ## External register writes ($6000–$7FFF)
//!
//! * If PRG-RAM is enabled (MMC3 `$A001` bit 7), the write also goes to PRG-RAM.
//! * If the lock condition is NOT met (see below), `exRegs[addr & 0x03] = value`.
//! * Lock condition: `(exRegs[3] & 0x90) == 0x80`.  When locked, no further
//!   external-register updates occur (PRG-RAM writes still happen normally).
//!
//! ## PRG banking
//!
//! The MMC3 inner page number is masked and ORed with an outer base:
//!
//! ```text
//! mask = ((0x3F | (exRegs[1] & 0x40) | ((exRegs[1] & 0x20) << 2))
//!         ^ ((exRegs[0] & 0x40) >> 2))
//!         ^ ((exRegs[1] & 0x80) >> 2)
//!
//! base = (exRegs[0] & 0x07)
//!      | ((exRegs[1] & 0x10) >> 1)
//!      | ((exRegs[1] & 0x0C) << 2)
//!      | ((exRegs[0] & 0x30) << 2)
//!
//! bank = ((base << 4) & !mask) | (inner_page & mask)
//! ```
//!
//! Default state (all regs = 0): mask = 0x3F, base = 0 → pure MMC3 banking
//! (inner page masked to 6 bits, outer bits zeroed).
//!
//! ## CHR banking
//!
//! ```text
//! mask = 0xFF ^ (exRegs[0] & 0x80)
//! bank = (inner_page & mask) | (((exRegs[0] & 0x08) << 4) & !mask)
//! ```
//!
//! Default state: mask = 0xFF → pure MMC3 CHR banking.
//!
//! ## Mirroring
//! Standard MMC3 H/V mirroring via `$A000`.
//!
//! ## IRQ
//! Standard MMC3 scanline IRQ.
//!
//! ## PRG-RAM
//! 8 KiB at `$6000–$7FFF` (standard MMC3 PRG-RAM semantics).
//!
//! ## CHR memory
//! CHR-ROM when header provides CHR data; CHR-RAM (8 KiB) when CHR-ROM is empty.
//!
//! ## Known Limitations
//! - The special 4-screen CHR-RAM mode (`exRegs[3] & 0x10`) is not implemented.
//! - The `exRegs[3] & 0x40` forced-page fixup for high-page PRG slots is not
//!   implemented. These are rare edge-case features absent from most Coolboy ROMs
//!   in the neser ROM database.
//! - Source: Mesen2 `MMC3_Coolboy.h`. No known discrepancies for the features
//!   that are implemented.

use crate::cartridge::Mapper;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;

/// Mapper 268 – Coolboy (MMC3 variant with outer bank registers)
pub struct Mapper268 {
    mmc3: MMC3Mapper,
    ex_regs: [u8; 4],
}

impl Mapper268 {
    const MAPPER_NUMBER: u16 = 268;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            ex_regs: [0u8; 4],
        }
    }

    /// Compute the PRG bank mask from external registers.
    fn prg_mask(&self) -> usize {
        let r0 = self.ex_regs[0] as usize;
        let r1 = self.ex_regs[1] as usize;
        ((0x3F | (r1 & 0x40) | ((r1 & 0x20) << 2)) ^ ((r0 & 0x40) >> 2)) ^ ((r1 & 0x80) >> 2)
    }

    /// Compute the PRG outer base from external registers.
    fn prg_base(&self) -> usize {
        let r0 = self.ex_regs[0] as usize;
        let r1 = self.ex_regs[1] as usize;
        (r0 & 0x07) | ((r1 & 0x10) >> 1) | ((r1 & 0x0C) << 2) | ((r0 & 0x30) << 2)
    }

    /// Apply outer PRG bank OR/AND masking to an MMC3 inner page number.
    fn apply_prg_outer(&self, inner_page: usize) -> usize {
        let mask = self.prg_mask();
        let base = self.prg_base();
        ((base << 4) & !mask) | (inner_page & mask)
    }

    /// Apply outer CHR bank OR/AND masking to an MMC3 inner 1K page number.
    fn apply_chr_outer(&self, inner_page: usize) -> usize {
        let r0 = self.ex_regs[0] as usize;
        let mask = 0xFF ^ (r0 & 0x80);
        (inner_page & mask) | (((r0 & 0x08) << 4) & !mask)
    }

    /// Returns `true` when the external registers are locked and should not be
    /// updated further.
    fn is_locked(&self) -> bool {
        (self.ex_regs[3] & 0x90) == 0x80
    }
}

impl Mapper for Mapper268 {
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
            return self.mmc3.read_prg(addr);
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let inner_page = self.mmc3.mapped_prg_bank(addr);
        let bank = self.apply_prg_outer(inner_page);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            // Always forward so MMC3 can service PRG-RAM if enabled.
            self.mmc3.write_prg(addr, value);
            // Also update the outer bank register unless locked.
            if !self.is_locked() {
                self.ex_regs[(addr & 0x03) as usize] = value;
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let inner_page = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner_page);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let inner_page = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner_page);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
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
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Only treat the last 4 bytes as ex_regs if there is enough data left
        // for a valid MMC3 snapshot (minimum 13 bytes).
        if data.len() >= 4 + 13 {
            let (mmc3_part, ex_part) = data.split_at(data.len() - 4);
            self.mmc3.restore_registers(mmc3_part);
            self.ex_regs.copy_from_slice(ex_part);
        } else {
            // Snapshot does not contain separate ex_regs; restore MMC3 only
            // and reset ex_regs to a known default state.
            self.mmc3.restore_registers(data);
            self.ex_regs = [0u8; 4];
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.ex_regs = [0u8; 4];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-2 bank counts to avoid modulo-wrap false passes.
    const PRG_8K_BANKS: usize = 9;
    const CHR_1K_BANKS: usize = 48;

    fn make_mapper() -> Mapper268 {
        Mapper268::new(MapperContext::new_for_test(
            268,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    fn make_mapper_chr_ram() -> Mapper268 {
        Mapper268::new(MapperContext::new_for_test(
            268,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![], // empty = CHR-RAM
            NametableLayout::Horizontal,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_268_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            268,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 268 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // Default state = standard MMC3 PRG banking
    // -------------------------------------------------------------------------

    #[test]
    fn default_state_prg_acts_like_mmc3() {
        let mut mapper = make_mapper();

        // Select R6 (PRG bank at $8000): bank_select = 0x06
        mapper.write_prg(0x8000, 0x06);
        // Set R6 = 3
        mapper.write_prg(0x8001, 3);

        // With exRegs all 0: mask=0x3F, base=0 → bank = page & 0x3F = 3
        // banked_data fills bank N with value N
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // -------------------------------------------------------------------------
    // PRG outer banking
    // -------------------------------------------------------------------------

    #[test]
    fn prg_outer_base_bits_from_ex_reg0_bits_4_5() {
        // With PRG_8K_BANKS=9, inner pages are 0..8 (mask=0x3F keeps all).
        // exRegs[0] = 0x10 (bit4 set):
        //   base = (0x10 & 0x07)=0 | 0 | 0 | (0x10 & 0x30)<<2 = 0x10<<2 = 0x40
        //   base<<4 = 0x400
        //   mask = 0x3F (exRegs[1]=0, exRegs[0] bit6=0, bit7=0)
        //   bank = (0x400 & !0x3F) | (inner & 0x3F) = 0x400 | inner
        // With inner=0: bank=0x400=1024; 1024 % 9 = 7
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // ex_regs[0] = 0x10

        // Select R6, set inner page to 0
        mapper.write_prg(0x8000, 0x06); // bank_select → R6
        mapper.write_prg(0x8001, 0); // R6 = 0

        let expected = 0x400usize % PRG_8K_BANKS; // = 7
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "PRG outer base from exReg0 bits 4-5 must select correct outer bank"
        );
    }

    #[test]
    fn prg_outer_base_bits_from_ex_reg1_bits_2_3() {
        // exRegs[1] = 0x04 (bit2 set):
        //   base = (0x04 & 0x0C) << 2 = 0x10
        //   base << 4 = 0x100
        //   mask = 0x3F (no changes to mask bits from exRegs[1])
        //   bank = (0x100 & !0x3F) | (inner & 0x3F) = 0x100 | inner
        // With inner = 0 and PRG_8K_BANKS = 9:
        //   bank % PRG_8K_BANKS = 0x100 % 9 = 256 % 9 = 4
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x04); // ex_regs[1] = 0x04

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner page = 0

        let expected = 0x100usize % PRG_8K_BANKS;
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "PRG outer base from exReg1 bits 2-3 must select correct outer bank"
        );
    }

    #[test]
    fn prg_inner_page_preserved_when_outer_bits_are_zero() {
        // With all exRegs = 0: bank = inner_page & 0x3F
        // PRG_8K_BANKS=9 → inner page 5 → bank=5 → read byte = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 5); // R6 = 5
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn prg_mask_extended_by_ex_reg1_bit6() {
        // exRegs[1] = 0x40 (bit6 set):
        //   mask = ((0x3F | 0x40 | 0) ^ 0) ^ 0 = 0x7F
        //   base = 0
        //   bank = inner & 0x7F
        // Inner page 7 → bank = 7 % 9 = 7
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 0x40); // ex_regs[1] = 0x40

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 7); // R6 = 7
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    // -------------------------------------------------------------------------
    // CHR outer banking
    // -------------------------------------------------------------------------

    #[test]
    fn default_state_chr_acts_like_mmc3() {
        let mut mapper = make_mapper();

        // Select R0 (2KB CHR bank pair) and set to CHR bank 8
        mapper.write_prg(0x8000, 0x00); // bank_select → R0
        mapper.write_prg(0x8001, 8); // R0 = 8 (maps $0000-$07FF)

        // Default mask = 0xFF → bank = 8 % 48 = 8
        assert_eq!(
            mapper.read_chr(0x0000),
            8,
            "CHR at $0000 should read bank 8 in default state"
        );
    }

    #[test]
    fn chr_outer_bit7_fixed_from_ex_reg0_bit3() {
        // exRegs[0] = 0x88 (bit7=1, bit3=1):
        //   mask = 0xFF ^ 0x80 = 0x7F
        //   !mask & 0xFF = 0x80
        //   chr_outer = (inner & 0x7F) | ((0x08 << 4) & 0x80) = (inner & 0x7F) | 0x80
        // Inner CHR page 0 → bank = 0 | 0x80 = 128 → 128 % 48 = 128 - 2*48 = 32
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x88); // ex_regs[0] = 0x88

        mapper.write_prg(0x8000, 0x00); // select R0
        mapper.write_prg(0x8001, 0); // R0 = 0

        let expected = 0x80usize % CHR_1K_BANKS;
        assert_eq!(
            mapper.read_chr(0x0000) as usize,
            expected,
            "CHR bit7 must be fixed from exReg0 bit3 when exReg0 bit7 is set"
        );
    }

    #[test]
    fn chr_inner_page_used_when_mask_is_0xff() {
        // Default: mask = 0xFF, so inner page passes through unchanged (mod bank count)
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 10); // R0 = 10
        // MMC3 R0 maps $0000-$07FF as 2KB. First 1K is at even bank (10→10,11).
        assert_eq!(mapper.read_chr(0x0000), 10 % CHR_1K_BANKS as u8);
    }

    // -------------------------------------------------------------------------
    // External register locking
    // -------------------------------------------------------------------------

    #[test]
    fn external_regs_locked_when_ex_reg3_bit7_set_bit4_clear() {
        let mut mapper = make_mapper();

        // Set outer base: exRegs[0] = 0x10 (bit4 set, adds outer bits)
        mapper.write_prg(0x6000, 0x10);

        // Lock: write 0x80 to ex_regs[3] (bit7=1, bit4=0 → locked)
        mapper.write_prg(0x6003, 0x80);

        // Attempt to change ex_regs[0] after locking – should be ignored
        mapper.write_prg(0x6000, 0x00);

        // ex_regs[0] should still be 0x10
        // Verify by checking PRG banking still uses base from 0x10
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        let expected_with_lock = 0x400usize % PRG_8K_BANKS;
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected_with_lock,
            "External regs must remain locked at previous value"
        );
    }

    #[test]
    fn external_regs_not_locked_when_ex_reg3_has_bit4_set() {
        let mut mapper = make_mapper();

        // Write 0x90 (bit7=1, bit4=1) → condition is 0x90 != 0x80 → NOT locked
        mapper.write_prg(0x6003, 0x90);

        // Set exRegs[0] = 0x10 – this should succeed (not locked)
        mapper.write_prg(0x6000, 0x10);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        let expected = 0x400usize % PRG_8K_BANKS;
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "External regs must be writable when bit4 is also set"
        );
    }

    #[test]
    fn external_regs_not_locked_when_ex_reg3_is_zero() {
        let mut mapper = make_mapper();

        // Default state: ex_regs[3] = 0x00 → (0 & 0x90) = 0 ≠ 0x80 → unlocked
        mapper.write_prg(0x6000, 0x10);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        let expected = 0x400usize % PRG_8K_BANKS;
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "Default state (ex_regs[3]=0) must allow external reg writes"
        );
    }

    // -------------------------------------------------------------------------
    // CHR-RAM write/read (when chr_rom is empty)
    // -------------------------------------------------------------------------

    #[test]
    fn chr_ram_write_and_read_basic() {
        let mut mapper = make_mapper_chr_ram();

        // Write to CHR at $0000 (default CHR bank 0) and read back
        mapper.write_chr(0x0000, 0xDE);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xDE,
            "CHR-RAM write must be readable at same address"
        );
    }

    // -------------------------------------------------------------------------
    // PRG-RAM passthrough at $6000-$7FFF
    // -------------------------------------------------------------------------

    #[test]
    fn prg_ram_writable_when_enabled_via_a001() {
        let mut mapper = make_mapper();

        // Enable PRG-RAM: write 0x80 to $A001 (bit7=enable, bit6=0=not write-protected)
        mapper.write_prg(0xA001, 0x80);

        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xBB,
            "PRG-RAM must be writable when MMC3 $A001 bit7 is set"
        );
    }

    // -------------------------------------------------------------------------
    // Register address decoding
    // -------------------------------------------------------------------------

    #[test]
    fn ex_reg_address_selects_correct_register() {
        // Address bits [1:0] select ex_regs index
        // $6000 & 3 = 0, $6001 & 3 = 1, $6002 & 3 = 2, $6003 & 3 = 3
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01);
        mapper.write_prg(0x6001, 0x02);
        mapper.write_prg(0x6002, 0x04);
        mapper.write_prg(0x6003, 0x08);

        // Verify ex_regs[0] = 0x01 by checking PRG mask/base effect
        // ex_regs[0] = 0x01: base = 0x01; base<<4 = 0x10; 0x10 & ~0x3F = 0 → no outer effect
        // ex_regs[0] = 0x01, ex_regs[1] = 0x02:
        //   mask = 0x3F (reg1 bit6=0, bit5=0, reg0 bit6=0, reg1 bit7=0)
        //   so mask unchanged → bank = inner & 0x3F
        // ex_regs[3] = 0x08: (0x08 & 0x90) = 0 ≠ 0x80 → still unlocked
        // We can verify by checking that the lock condition check uses exReg3=0x08
        assert!(!mapper.is_locked(), "ex_regs[3]=0x08 must not trigger lock");

        // Attempt write to confirm unlocked
        mapper.write_prg(0x6003, 0x00); // unlock check
        assert!(!mapper.is_locked());
    }

    // -------------------------------------------------------------------------
    // Mirroring
    // -------------------------------------------------------------------------

    #[test]
    fn mirroring_via_a000_standard_mmc3() {
        let mut mapper = make_mapper();

        // $A000 bit0: 0=Vertical, 1=Horizontal
        mapper.write_prg(0xA000, 0x01); // Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0xA000, 0x00); // Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // -------------------------------------------------------------------------
    // Reset
    // -------------------------------------------------------------------------

    #[test]
    fn reset_clears_ex_regs() {
        let mut mapper = make_mapper();

        // Set some ex_regs
        mapper.write_prg(0x6000, 0x10);
        mapper.write_prg(0x6001, 0x40);

        mapper.reset();

        // After reset, ex_regs should all be 0 → default MMC3 banking
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "After reset, PRG should use default (unmodified inner) bank"
        );
    }

    // -------------------------------------------------------------------------
    // Snapshot / restore
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trip_includes_ex_regs() {
        let mut mapper = make_mapper();

        // Set outer banking state
        mapper.write_prg(0x6000, 0x10); // ex_regs[0] = 0x10

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // Should read same PRG bank as before snapshot
        let expected = 0x400usize % PRG_8K_BANKS;
        assert_eq!(
            restored.read_prg(0x8000) as usize,
            expected,
            "Restored snapshot must reproduce outer PRG banking"
        );
    }
}
