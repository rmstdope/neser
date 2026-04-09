//! Mapper 269 – Games Xplosion 121-in-1 / 15000-in-1 / 18000-in-1
//!
//! Specification source: libretro-fceumm `src/boards/269.c`
//! (FCEUmm – GPLv2, Copyright (C) 2020)
//!
//! Hardware: MMC3-based multicart.  CHR data is synthesised from PRG ROM via
//! a bit-unscramble function; there is no separate CHR ROM section in the
//! iNES file.  Four outer-bank registers (`ex_regs[0..3]`) and a write-index
//! (`ex_reg_idx`) extend MMC3 PRG/CHR banking.
//!
//! ## External registers (`$5000–$5FFF`)
//!
//! Every write cycles through `ex_regs[0]`…`ex_regs[3]` in turn (the write
//! index wraps at 3).  Once `ex_regs[3] bit 7` is set the registers are locked
//! and no further writes are accepted.
//!
//! Power-on / Reset state: `ex_regs = [0, 0, 0x0F, 0]`, `ex_reg_idx = 0`.
//!
//! ## PRG banking (`M269PW`)
//!
//! ```text
//! MV  = inner_page & ((ex_regs[3] & 0x3F) ^ 0x3F)
//! MV |= ex_regs[1]
//! MV |= (ex_regs[3] & 0x40) << 2
//! ```
//!
//! Default (ex_regs = [0, 0, 0x0F, 0]):
//! - mask = `(0 & 0x3F) ^ 0x3F = 0x3F`  → inner bits 0-5 pass through.
//! - outer = 0  → pure MMC3 PRG banking for pages 0–63.
//!
//! ## CHR banking (`M269CW`)
//!
//! ```text
//! NV = inner_page
//! if ex_regs[2] & 0x08:
//!     NV &= (1 << ((ex_regs[2] & 7) + 1)) - 1
//! NV |= ex_regs[0] | ((ex_regs[2] & 0xF0) << 4)
//! ```
//!
//! Default (ex_regs[2] = 0x0F):
//! - Bit 3 set → mask = `(1 << 8) - 1 = 0xFF`  → NV &= 0xFF (no change).
//! - Outer bits = 0  → pure MMC3 CHR banking.
//!
//! ## CHR synthesis
//!
//! CHR ROM is derived from PRG ROM at load time by applying `unscramble_chr`
//! to every byte:
//!
//! | PRG bit | CHR bit |
//! |---------|---------|
//! | 0       | 6       |
//! | 1       | 4       |
//! | 2       | 2       |
//! | 3       | 0       |
//! | 4       | 1       |
//! | 5       | 3       |
//! | 6       | 5       |
//! | 7       | 7       |

use crate::nes::cartridge::Mapper;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;

/// Mapper 269 – Games Xplosion 121-in-1 / 15000-in-1 / 18000-in-1
pub struct Mapper269 {
    mmc3: MMC3Mapper,
    ex_regs: [u8; 4],
    ex_reg_idx: u8,
}

impl Mapper269 {
    const MAPPER_NUMBER: u16 = 269;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let chr = Self::unscramble_chr_from_prg(&ctx.prg_rom);
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, chr, ctx.mirroring, false),
            ex_regs: [0, 0, 0x0F, 0],
            ex_reg_idx: 0,
        }
    }

    /// Derives the synthetic CHR ROM from PRG ROM by unscrambling each byte.
    fn unscramble_chr_from_prg(prg: &[u8]) -> Vec<u8> {
        prg.iter().map(|&b| Self::unscramble_chr(b)).collect()
    }

    /// Unscramble one PRG byte into its CHR equivalent.
    ///
    /// Bit permutation: 0→6, 1→4, 2→2, 3→0, 4→1, 5→3, 6→5, 7→7.
    fn unscramble_chr(b: u8) -> u8 {
        let b = b as u16;
        (((b & 0x01) << 6)
            | ((b & 0x02) << 3)
            | ((b & 0x04) << 0)
            | ((b & 0x08) >> 3)
            | ((b & 0x10) >> 3)
            | ((b & 0x20) >> 2)
            | ((b & 0x40) >> 1)
            | ((b & 0x80) << 0)) as u8
    }

    /// Returns `true` when the external registers are locked (ex_regs[3] bit 7).
    fn is_locked(&self) -> bool {
        self.ex_regs[3] & 0x80 != 0
    }

    /// Apply outer PRG bank from external registers to an MMC3 inner page.
    ///
    /// `MV = (inner & mask) | ex_regs[1] | ((ex_regs[3] & 0x40) << 2)`
    /// where `mask = (ex_regs[3] & 0x3F) ^ 0x3F`.
    fn apply_prg_outer(&self, inner: usize) -> usize {
        let r1 = self.ex_regs[1] as usize;
        let r3 = self.ex_regs[3] as usize;
        let mask = (r3 & 0x3F) ^ 0x3F;
        (inner & mask) | r1 | ((r3 & 0x40) << 2)
    }

    /// Apply outer CHR bank from external registers to an MMC3 inner 1K page.
    fn apply_chr_outer(&self, inner: usize) -> usize {
        let r0 = self.ex_regs[0] as usize;
        let r2 = self.ex_regs[2] as usize;
        let mut nv = inner;
        if r2 & 0x08 != 0 {
            nv &= (1usize << ((r2 & 0x07) + 1)) - 1;
        }
        nv | r0 | ((r2 & 0xF0) << 4)
    }
}

impl Mapper for Mapper269 {
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
        let inner = self.mmc3.mapped_prg_bank(addr);
        let bank = self.apply_prg_outer(inner);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
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
            0x5000..=0x5FFF => {
                if !self.is_locked() {
                    self.ex_regs[self.ex_reg_idx as usize] = value;
                    self.ex_reg_idx = (self.ex_reg_idx + 1) & 3;
                }
            }
            _ => self.mmc3.write_prg(addr, value),
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let inner = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let inner = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner);
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
        snap.push(self.ex_reg_idx);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Extra 5 bytes: 4 ex_regs + 1 ex_reg_idx, appended after MMC3 state
        // (minimum MMC3 snapshot = 13 bytes).
        if data.len() >= 5 + 13 {
            let (mmc3_part, ex_part) = data.split_at(data.len() - 5);
            self.mmc3.restore_registers(mmc3_part);
            self.ex_regs.copy_from_slice(&ex_part[..4]);
            self.ex_reg_idx = ex_part[4] & 0x03;
        } else {
            self.mmc3.restore_registers(data);
            self.ex_regs = [0, 0, 0x0F, 0];
            self.ex_reg_idx = 0;
        }
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.ex_regs = [0, 0, 0x0F, 0];
        self.ex_reg_idx = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // PRG: 9 × 8 KiB banks (non-power-of-2 avoids modulo wrap false passes).
    // CHR: derived from PRG (same size), so 9×8 = 72 1KiB CHR banks.
    const PRG_8K_BANKS: usize = 9;

    // For clean per-1KiB CHR bank tests we need 1KiB-banked PRG data.
    const CHR_PRG_1K_BANKS: usize = 64;

    fn make_mapper() -> Mapper269 {
        Mapper269::new(MapperContext::new_for_test(
            269,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![], // mapper 269 ignores passed CHR; synthesises from PRG
            NametableLayout::Horizontal,
        ))
    }

    /// Mapper whose PRG is built from 1 KiB blocks so CHR bank `k` yields
    /// `unscramble_chr(k as u8)`.
    fn make_mapper_chr_test() -> Mapper269 {
        Mapper269::new(MapperContext::new_for_test(
            269,
            banked_data(1024, CHR_PRG_1K_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_269_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            269,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 269 must be registered in the factory"
        );
    }

    // -------------------------------------------------------------------------
    // Default state = standard MMC3 PRG banking
    // -------------------------------------------------------------------------

    #[test]
    fn default_state_prg_acts_like_mmc3() {
        let mut mapper = make_mapper();

        // R6 controls PRG at $8000; set it to inner page 3.
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);

        // Default ex_regs: mask = 0x3F, outer = 0 → bank = 3 & 0x3F = 3.
        // banked_data fills 8 KiB bank N with byte value N.
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // -------------------------------------------------------------------------
    // PRG outer banking via external registers
    // -------------------------------------------------------------------------

    #[test]
    fn prg_outer_or_from_exreg1() {
        // Write 0x20 to ex_regs[0] (first write → idx 0), then 0x20 to
        // ex_regs[1] (second write → idx 1).
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00); // ex_regs[0] = 0  (idx→1)
        mapper.write_prg(0x5000, 0x20); // ex_regs[1] = 0x20 (idx→2)

        // With inner page 0:
        //   mask = (0 & 0x3F) ^ 0x3F = 0x3F
        //   bank = (0 & 0x3F) | 0x20 | 0 = 0x20 = 32
        //   32 % 9 = 5
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        assert_eq!(mapper.read_prg(0x8000) as usize, 0x20 % PRG_8K_BANKS);
    }

    #[test]
    fn prg_inner_mask_from_exreg3_lower_bits() {
        // Write 0x00, 0x00, 0x0F, 0x01 to ex_regs[0..3] in sequence.
        // ex_regs[3] = 0x01 → mask = (0x01 & 0x3F) ^ 0x3F = 0x3E
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00); // ex_regs[0]=0, idx→1
        mapper.write_prg(0x5000, 0x00); // ex_regs[1]=0, idx→2
        mapper.write_prg(0x5000, 0x0F); // ex_regs[2]=0x0F, idx→3
        mapper.write_prg(0x5000, 0x01); // ex_regs[3]=0x01, idx→0

        // inner page 3 (binary 11):  3 & 0x3E = 2
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);

        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn prg_outer_high_bit_from_exreg3_bit6() {
        // ex_regs[3] = 0x40: outer high = (0x40 & 0x40) << 2 = 0x100
        // mask = (0x40 & 0x3F) ^ 0x3F = 0 ^ 0x3F = 0x3F
        // bank = (0 & 0x3F) | 0 | 0x100 = 0x100 = 256
        // 256 % 9 = 4
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00); // idx→1
        mapper.write_prg(0x5000, 0x00); // idx→2
        mapper.write_prg(0x5000, 0x0F); // idx→3
        mapper.write_prg(0x5000, 0x40); // ex_regs[3]=0x40, idx→0

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        assert_eq!(mapper.read_prg(0x8000) as usize, 0x100 % PRG_8K_BANKS);
    }

    // -------------------------------------------------------------------------
    // Sequential write index cycling
    // -------------------------------------------------------------------------

    #[test]
    fn sequential_writes_cycle_through_all_ex_regs() {
        let mut mapper = make_mapper();

        // Four writes fill ex_regs[0..3] in order.
        mapper.write_prg(0x5000, 0x01); // ex_regs[0]=0x01, idx→1
        mapper.write_prg(0x5000, 0x02); // ex_regs[1]=0x02, idx→2
        mapper.write_prg(0x5000, 0x0F); // ex_regs[2]=0x0F (keep CHR mask default), idx→3
        mapper.write_prg(0x5000, 0x04); // ex_regs[3]=0x04, idx→0

        // Verify ex_regs[1]=0x02 is in effect: outer OR → bank = inner | 0x02
        // inner=0 → bank=0x02 → 2 % 9 = 2
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.read_prg(0x8000) as usize, 0x02 % PRG_8K_BANKS);

        // Fifth write wraps back to ex_regs[0].
        mapper.write_prg(0x5000, 0x08); // ex_regs[0]=0x08, idx→1
        // ex_regs[1] still 0x02; bank = inner | 0x02 = 2 (unchanged)
        assert_eq!(mapper.read_prg(0x8000) as usize, 0x02 % PRG_8K_BANKS);
    }

    // -------------------------------------------------------------------------
    // Lock: ex_regs[3] bit 7
    // -------------------------------------------------------------------------

    #[test]
    fn lock_prevents_further_ex_reg_writes() {
        let mut mapper = make_mapper();

        // Write outer base 0x20 to ex_regs[1].
        mapper.write_prg(0x5000, 0x00); // ex_regs[0]=0, idx→1
        mapper.write_prg(0x5000, 0x20); // ex_regs[1]=0x20, idx→2

        // Write lock: 0x80 to ex_regs[3] by cycling through idx 2→3.
        mapper.write_prg(0x5000, 0x0F); // ex_regs[2]=0x0F, idx→3
        mapper.write_prg(0x5000, 0x80); // ex_regs[3]=0x80 (lock), idx→0

        // Attempt to clear ex_regs[1] – must be ignored.
        mapper.write_prg(0x5000, 0x00); // would go to ex_regs[0], ignored
        mapper.write_prg(0x5000, 0x00); // would go to ex_regs[1], ignored

        // ex_regs[1] should still be 0x20 → bank = inner | 0x20 = 0x20
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            0x20 % PRG_8K_BANKS,
            "External registers must stay locked at their pre-lock values"
        );
    }

    // -------------------------------------------------------------------------
    // Default state = standard MMC3 CHR banking
    // -------------------------------------------------------------------------

    #[test]
    fn default_state_chr_acts_like_mmc3() {
        let mut mapper = make_mapper_chr_test();

        // R0 controls $0000-$07FF (2 KiB).  Set to 4: maps 1K banks 4 and 5.
        mapper.write_prg(0x8000, 0x00); // bank_select → R0
        mapper.write_prg(0x8001, 4); // R0 = 4

        // Default ex_regs[2]=0x0F: mask=0xFF, outer=0 → bank = 4
        // CHR bank 4 (make_mapper_chr_test) = unscramble_chr(4)
        assert_eq!(mapper.read_chr(0x0000), Mapper269::unscramble_chr(4),);
    }

    // -------------------------------------------------------------------------
    // CHR outer banking via ex_regs[0]
    // -------------------------------------------------------------------------

    #[test]
    fn chr_outer_or_from_exreg0() {
        let mut mapper = make_mapper_chr_test();

        // Write 0x08 to ex_regs[0] (first sequential write).
        mapper.write_prg(0x5000, 0x08); // ex_regs[0]=0x08, idx→1

        // R0 = 0 → inner page 0; apply_chr_outer: NV=0 | 0x08 = 8
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0);

        // CHR bank 8 = unscramble_chr(8)
        assert_eq!(
            mapper.read_chr(0x0000),
            Mapper269::unscramble_chr(8),
            "CHR outer OR from ex_regs[0] must select bank 8"
        );
    }

    #[test]
    fn chr_mask_from_exreg2_bit3() {
        let mut mapper = make_mapper_chr_test();

        // ex_regs[2] = 0x08 (bit3=1, lower3=0):
        //   mask = (1 << (0 + 1)) - 1 = 1 (only bit 0 passes through)
        //
        // MMC3 R0 controls $0000-$07FF as a 2K block.  The 1K inner page at
        // $0000 is `(R0 & 0xFE)` (even-aligned) and at $0400 is `(R0 & 0xFE)|1`
        // (odd).  With R0=2:
        //   $0000 → inner = 2 → 2 & 1 = 0 → bank 0
        //   $0400 → inner = 3 → 3 & 1 = 1 → bank 1 = unscramble_chr(1)
        //
        // Without the mask (ex_regs[2]=0x0F default): bank at $0400 = 3.
        mapper.write_prg(0x5000, 0x00); // ex_regs[0]=0, idx→1
        mapper.write_prg(0x5000, 0x00); // ex_regs[1]=0, idx→2
        mapper.write_prg(0x5000, 0x08); // ex_regs[2]=0x08, idx→3

        mapper.write_prg(0x8000, 0x00); // bank_select → R0
        mapper.write_prg(0x8001, 2); // R0=2 → 1K banks 2 ($0000) and 3 ($0400)

        // $0400 inner page = 3 → masked: 3 & 1 = 1 → CHR bank 1
        assert_eq!(
            mapper.read_chr(0x0400),
            Mapper269::unscramble_chr(1),
            "CHR mask from ex_regs[2] bit 3 must limit inner page bits"
        );
    }

    // -------------------------------------------------------------------------
    // CHR data is unscrambled PRG
    // -------------------------------------------------------------------------

    #[test]
    fn chr_data_is_unscrambled_prg() {
        // Single 8 KiB PRG bank filled with a test pattern.
        let prg: Vec<u8> = (0u8..=255)
            .collect::<Vec<_>>()
            .into_iter()
            .cycle()
            .take(8192)
            .collect();
        let mut mapper = Mapper269::new(MapperContext::new_for_test(
            269,
            prg.clone(),
            vec![],
            NametableLayout::Horizontal,
        ));

        // Default R0=0 → CHR bank 0, offset 0 → prg[0] unscrambled
        // Check first few bytes
        for (i, &prg_byte) in prg.iter().take(16).enumerate() {
            let chr_addr = i as u16;
            let expected = Mapper269::unscramble_chr(prg_byte);
            assert_eq!(
                mapper.read_chr(chr_addr),
                expected,
                "CHR[{i}] must equal unscramble_chr(PRG[{i}])"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Reset restores power-on state
    // -------------------------------------------------------------------------

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();

        // Modify all external registers.
        mapper.write_prg(0x5000, 0xFF);
        mapper.write_prg(0x5000, 0xFF);
        mapper.write_prg(0x5000, 0xFF);
        // ex_regs[3] must be written without bit 7 to avoid locking before reset.
        mapper.write_prg(0x5000, 0x40);

        mapper.reset();

        // After reset, ex_regs[2] = 0x0F (CHR mask default).
        // PRG: inner=5 → bank = 5 & 0x3F | 0 = 5.
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "After reset, PRG banking must behave like pure MMC3 (ex_regs cleared)"
        );

        // CHR should also be back to default.
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            Mapper269::unscramble_chr(0),
            "After reset, CHR banking must behave like pure MMC3"
        );
    }

    // -------------------------------------------------------------------------
    // read_prg_open_bus: PRG-RAM at $6000–$7FFF
    // -------------------------------------------------------------------------

    #[test]
    fn read_prg_open_bus_returns_prg_ram_at_6000() {
        let mut mapper = make_mapper();
        let open_bus: u8 = 0xAB;

        // Write a known value to PRG-RAM via write_prg.
        mapper.write_prg(0x6000, 0x55);

        // read_prg_open_bus must return the written value, not open_bus.
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            0x55,
            "read_prg_open_bus($6000) must return PRG-RAM content, not open-bus"
        );
    }

    // -------------------------------------------------------------------------
    // Save-state: registers_snapshot round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_roundtrip() {
        let mut mapper = make_mapper();

        // Write a known state into external registers.
        mapper.write_prg(0x5000, 0x11); // ex_regs[0]=0x11
        mapper.write_prg(0x5000, 0x22); // ex_regs[1]=0x22
        mapper.write_prg(0x5000, 0x0F); // ex_regs[2]=0x0F
        mapper.write_prg(0x5000, 0x03); // ex_regs[3]=0x03, idx wraps to 0
        mapper.write_prg(0x5000, 0x33); // ex_regs[0]=0x33, idx→1  (second pass)

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // Both should agree on PRG banking.
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 1);
        mapper2.write_prg(0x8000, 0x06);
        mapper2.write_prg(0x8001, 1);

        assert_eq!(
            mapper.read_prg(0x8000),
            mapper2.read_prg(0x8000),
            "Restored mapper must have identical PRG banking to original"
        );
    }
}
