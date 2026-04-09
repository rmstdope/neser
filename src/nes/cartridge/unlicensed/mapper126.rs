//! Mapper 126 - MMC3 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_126>
//! - Reference: Mesen MMC3_126.h
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 126 - MMC3-based multicart
///
/// Hardware: MMC3 with four external bank-control registers at $6000-$7FFF
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_126>
///
/// External registers ($6000-$7FFF, addressed by A0-A1):
/// - exReg\[0\] ($6000): PRG/CHR outer bank control + CHR inner mask
/// - exReg\[1\] ($6001): always writable (unused by banking logic)
/// - exReg\[2\] ($6002): CHR outer bank supplement
/// - exReg\[3\] ($6003): PRG mode (NROM-128/256) and CHR linear mode + lock
///
/// Register locking: exReg\[3\] bit 7 locks writes to exReg\[0\] and exReg\[3\]
///
/// PRG bank mapping:
/// - Normal (exReg\[3\] bits 0-1 = 0): standard MMC3 with outer masking
/// - NROM-128 (bits 0-1 = 1 or 2): 16 KiB mirrored to 32 KiB
/// - NROM-256 (bits 0-1 = 3): 32 KiB sequential
///
/// CHR bank mapping:
/// - Normal (exReg\[3\] bit 4 = 0): MMC3 with outer bank OR + inner mask
/// - Linear (exReg\[3\] bit 4 = 1): 8 KiB sequential CHR from outer + exReg\[2\]
pub struct Mapper126 {
    pub(crate) mmc3: MMC3Mapper,
    ex_regs: [u8; 4],
}

impl Mapper126 {
    const MAPPER_NUMBER: u8 = 126;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            ex_regs: [0; 4],
        }
    }

    /// Applies outer bank masking to a raw PRG 8 KiB page number.
    ///
    /// Translated from Mesen MMC3_126::SelectPrgPage:
    ///   page &= ((~reg >> 2) & 0x10) | 0x0F
    ///   page |= (reg & (0x06 | (reg & 0x40) >> 6)) << 4 | (reg & 0x10) << 3
    fn apply_prg_outer(&self, raw_page: u8) -> usize {
        let reg = self.ex_regs[0] as usize;
        let inner_mask = ((!reg >> 2) & 0x10) | 0x0F;
        let outer = (reg & (0x06 | ((reg & 0x40) >> 6))) << 4 | (reg & 0x10) << 3;
        ((raw_page as usize) & inner_mask) | outer
    }

    /// Computes the CHR outer bank from exRegs\[0\] and exRegs\[2\].
    ///
    /// Bit-field assembly from Mesen MMC3_126::GetChrOuterBank:
    ///   bit 7  (0x0080): ~reg & 0x80 masked by exRegs\[2\], OR reg bits 3+7
    ///   bit 8  (0x0100): reg bit 5
    ///   bit 9  (0x0200): reg bit 4
    fn chr_outer_bank(&self) -> usize {
        let reg = self.ex_regs[0] as usize;
        let reg2 = self.ex_regs[2] as usize;
        (!reg & 0x0080 & reg2)
            | ((reg << 4) & 0x0080 & reg)
            | ((reg << 3) & 0x0100)
            | ((reg << 5) & 0x0200)
    }

    /// CHR inner mask: exRegs\[0\] bit 7 = 1 → 0x7F, = 0 → 0xFFFF (no masking).
    fn chr_inner_mask(&self) -> usize {
        if (self.ex_regs[0] & 0x80) != 0 {
            0x7F
        } else {
            0xFFFF
        }
    }

    fn is_chr_linear_mode(&self) -> bool {
        (self.ex_regs[3] & 0x10) != 0
    }

    /// Linear CHR base: outer bank OR (exRegs\[2\] low nibble << 3).
    fn chr_linear_base(&self) -> usize {
        self.chr_outer_bank() | ((self.ex_regs[2] as usize & 0x0F) << 3)
    }
}

impl Mapper for Mapper126 {
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        let nrom_mode = self.ex_regs[3] & 0x03;

        if nrom_mode == 0 {
            let raw_page = self.mmc3.raw_prg_8k_page_number(addr);
            let bank = self.apply_prg_outer(raw_page);
            self.mmc3.read_prg_at_bank(bank, offset)
        } else {
            // NROM mode: R6 is the base page
            let prg_mode = (self.mmc3.bank_select_reg() & 0x40) != 0;
            let r6_addr = if prg_mode { 0xC000 } else { 0x8000 };
            let r6_raw = self.mmc3.raw_prg_8k_page_number(r6_addr);
            let base = self.apply_prg_outer(r6_raw);
            let slot = ((addr - 0x8000) >> 13) as usize;
            let bank = if nrom_mode == 0x03 {
                base + slot
            } else {
                base + (slot & 1)
            };
            self.mmc3.read_prg_at_bank(bank, offset)
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            let reg_index = (addr & 0x03) as usize;
            let locked = (self.ex_regs[3] & 0x80) != 0;
            if reg_index == 1 || reg_index == 2 || !locked {
                self.ex_regs[reg_index] = value;
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        if self.is_chr_linear_mode() {
            let base = self.chr_linear_base();
            let slot = (addr as usize & 0x1FFF) >> 10;
            self.mmc3.read_chr_1k_at(base + slot, offset)
        } else {
            let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
            let bank = self.chr_outer_bank() | (raw_bank & self.chr_inner_mask());
            self.mmc3.read_chr_1k_at(bank, offset)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        if self.is_chr_linear_mode() {
            let base = self.chr_linear_base();
            let slot = (addr as usize & 0x1FFF) >> 10;
            self.mmc3.write_chr_1k_at(base + slot, offset, value);
        } else {
            let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
            let bank = self.chr_outer_bank() | (raw_bank & self.chr_inner_mask());
            self.mmc3.write_chr_1k_at(bank, offset, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.extend_from_slice(&self.ex_regs);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let mmc3_snapshot_len = self.mmc3.registers_snapshot().len();

        if data.len() >= mmc3_snapshot_len + self.ex_regs.len() {
            let (mmc3_data, ex_data) = data.split_at(data.len() - self.ex_regs.len());
            self.ex_regs.copy_from_slice(ex_data);
            self.mmc3.restore_registers(mmc3_data);
        } else {
            self.ex_regs = [0; 4];
            self.mmc3.restore_registers(data);
        }
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
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 256 PRG 8K banks = 2 MB, 256 CHR 1K banks = 256 KB
    const PRG_BANKS: usize = 256;
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            126,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 126 should be implemented")
    }

    /// Write to external register via $6000-$7FFF.
    fn write_ex_reg(mapper: &mut Box<dyn Mapper>, reg: u16, value: u8) {
        mapper.write_prg(0x6000 | reg, value);
    }

    // --- Factory ---

    #[test]
    fn mapper_126_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            126,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 126 must be registered in the factory"
        );
    }

    // --- Normal MMC3 PRG banking with outer mask ---

    #[test]
    fn prg_default_fixed_last_bank() {
        // With exRegs[0] = 0, inner mask = 0x1F, outer = 0.
        // Fixed last bank raw = 0xFF. Masked: 0xFF & 0x1F | 0 = 0x1F = 31.
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Default fixed-last bank must be 0xFF & 0x1F = 31"
        );
    }

    #[test]
    fn prg_r6_banking_in_default_block() {
        // R6 = 3, exRegs[0] = 0 → bank = 3 & 0x1F | 0 = 3
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select = R6
        mapper.write_prg(0x8001, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "R6=3 with no outer bank must read bank 3"
        );
    }

    #[test]
    fn prg_outer_bank_from_ex_reg0_bits_1_2() {
        // exRegs[0] = 0x02 (bit 1 set): outer = (0x02 & 0x06) << 4 = 0x20
        // R6 = 3: bank = (3 & 0x1F) | 0x20 = 0x23 = 35
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x02);
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            35,
            "exRegs[0]=0x02, R6=3 must read bank 35"
        );
    }

    #[test]
    fn prg_inner_mask_controlled_by_ex_reg0_bit6() {
        // exRegs[0] = 0x40 (bit 6 set): inner mask shrinks to 0x0F
        // R6 = 0x15: 0x15 & 0x0F = 0x05, outer = 0x40 & 0x06 << 4 = 0...
        // Wait: exRegs[0] = 0x40, bits 1-2 = 0, bit 0 = 0.
        // (0x40 & 0x40) >> 6 = 1. 0x06 | 1 = 0x07. 0x40 & 0x07 = 0. outer_low = 0.
        // (0x40 & 0x10) << 3 = 0. outer = 0.
        // So bank = (0x15 & 0x0F) | 0 = 5.
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x40);
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 0x15);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "exRegs[0] bit 6 must shrink inner mask to 0x0F"
        );
    }

    #[test]
    fn prg_outer_bank_from_ex_reg0_bit4() {
        // exRegs[0] = 0x10 (bit 4 set): outer = (0x10 & 0x10) << 3 = 0x80
        // bits 1-2 = 0, bit 6 = 0 → inner mask = 0x1F
        // R6 = 0: bank = (0 & 0x1F) | 0x80 = 0x80 = 128
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x10);
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            128,
            "exRegs[0] bit 4 must contribute to outer bank at bit 7"
        );
    }

    // --- NROM modes ---

    #[test]
    fn nrom_128_mirrors_16k_across_32k() {
        // exRegs[3] bits 0-1 = 1 → NROM-128
        // R6 = 2, exRegs[0] = 0 → base = 2
        // Slots: $8000=2, $A000=3, $C000=2, $E000=3
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110); // R6, mode 0
        mapper.write_prg(0x8001, 2);
        write_ex_reg(&mut mapper, 3, 0x01); // NROM-128

        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 = base");
        assert_eq!(mapper.read_prg(0xA000), 3, "$A000 = base+1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 = base (mirrored)");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 = base+1 (mirrored)");
    }

    #[test]
    fn nrom_256_maps_32k_sequential() {
        // exRegs[3] bits 0-1 = 3 → NROM-256
        // R6 = 4, exRegs[0] = 0 → base = 4
        // Slots: $8000=4, $A000=5, $C000=6, $E000=7
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 4);
        write_ex_reg(&mut mapper, 3, 0x03); // NROM-256

        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 = base");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 = base+1");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 = base+2");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 = base+3");
    }

    #[test]
    fn nrom_mode_with_outer_bank() {
        // NROM-128, exRegs[0] = 0x02 → outer = 0x20
        // R6 = 5 → base = (5 & 0x1F) | 0x20 = 0x25 = 37
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x02);
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 5);
        write_ex_reg(&mut mapper, 3, 0x01); // NROM-128

        assert_eq!(mapper.read_prg(0x8000), 37, "$8000 = 37");
        assert_eq!(mapper.read_prg(0xA000), 38, "$A000 = 38");
        assert_eq!(mapper.read_prg(0xC000), 37, "$C000 = 37 (mirrored)");
    }

    // --- CHR banking ---

    #[test]
    fn chr_default_mmc3_banking() {
        // exRegs all zero: outer = 0, inner mask = 0xFFFF (no masking)
        // R2 = 5 → CHR bank at $1000 = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "Default CHR R2=5 must read bank 5"
        );
    }

    #[test]
    fn chr_outer_bank_from_ex_reg0_bit7_and_bit3() {
        // exRegs[0] = 0x88 selects CHR outer bank 0x80 and an inner mask of 0x7F.
        // With MMC3 R2 = 5, the mapped 1 KiB CHR bank at $1000 is
        // 0x80 | (5 & 0x7F) = 0x85 = 133.
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x88);
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "CHR outer bank 0x80 + inner 5 = bank 133"
        );
    }

    #[test]
    fn chr_inner_mask_limits_bank_when_bit7_set() {
        // exRegs[0] = 0x80 (bit 7 set, no other bits):
        // inner mask = 0x7F
        // chr_outer: !0x80 & 0x80 & exRegs[2] = 0 (bit 7 is set, so !reg bit 7 = 0)
        //            reg << 4 & 0x80 & reg = 0x800 & 0x80 = 0x80, & 0x80 = 0x80
        // Wait: (0x80 << 4) & 0x0080 = 0x800 & 0x80 = 0. Bit check: 0x800 binary is 0b100000000000.
        // 0x0080 binary is 0b10000000. AND = 0. So reg << 4 & 0x0080 = 0 for reg = 0x80.
        // All outer terms are 0. outer = 0.
        // inner mask = 0x7F
        // R2 = 0xFF: bank = 0 | (0xFF & 0x7F) = 0x7F = 127
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x80);
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(
            mapper.read_chr(0x1000),
            127,
            "CHR inner mask 0x7F must truncate 0xFF to 0x7F = 127"
        );
    }

    #[test]
    fn chr_linear_mode_uses_sequential_8k() {
        // exRegs[3] bit 4 = 1 → linear CHR mode
        // exRegs[0] = 0 (outer = 0), exRegs[2] = 0x02 (linear base = (2 & 0x0F) << 3 = 16)
        // 8 consecutive 1K banks: 16, 17, 18, ..., 23
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 2, 0x02);
        write_ex_reg(&mut mapper, 3, 0x10); // CHR linear mode

        assert_eq!(mapper.read_chr(0x0000), 16, "Linear CHR $0000 = bank 16");
        assert_eq!(mapper.read_chr(0x0400), 17, "Linear CHR $0400 = bank 17");
        assert_eq!(mapper.read_chr(0x1C00), 23, "Linear CHR $1C00 = bank 23");
    }

    // --- Register locking ---

    #[test]
    fn lock_bit_prevents_writes_to_ex_reg0_and_3() {
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x02); // set outer
        write_ex_reg(&mut mapper, 3, 0x80); // lock

        // Try to change exRegs[0] — should be blocked
        write_ex_reg(&mut mapper, 0, 0x00);
        // Try to change exRegs[3] — should be blocked
        write_ex_reg(&mut mapper, 3, 0x00);

        // exRegs[0] should still be 0x02 → outer = 0x20
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0x20,
            "exRegs[0] must remain 0x02 after lock (bank = 0 | 0x20 = 32)"
        );
    }

    #[test]
    fn lock_bit_does_not_block_ex_reg1_and_2() {
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 3, 0x10); // CHR linear mode, no lock
        write_ex_reg(&mut mapper, 3, 0x90); // lock + linear mode

        write_ex_reg(&mut mapper, 1, 0xFF); // should still succeed even when locked
        write_ex_reg(&mut mapper, 2, 0x03); // should still succeed even when locked
        assert_eq!(
            mapper.read_chr(0x0000),
            24,
            "exRegs[2] must be writable even when locked"
        );
    }

    // --- IRQ delegation ---

    #[test]
    fn irq_delegates_to_mmc3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 126");
    }

    // --- Save state ---

    #[test]
    fn snapshot_preserves_ex_regs() {
        let mut mapper = make_mapper();
        write_ex_reg(&mut mapper, 0, 0x12);
        write_ex_reg(&mut mapper, 1, 0x34);
        write_ex_reg(&mut mapper, 2, 0x56);
        write_ex_reg(&mut mapper, 3, 0x78);

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);
        let snap2 = mapper2.registers_snapshot();

        assert_eq!(snap, snap2, "Snapshot round-trip must preserve ex_regs");
    }
}
