//! Mapper 260 – BMC-HPXX (MMC3-variant multicart)
//!
//! Specifications:
//! - Primary source: Mesen2 `BmcHpxx.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Mmc3Variants/BmcHpxx.h>
//! - No dedicated NesDev wiki page found.
//!
//! Hardware overview:
//! - MMC3-based multicart board with extra outer-bank registers at `$5000–$5FFF`.
//! - PRG-ROM: up to 1 MiB; inner window is 8 KiB × 4 (MMC3-managed), outer block
//!   is controlled by `ex_regs[1]`.
//! - CHR-ROM: up to 4 MiB; inner window is 1 KiB × 8 (MMC3-managed), outer block
//!   is controlled by `ex_regs[2]`.
//! - Mirroring: Dynamic via MMC3 `$A000` in normal mode, or H/V forced by
//!   `ex_regs[4]` bit 2 in outer-locked mode.
//! - IRQ: Standard MMC3 scanline counter (A12 rising-edge).
//!
//! # Extra registers (`$5000–$5FFF`, indexed by `addr & 0x03`)
//!
//! ```text
//! ex_regs[0]:
//!   bit 7  – lock: any write with this bit set permanently locks ex_regs[0..3]
//!   bit 2  – outer-locked mode: PRG/CHR forced by ex_regs instead of MMC3
//!   bit 1  – (normal) PRG outer range: 1 → base = ex_regs[1] & 0x18 / mask = 0x0F
//!                                      0 → base = ex_regs[1] & 0x10 / mask = 0x1F
//!   bit 0  – (normal) CHR outer range: 1 → base = ex_regs[2] & 0x30 / mask = 0x7F
//!                                      0 → base = ex_regs[2] & 0x20 / mask = 0xFF
//! ex_regs[1] – PRG outer bank bits
//! ex_regs[2] – CHR outer bank bits
//! ex_regs[3] – unused (write slot 3 of the register array)
//! ex_regs[4] – updated by $8000–$FFFF writes in outer-locked mode;
//!              bit 2 → mirroring (1=Vertical, 0=Horizontal);
//!              bits 1:0 / bit 0 → CHR fine bank bits for locked CHR modes 3 / 2
//! ```
//!
//! # PRG banking (`$8000–$FFFF`)
//!
//! Normal mode (`ex_regs[0] & 0x04 == 0`):
//! - Effective 8 KiB bank = `(mmc3_bank & mask) | (base << 1)` where
//!   `base`/`mask` depend on `ex_regs[0]` bit 1.
//!
//! Outer-locked mode (`ex_regs[0] & 0x04 != 0`):
//! - If `(ex_regs[0] & 0x0F) == 0x04`: 16 KiB mirrored
//!   - `page = (ex_regs[1] & 0x1F) << 1`
//!   - slots 0 & 2 → `page`, slots 1 & 3 → `page + 1`
//! - Else: 32 KiB fixed
//!   - `page = (ex_regs[1] & 0x1E) << 1`
//!   - slot k → `page + k`
//!
//! # CHR banking (`$0000–$1FFF`)
//!
//! Normal mode:
//! - Effective 1 KiB bank = `(mmc3_1k_bank & mask) | (base << 3)` where
//!   `base`/`mask` depend on `ex_regs[0]` bit 0.
//!
//! Outer-locked mode:
//! - All 8 × 1 KiB slots fixed to `base | offset`
//! - `base` depends on `ex_regs[0] & 0x03`:
//!   - 0, 1 → `(ex_regs[2] & 0x3F) << 3`
//!   - 2    → `((ex_regs[2] & 0x3E) | (ex_regs[4] & 0x01)) << 3`
//!   - 3    → `((ex_regs[2] & 0x3C) | (ex_regs[4] & 0x03)) << 3`
//! - `offset = (addr & 0x1FFF) >> 10` (1 KiB slot 0–7)
//!
//! # Known Limitations
//! - DIP switch reads at `$5000–$5FFF` return 0 (4-switch DIP not emulated).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 260 – BMC-HPXX
pub struct Mapper260 {
    mmc3: MMC3Mapper,
    /// Extra registers:
    /// - `[0..3]`: written from `$5000–$5FFF` (latch via `addr & 0x03`)
    /// - `[4]`: updated by `$8000–$FFFF` writes in outer-locked mode
    ex_regs: [u8; 5],
    /// Once set, `ex_regs[0..3]` can no longer be modified.
    locked: bool,
}

impl Mapper260 {
    const MAPPER_NUMBER: u16 = 260;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mmc3 = MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false);
        Self {
            mmc3,
            ex_regs: [0; 5],
            locked: false,
        }
    }

    /// Compute the effective 8 KiB PRG bank index for a CPU address.
    fn mapped_prg_bank(&self, addr: u16) -> usize {
        let slot = ((addr as usize).saturating_sub(0x8000)) >> 13; // 0..3
        if self.ex_regs[0] & 0x04 != 0 {
            // Outer-locked mode
            if (self.ex_regs[0] & 0x0F) == 0x04 {
                // 16 KiB mirror: slots 0,2 → page; slots 1,3 → page+1
                let page = ((self.ex_regs[1] & 0x1F) as usize) << 1;
                page + (slot & 0x01)
            } else {
                // 32 KiB fixed: slot k → page+k
                let page = ((self.ex_regs[1] & 0x1E) as usize) << 1;
                page + slot
            }
        } else {
            // Normal MMC3 mode with outer bank masking
            let mmc3_bank = self.mmc3.mapped_prg_bank(addr);
            let (base, mask) = if self.ex_regs[0] & 0x02 != 0 {
                ((self.ex_regs[1] & 0x18) as usize, 0x0F_usize)
            } else {
                ((self.ex_regs[1] & 0x10) as usize, 0x1F_usize)
            };
            (mmc3_bank & mask) | (base << 1)
        }
    }

    /// Compute the effective 1 KiB CHR bank index for a PPU address.
    fn mapped_chr_1k_bank(&self, addr: u16) -> usize {
        let chr_1k_offset = ((addr & 0x1FFF) as usize) >> 10; // 0..7
        if self.ex_regs[0] & 0x04 != 0 {
            // Outer-locked mode: 8 KiB CHR fixed
            let base = match self.ex_regs[0] & 0x03 {
                0 | 1 => ((self.ex_regs[2] & 0x3F) as usize) << 3,
                2 => (((self.ex_regs[2] & 0x3E) | (self.ex_regs[4] & 0x01)) as usize) << 3,
                _ => (((self.ex_regs[2] & 0x3C) | (self.ex_regs[4] & 0x03)) as usize) << 3,
            };
            base | chr_1k_offset
        } else {
            // Normal MMC3 mode with outer CHR masking
            let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr);
            let (base, mask) = if self.ex_regs[0] & 0x01 != 0 {
                ((self.ex_regs[2] & 0x30) as usize, 0x7F_usize)
            } else {
                ((self.ex_regs[2] & 0x20) as usize, 0xFF_usize)
            };
            (mmc3_bank & mask) | (base << 3)
        }
    }
}

impl Mapper for Mapper260 {
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
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn get_mirroring(&self) -> NametableLayout {
        if self.ex_regs[0] & 0x04 != 0 {
            if self.ex_regs[4] & 0x04 != 0 {
                NametableLayout::Vertical
            } else {
                NametableLayout::Horizontal
            }
        } else {
            self.mmc3.base.mirroring()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x5FFF => 0, // DIP switches not emulated
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
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
            0x5000..=0x5FFF => 0,
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000..=0x5FFF => {
                if !self.locked {
                    self.ex_regs[(addr & 0x03) as usize] = value;
                    if value & 0x80 != 0 {
                        self.locked = true;
                    }
                }
            }
            0x6000..=0x7FFF => {
                self.mmc3.write_prg(addr, value);
            }
            0x8000..=0xFFFF => {
                if self.ex_regs[0] & 0x04 != 0 {
                    // In outer-locked mode, latch the value into ex_regs[4]
                    // while still forwarding the write to the underlying MMC3.
                    self.ex_regs[4] = value;
                }
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
        // Layout (version 0):
        // [ mmc3_snapshot..., ex_regs(5), locked(1), version(1), mmc3_len_le(2) ]
        let mmc3_data = self.mmc3.registers_snapshot();
        let mmc3_len = mmc3_data.len();

        // ex_regs(5) + locked(1) + version(1) + length(2)
        let overhead = self.ex_regs.len() + 1 + 1 + 2;
        let mut snap = Vec::with_capacity(mmc3_len + overhead);

        // MMC3 portion
        snap.extend_from_slice(&mmc3_data);
        // Mapper260-specific portion
        snap.extend_from_slice(&self.ex_regs);
        snap.push(u8::from(self.locked));
        // Mapper260 snapshot version
        snap.push(0); // version 0
        let mmc3_len_u16 = mmc3_len as u16;
        snap.push((mmc3_len_u16 & 0xFF) as u8);
        snap.push((mmc3_len_u16 >> 8) as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Expected layout (version 0):
        // [ mmc3_snapshot..., ex_regs(5), locked(1), version(1), mmc3_len_le(2) ]
        const EX_REGS_LEN: usize = 5;
        const LOCKED_LEN: usize = 1;
        const VERSION_LEN: usize = 1;
        const LEN_FIELD_LEN: usize = 2;
        const FOOTER_LEN: usize = VERSION_LEN + LEN_FIELD_LEN;
        const MIN_TOTAL_LEN: usize = EX_REGS_LEN + LOCKED_LEN + FOOTER_LEN;

        if data.len() < MIN_TOTAL_LEN {
            return;
        }

        let total = data.len();

        // Read MMC3 snapshot length (u16 LE) from the last two bytes.
        let len_lo = data[total - 2];
        let len_hi = data[total - 1];
        let mmc3_len = u16::from_le_bytes([len_lo, len_hi]) as usize;

        // Read and validate version byte.
        let version = data[total - 3];
        if version != 0 {
            // Unknown version; refuse to restore to avoid mis-parsing.
            return;
        }

        // Validate that lengths are self-consistent.
        let expected_min = mmc3_len
            .saturating_add(EX_REGS_LEN)
            .saturating_add(LOCKED_LEN)
            .saturating_add(FOOTER_LEN);
        if total < expected_min || mmc3_len > total.saturating_sub(FOOTER_LEN) {
            // Corrupt or inconsistent snapshot; bail out.
            return;
        }

        let ex_start = mmc3_len;
        let ex_end = ex_start + EX_REGS_LEN;
        let locked_idx = ex_end;

        if locked_idx + 1 > total - FOOTER_LEN {
            return;
        }

        self.ex_regs.copy_from_slice(&data[ex_start..ex_end]);
        self.locked = data[locked_idx] != 0;

        let mmc3_data = &data[..mmc3_len];
        self.mmc3.restore_registers(mmc3_data);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.ex_regs = [0; 5];
        self.locked = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    // ROM dimensions
    // PRG: 64 × 8 KiB = 512 KiB (banks 0..63)
    const PRG_BANKS: usize = 64;
    // CHR: 512 × 1 KiB = 512 KiB (banks 0..511)
    const CHR_BANKS: usize = 512;

    /// Build PRG-ROM where each 8 KiB bank `n` has byte 0 = `n & 0xFF`.
    fn make_prg_rom() -> Vec<u8> {
        let mut prg = vec![0u8; PRG_BANKS * 0x2000];
        for bank in 0..PRG_BANKS {
            prg[bank * 0x2000] = (bank & 0xFF) as u8;
        }
        prg
    }

    /// Build CHR-ROM where each 1 KiB bank `n` has byte 0 = `n & 0xFF`
    /// and byte 1 = `(n >> 8) & 0xFF`.
    fn make_chr_rom() -> Vec<u8> {
        let mut chr = vec![0u8; CHR_BANKS * 0x0400];
        for bank in 0..CHR_BANKS {
            chr[bank * 0x0400] = (bank & 0xFF) as u8;
            chr[bank * 0x0400 + 1] = ((bank >> 8) & 0xFF) as u8;
        }
        chr
    }

    fn create_mapper260() -> Mapper260 {
        Mapper260::new(MapperContext::new_for_test(
            260,
            make_prg_rom(),
            make_chr_rom(),
            NametableLayout::Horizontal,
        ))
    }

    // ────────────────────────────────────────────────────────────────────────
    // Factory registration
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn mapper_260_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            260,
            make_prg_rom(),
            make_chr_rom(),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 260 must be registered in the factory"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // PRG banking – normal mode (ex_regs[0] & 0x04 == 0)
    // ────────────────────────────────────────────────────────────────────────

    /// Default power-on state: ex_regs all zero, normal MMC3 mode.
    /// MMC3 maps $E000-$FFFF to the fixed last 8 KiB bank.
    /// With 64 banks and mask=0x1F: effective = (63 & 31) | 0 = 31.
    #[test]
    fn prg_normal_mode_default_state_e000_reads_bank31() {
        let mapper = create_mapper260();
        // Read byte 0 of the bank mapped to $E000
        let val = mapper.read_prg(0xE000);
        assert_eq!(val, 31, "$E000 in default state should read bank 31");
    }

    /// Normal mode with ex_regs[1] outer block: mask=0x1F, base=ex_regs[1] & 0x10.
    /// ex_regs[1] = 0x10 → base=16, base<<1=32.
    /// MMC3 $E000 fixed last bank (63), 63 & 0x1F = 31; effective = 31 | 32 = 63.
    #[test]
    fn prg_normal_mode_outer_32kb_block_shifts_mmc3_bank() {
        let mut mapper = create_mapper260();
        // Set ex_regs[1] = 0x10 via $5001
        mapper.write_prg(0x5001, 0x10);
        let val = mapper.read_prg(0xE000);
        assert_eq!(val, 63, "$E000 with ex_regs[1]=0x10 should read bank 63");
    }

    /// Normal mode, ex_regs[0] bit1=1 (16KB outer window).
    /// MMC3 R6=5 ($8000), base=ex_regs[1] & 0x18, mask=0x0F.
    /// ex_regs[1]=0x08 → base=0x08, base<<1=16; effective = (5 & 0x0F) | 16 = 21.
    #[test]
    fn prg_normal_mode_16kb_outer_mask_applies_base() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x02); // ex_regs[0] bit1=1
        mapper.write_prg(0x5001, 0x08); // ex_regs[1] = 0x08
        // Set MMC3 R6=5 → $8000-$9FFF
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        let val = mapper.read_prg(0x8000);
        assert_eq!(val, 21, "$8000 with outer-16k base=8 and R6=5 → bank 21");
    }

    /// Normal mode: MMC3 bank value above mask wraps via mask.
    /// ex_regs[0]=0x02 (bit1=1), ex_regs[1]=0x18 (base=0x18, mask=0x0F).
    /// R6=17 → 17 & 0x0F = 1; base<<1 = 48; effective = 49.
    #[test]
    fn prg_normal_mode_mmc3_bank_masked_by_outer_window() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x02);
        mapper.write_prg(0x5001, 0x18);
        mapper.write_prg(0x8000, 0x06); // bank_select → R6
        mapper.write_prg(0x8001, 17); // R6 = 17
        let val = mapper.read_prg(0x8000);
        // 17 & 0x0F = 1; 0x18 << 1 = 48; effective = 49
        assert_eq!(val, 49, "$8000 with R6=17, base=0x18 → bank 49");
    }

    // ────────────────────────────────────────────────────────────────────────
    // PRG banking – outer-locked mode (ex_regs[0] & 0x04 != 0)
    // ────────────────────────────────────────────────────────────────────────

    /// Locked mode with (ex_regs[0] & 0x0F) == 0x04: 16 KiB mirrored.
    /// ex_regs[1]=0x05 → page=(5 & 0x1F)<<1=10.
    /// $8000→bank10, $A000→bank11, $C000→bank10, $E000→bank11.
    #[test]
    fn prg_locked_16kb_mirror_maps_slots_symmetrically() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04); // ex_regs[0] = 0x04, locked mode, 16KB mirror
        mapper.write_prg(0x5001, 0x05); // ex_regs[1] = 0x05 → page=10
        assert_eq!(mapper.read_prg(0x8000), 10, "$8000 → bank 10");
        assert_eq!(mapper.read_prg(0xA000), 11, "$A000 → bank 11");
        assert_eq!(mapper.read_prg(0xC000), 10, "$C000 → bank 10 (mirror)");
        assert_eq!(mapper.read_prg(0xE000), 11, "$E000 → bank 11 (mirror)");
    }

    /// Locked mode with (ex_regs[0] & 0x0F) != 0x04: 32 KiB fixed.
    /// ex_regs[0]=0x06 (bit2=1, bit1=1), ex_regs[1]=0x0A.
    /// page=(0x0A & 0x1E)<<1 = 0x0A<<1 = 20.
    /// $8000→20, $A000→21, $C000→22, $E000→23.
    #[test]
    fn prg_locked_32kb_fixed_maps_four_sequential_banks() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x06); // ex_regs[0] = 0x06 → locked, 32KB mode
        mapper.write_prg(0x5001, 0x0A); // ex_regs[1] = 0x0A → page=20
        assert_eq!(mapper.read_prg(0x8000), 20, "$8000 → bank 20");
        assert_eq!(mapper.read_prg(0xA000), 21, "$A000 → bank 21");
        assert_eq!(mapper.read_prg(0xC000), 22, "$C000 → bank 22");
        assert_eq!(mapper.read_prg(0xE000), 23, "$E000 → bank 23");
    }

    // ────────────────────────────────────────────────────────────────────────
    // CHR banking – normal mode (ex_regs[0] & 0x04 == 0)
    // ────────────────────────────────────────────────────────────────────────

    /// Default normal mode: mask=0xFF, base=(ex_regs[2] & 0x20)=0.
    /// MMC3 default R0=0 → CHR $0000 = 1KB bank 0.
    #[test]
    fn chr_normal_mode_default_reads_mmc3_bank0() {
        let mut mapper = create_mapper260();
        let val = mapper.read_chr(0x0000);
        assert_eq!(val, 0, "CHR $0000 default = bank 0");
    }

    /// Normal mode with ex_regs[2] outer block: mask=0xFF, base=(ex_regs[2] & 0x20).
    /// ex_regs[2]=0x20 → base=32, base<<3=256.
    /// MMC3 R2=10 → CHR $1000 = 1KB bank 10.
    /// Effective = (10 & 0xFF) | 256 = 266 → byte1 of bank = 1.
    #[test]
    fn chr_normal_mode_outer_8kb_block_shifts_chr_bank() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5002, 0x20); // ex_regs[2] = 0x20 → base=32, base<<3=256
        // Set MMC3 R2=10 (CHR bank for $1000 in CHR mode 0)
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10);
        // bank 266: byte0 = 266 & 0xFF = 10, byte1 = 266 >> 8 = 1
        let byte0 = mapper.read_chr(0x1000);
        let byte1 = mapper.read_chr(0x1001);
        assert_eq!(
            byte0, 10,
            "CHR $1000 byte0 with outer=0x20 → bank 266, low byte=10"
        );
        assert_eq!(
            byte1, 1,
            "CHR $1000 byte1 with outer=0x20 → bank 266, high byte=1"
        );
    }

    /// Normal mode with ex_regs[0] bit0=1: mask=0x7F, base=(ex_regs[2] & 0x30).
    /// ex_regs[2]=0x30 → base=0x30=48, base<<3=384.
    /// MMC3 R2=10 → CHR $1000 = 1KB bank 10.
    /// Effective = (10 & 0x7F) | 384 = 394 → byte0=394&0xFF=138, byte1=394>>8=1.
    #[test]
    fn chr_normal_mode_4kb_outer_mask_with_bit0_set() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x01); // ex_regs[0] bit0=1 → 4KB outer range
        mapper.write_prg(0x5002, 0x30); // ex_regs[2] = 0x30 → base=48, base<<3=384
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 10);
        // bank 394: byte0 = 394 & 0xFF = 138, byte1 = 394 >> 8 = 1
        let byte0 = mapper.read_chr(0x1000);
        let byte1 = mapper.read_chr(0x1001);
        assert_eq!(byte0, 138, "CHR $1000 byte0 with outer-4kb mode → bank 394");
        assert_eq!(byte1, 1, "CHR $1000 byte1 with outer-4kb mode → bank 394");
    }

    // ────────────────────────────────────────────────────────────────────────
    // CHR banking – outer-locked mode (ex_regs[0] & 0x04 != 0)
    // ────────────────────────────────────────────────────────────────────────

    /// Locked mode case 0 (ex_regs[0] & 0x03 == 0): base=(ex_regs[2] & 0x3F)<<3.
    /// ex_regs[0]=0x04, ex_regs[2]=0x0A → base=0x0A<<3=80.
    /// CHR $0000 (offset 0) → bank 80; CHR $1C00 (offset 7) → bank 87.
    #[test]
    fn chr_locked_case0_8kb_fixed_window() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04); // locked, case 0
        mapper.write_prg(0x5002, 0x0A); // ex_regs[2]=0x0A
        assert_eq!(mapper.read_chr(0x0000), 80, "$0000 → CHR bank 80");
        assert_eq!(mapper.read_chr(0x1C00), 87, "$1C00 → CHR bank 87");
    }

    /// Locked mode case 1 (ex_regs[0] & 0x03 == 1): same formula as case 0.
    /// ex_regs[0]=0x05 → locked, case 1.
    #[test]
    fn chr_locked_case1_same_as_case0() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x05); // locked, case 1
        mapper.write_prg(0x5002, 0x0A); // base = 0x0A << 3 = 80
        assert_eq!(mapper.read_chr(0x0000), 80, "$0000 → CHR bank 80");
        assert_eq!(mapper.read_chr(0x1C00), 87, "$1C00 → CHR bank 87");
    }

    /// Locked mode case 2 (ex_regs[0] & 0x03 == 2):
    /// base = ((ex_regs[2] & 0x3E) | (ex_regs[4] & 0x01)) << 3.
    /// ex_regs[0]=0x06, ex_regs[2]=0x10, ex_regs[4]=0x01 (via $8000+ write).
    /// base = (0x10 | 0x01) << 3 = 0x11 << 3 = 136.
    #[test]
    fn chr_locked_case2_ex_reg4_bit0_adds_fine_chr_bit() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x06); // locked, case 2
        mapper.write_prg(0x5002, 0x10); // ex_regs[2]=0x10
        // In locked mode, write to $8000+ updates ex_regs[4]
        mapper.write_prg(0x8000, 0x01); // ex_regs[4]=0x01
        // base = (0x10 | 0x01) << 3 = 136
        assert_eq!(mapper.read_chr(0x0000), 136, "$0000 → CHR bank 136");
        assert_eq!(mapper.read_chr(0x1C00), 143, "$1C00 → CHR bank 143");
    }

    /// Locked mode case 2 without ex_regs[4] fine bit: base=(0x10 & 0x01=0)<<3=128.
    #[test]
    fn chr_locked_case2_without_fine_bit_uses_lower_base() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x06); // locked, case 2
        mapper.write_prg(0x5002, 0x10); // ex_regs[2]=0x10
        mapper.write_prg(0x8000, 0x00); // ex_regs[4]=0x00
        // base = 0x10 << 3 = 128
        assert_eq!(mapper.read_chr(0x0000), 128, "$0000 → CHR bank 128");
    }

    /// Locked mode case 3 (ex_regs[0] & 0x03 == 3):
    /// base = ((ex_regs[2] & 0x3C) | (ex_regs[4] & 0x03)) << 3.
    /// ex_regs[0]=0x07, ex_regs[2]=0x10, ex_regs[4]=0x03.
    /// base = (0x10 | 0x03) << 3 = 0x13 << 3 = 152.
    #[test]
    fn chr_locked_case3_ex_reg4_bits1_0_add_fine_chr_bits() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x07); // locked, case 3
        mapper.write_prg(0x5002, 0x10); // ex_regs[2]=0x10
        mapper.write_prg(0x8000, 0x03); // ex_regs[4]=0x03
        // base = (0x10 | 0x03) << 3 = 0x13 << 3 = 152
        assert_eq!(mapper.read_chr(0x0000), 152, "$0000 → CHR bank 152");
        assert_eq!(mapper.read_chr(0x1C00), 159, "$1C00 → CHR bank 159");
    }

    // ────────────────────────────────────────────────────────────────────────
    // Mirroring
    // ────────────────────────────────────────────────────────────────────────

    /// Normal mode: mirroring follows MMC3 $A000 writes.
    #[test]
    fn mirroring_normal_mode_delegates_to_mmc3() {
        let mut mapper = create_mapper260();
        // Power-on is Horizontal (from context)
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // MMC3 $A000 write: bit0=1 → Horizontal; bit0=0 → Vertical
        mapper.write_prg(0xA000, 0); // MMC3 mirroring → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 1); // MMC3 mirroring → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    /// Locked mode: mirroring is controlled by ex_regs[4] bit 2.
    /// ex_regs[4] & 0x04 != 0 → Vertical; == 0 → Horizontal.
    #[test]
    fn mirroring_locked_mode_vertical_when_ex_reg4_bit2_set() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04); // locked mode
        mapper.write_prg(0x8000, 0x04); // ex_regs[4] bit2=1 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_locked_mode_horizontal_when_ex_reg4_bit2_clear() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04); // locked mode
        mapper.write_prg(0x8000, 0x00); // ex_regs[4] bit2=0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ────────────────────────────────────────────────────────────────────────
    // Lock mechanism
    // ────────────────────────────────────────────────────────────────────────

    /// After a write with bit7 set, further writes to $5000-$5FFF are ignored.
    #[test]
    fn lock_bit_prevents_further_ex_reg_updates() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5001, 0x80); // sets ex_regs[1]=0x80 AND locks
        mapper.write_prg(0x5001, 0x10); // should be ignored
        mapper.write_prg(0x5002, 0x20); // should be ignored
        // ex_regs[1] stays 0x80 (base=0 for bits & 0x10 since bit4 of 0x80 = 0)
        // ex_regs[2] stays 0x00
        // In default mode: mask=0xFF, base=(0x00 & 0x20)=0 → normal MMC3
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Locked ex_regs[2] must remain 0"
        );
    }

    /// Without lock bit, writes update normally.
    #[test]
    fn without_lock_writes_update_ex_regs() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5001, 0x10); // ex_regs[1]=0x10 (no lock)
        mapper.write_prg(0x5001, 0x20); // overwrite to 0x20
        // ex_regs[1]=0x20 → for normal mode mask=0x1F: base=ex_regs[1]&0x10=0
        // base<<1=0, but $E000 still gets mmc3 bank (63 & 31 = 31) | 0 = 31
        let val = mapper.read_prg(0xE000);
        assert_eq!(
            val, 31,
            "ex_regs[1]=0x20 should not shift PRG base (bit4 clear)"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // IRQ – delegated to MMC3
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn irq_is_delegated_to_mmc3() {
        let mut mapper = create_mapper260();
        // Set IRQ latch=1 and enable
        mapper.write_prg(0xC000, 1); // IRQ latch
        mapper.write_prg(0xC001, 0); // IRQ reload
        mapper.write_prg(0xE001, 0); // IRQ enable
        // Trigger 2 A12 rising edges (need 3 CPU low cycles between each)
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ should fire");
    }

    // ────────────────────────────────────────────────────────────────────────
    // Save state / snapshot roundtrip
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_roundtrip_preserves_ex_regs_and_lock() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04); // ex_regs[0]=0x04 (locked mode)
        mapper.write_prg(0x5001, 0x05); // ex_regs[1]=0x05
        mapper.write_prg(0x5002, 0x0A); // ex_regs[2]=0x0A
        mapper.write_prg(0x8000, 0x00); // ex_regs[4]=0x00 (in locked mode)

        let snap = mapper.registers_snapshot();
        let mut restored = create_mapper260();
        restored.restore_registers(&snap);

        // Verify PRG: locked 16KB mirror, page=10
        assert_eq!(
            restored.read_prg(0x8000),
            10,
            "After restore: $8000 → bank 10"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            10,
            "After restore: $C000 → bank 10"
        );
        // Verify CHR: locked case 0, base=0x0A<<3=80
        assert_eq!(
            restored.read_chr(0x0000),
            80,
            "After restore: CHR $0000 → bank 80"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // Reset
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_ex_regs_and_lock() {
        let mut mapper = create_mapper260();
        mapper.write_prg(0x5000, 0x04);
        mapper.write_prg(0x5001, 0x05);
        mapper.write_prg(0x5002, 0x80); // also locks
        mapper.reset();
        // After reset, ex_regs are all 0 and unlocked
        assert_eq!(mapper.ex_regs, [0; 5]);
        assert!(!mapper.locked);
        // Writes should work again after reset
        mapper.write_prg(0x5001, 0x10);
        let val = mapper.read_prg(0xE000);
        // ex_regs[0]=0, ex_regs[1]=0x10, normal mode: base=0x10, base<<1=32
        // MMC3 last bank=63, 63&0x1F=31; 31|32=63
        assert_eq!(
            val, 63,
            "After reset and re-write, PRG should work correctly"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // Mapper number
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn mapper_number_is_260() {
        let mapper = create_mapper260();
        assert_eq!(mapper.mapper_number(), 260);
    }
}
