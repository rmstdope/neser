//! Mapper 176 – 8025 Enhanced MMC3
//!
//! # Specifications
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_176>
//! - Reference impl: Mesen2 (FK23C / FS005 mapper variants)
//!
//! ## Overview
//!
//! Mapper 176 (PCB name "8025") is an MMC3-derived chipset used by many
//! multicarts, Chinese single-game cartridges, and educational systems.
//! It adds *outer bank registers* in the `$5000–$5FFF` address range (and
//! `$4800` for submapper 5) that apply additional high-order bank bits on
//! top of the standard MMC3 bank-select values.
//!
//! Six submappers cover incompatible PCB variants:
//!
//! | # | PCB              | MAPR                  | PRG bits | Outer PRG | Outer CHR |
//! |---|------------------|-----------------------|----------|-----------|-----------|
//! | 0 | LP-8002KB/SFC-12B| BMC-Super24in1SC03    | 6 norm.  | $5xx1     | $5xx2     |
//! | 1 | FK-xxx/BS-xxx    | BMC-FK23C / FK23CA    | 8 norm.  | $5xx1     | $5xx2     |
//! | 2 | FS005/FS006      | WAIXING-FS005         | 6 swap.  | $5xx1     | $5xx2     |
//! | 3 | JX9003B          | —                     | 8 norm.  | $5xx1+5   | $5xx2+6   |
//! | 4 | ?                | —                     | 6 norm.  | $5xx1     | $5xx2     |
//! | 5 | HST-162          | —                     | 6 norm.  | $4800     | $5xx2     |
//!
//! Mapper **179** (Nestopia misassignment) is treated as an alias of submapper 0.
//!
//! ## Registers
//!
//! All `$5xxx` registers use address bits `[1:0]` to select the register
//! (address bits `[11:8]` are the solder-pad-variable "x").
//! Submapper 3 uses bits `[2:0]` instead.
//!
//! ### Mode Register (`$5xx0`)
//! ```text
//! D~7654 3210
//!   PCTm PMMM
//!   ||||  +++- PRG Banking Mode (in MMC3 mode)
//!   |||         0: 512 KiB outer window
//!   |||         1: 256 KiB outer window
//!   |||         2: 128 KiB outer window
//!   |||         3: NROM-128 (16 KiB @ $8000 mirrored)
//!   |||         4: NROM-256 (32 KiB @ $8000)
//!   |||         5: UNROM (16 KiB sw. @ $8000, fixed last @ $C000)
//!   ||+---- PRG A21 (submapper 2 only)
//!   |+----- Select CHR outer bank size (0=256KiB, 1=128KiB)
//!   +------ PRG A22 (submapper 2 only)
//! ```
//!
//! ### PRG Base Register LSB (`$5xx1`)
//! ```text
//! D~7654 3210
//!   .PPP PPPP = PRG A20..A14 (outer bank bits)
//! ```
//!
//! ### CHR Base Register LSB (`$5xx2`)
//! ```text
//! D~7654 3210
//!   ccCC CCCC = CHR A20..A13 outer bits
//! ```
//!
//! ### Extended Mode Register (`$5xx3`, submappers 1–2 only)
//! ```text
//! D~7654 3210
//!   .... ..E.  bit 1 = Extended MMC3 Mode (1=enable)
//! ```
//!
//! ### PRG Base MSB (`$5xx5`, submapper 3 only) — bits [3:0] = PRG A24..A21
//! ### CHR Base MSB (`$5xx6`, submapper 3 only) — bits [3:0] = CHR A24..A21
//!
//! ### PRG Base (`$4800`, submapper 5 only)
//! ```text
//! D~7654 3210
//!   ..PP PPPP = PRG A24..A19
//! ```
//!
//! ## PRG Banking
//!
//! With 6 outer PRG bits and Mode 0 (512 KiB window) — the most common case:
//! ```text
//! actual_bank = ((prg_base & 0x03) << 6) | (mmc3_inner_page & 0x3F)
//! ```
//! (For inner page `$FE`/`$FF`, the same formula gives the last two banks of
//! the outer 512 KiB window.)
//!
//! Mode 0 with 8 outer PRG bits (submappers 1 and 3):
//! ```text
//! Mode 0 with outer PRG bits above the 8-bit MMC3 selection (submappers 1 and 3):
//! ```text
//! actual_bank = outer_prg_bits | mmc3_inner_page
//! ```
//! Here `mmc3_inner_page` supplies the low 8 PRG bank bits, while `prg_base`
//! (and `prg_base_msb` on submapper 3) contribute higher bank-select bits above
//! the MMC3-selected portion.
//! In Extended MMC3 Mode (`$5xx3` bit 1 = 1) for submapper 1, the PRG base
//! register contributes bits above the single MMC3-controlled low bit:
//! ```text
//! actual_bank = ((prg_base & 0x7F) << 1) | (mmc3_inner_page & 0x01)
//! ```
//!
//! ## CHR Banking
//!
//! 256 KiB outer window (`$5xx0` bit 4 = 0):
//! ```text
//! actual_chr = ((chr_base & 0x07) << 8) | mmc3_raw_1k_bank
//! ```
//! 128 KiB outer window (`$5xx0` bit 4 = 1):
//! ```text
//! actual_chr = ((chr_base & 0x0F) << 7) | (mmc3_raw_1k_bank & 0x7F)
//! ```
//!
//! ## IRQ and Mirroring
//!
//! Standard MMC3 scanline IRQ and H/V mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperContext};
use crate::nes::cartridge::nintendo::mmc3::MMC3Mapper;
const PRG_BANK_MASK: usize = 0x1FFF; // 8 KiB - 1
const CHR_BANK_MASK: usize = 0x03FF; // 1 KiB - 1

/// Mapper 176 – 8025 Enhanced MMC3.
///
/// See the module-level documentation for hardware details.
pub struct Mapper176 {
    mmc3: MMC3Mapper,
    /// NES 2.0 submapper (0–5).
    submapper: u8,
    /// Mode register `$5xx0`.
    mode_reg: u8,
    /// PRG Base Register LSB `$5xx1` (and default for submapper 5 via `$4800`).
    prg_base: u8,
    /// CHR Base Register LSB `$5xx2`.
    chr_base: u8,
    /// Extended Mode Register `$5xx3` (submappers 1–2 only).
    ext_mode: u8,
    /// PRG Base MSB `$5xx5` (submapper 3 only).
    prg_base_msb: u8,
    /// CHR Base MSB `$5xx6` (submapper 3 only).
    chr_base_msb: u8,
    /// Mapper number (176 or 179 alias).
    mapper_number: u16,
}

impl Mapper176 {
    pub fn new(ctx: MapperContext) -> Self {
        let submapper = ctx.submapper;
        let mapper_number = ctx.mapper;
        let mmc3 = MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false);
        Self {
            mmc3,
            submapper,
            mode_reg: 0,
            prg_base: 0,
            chr_base: 0,
            ext_mode: 0,
            prg_base_msb: 0,
            chr_base_msb: 0,
            mapper_number,
        }
    }

    /// Returns true if the Extended MMC3 Mode is active (submappers 1–2 only).
    fn extended_mmc3_mode(&self) -> bool {
        matches!(self.submapper, 1 | 2) && (self.ext_mode & 0x02) != 0
    }

    /// Compute the actual 8 KiB PRG bank for a CPU address in `$8000–$FFFF`.
    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let inner = self.mmc3.raw_prg_8k_page_number(addr) as usize;
        let prg_mode = (self.mode_reg & 0x07) as usize;

        // Extended MMC3 Mode (submappers 1-2): outer PRG base shifts above all 8 inner bits.
        if self.extended_mmc3_mode() {
            let outer = (self.prg_base as usize & 0x7F) << 1;
            return outer | (inner & 0x01);
        }

        match self.submapper {
            // Submappers 0, 2, 4 — 6-bit outer PRG window
            0 | 2 | 4 => self.prg_bank_6outer(inner, prg_mode),
            // Submappers 1, 3 — 8-bit inner page from MMC3 (bank-select $8000.6/7 active)
            1 | 3 => {
                let msb = (self.prg_base_msb as usize & 0x0F) << 8;
                let base = (self.prg_base as usize & 0x7F) << 1;
                msb | base | inner
            }
            // Submapper 5 — PRG upper bits from $4800
            5 => {
                let outer = (self.prg_base as usize & 0x3F) << 6;
                outer | (inner & 0x3F)
            }
            _ => inner,
        }
    }

    /// 6-bit outer PRG bank for submappers 0/2/4, applying PRG mode.
    fn prg_bank_6outer(&self, inner: usize, prg_mode: usize) -> usize {
        match prg_mode {
            0 => {
                // 512 KiB outer window: 2 outer bits + 6 MMC3 bits
                ((self.prg_base as usize & 0x03) << 6) | (inner & 0x3F)
            }
            1 => {
                // 256 KiB outer window: 3 outer bits + 5 MMC3 bits
                ((self.prg_base as usize & 0x07) << 5) | (inner & 0x1F)
            }
            2 => {
                // 128 KiB outer window: 4 outer bits + 4 MMC3 bits
                ((self.prg_base as usize & 0x0F) << 4) | (inner & 0x0F)
            }
            3 => {
                // NROM-128: 16 KiB at $8000–$BFFF mirrored
                let base8 = (self.prg_base as usize & 0x3F) << 1;
                let slot = (inner & 0x03) & 0x01; // mirror: only 2 banks
                base8 | slot
            }
            4 => {
                // NROM-256: 32 KiB at $8000–$FFFF
                let base8 = (self.prg_base as usize & 0x3F) << 1;
                let slot = inner & 0x03;
                (base8 & 0xFC) | slot
            }
            5 => {
                // UNROM: 16 KiB switchable at $8000, inner bank #7 fixed at $C000
                // inner values for UNROM: regs[6] switchable, 0xFF fixed
                let base8 = (self.prg_base as usize & 0x3F) << 1;
                let slot = inner & 0x07;
                base8 | slot
            }
            _ => inner,
        }
    }

    /// Compute the actual 1 KiB CHR bank for a PPU address.
    fn chr_bank_for_addr(&self, addr: u16) -> usize {
        let inner = self.mmc3.raw_chr_1k_bank(addr);
        let small_chr = (self.mode_reg & 0x10) != 0; // bit 4: 1=128KiB, 0=256KiB

        let msb = (self.chr_base_msb as usize & 0x0F) << 21; // submapper 3 CHR A24..A21

        if small_chr {
            // 128 KiB mode: 4 outer bits + 7 MMC3 bits
            msb | ((self.chr_base as usize & 0x0F) << 7) | (inner & 0x7F)
        } else {
            // 256 KiB mode: 3 outer bits + 8 MMC3 bits
            msb | ((self.chr_base as usize & 0x07) << 8) | inner
        }
    }

    /// Handle a write to the outer-bank register range `$5000–$5FFF`.
    fn write_outer_register(&mut self, addr: u16, value: u8) {
        let reg = if self.submapper == 3 {
            addr & 7
        } else {
            addr & 3
        };
        match reg {
            0 => self.mode_reg = value,
            1 => self.prg_base = value & 0x7F,
            2 => self.chr_base = value,
            3 => self.ext_mode = value,
            5 if self.submapper == 3 => self.prg_base_msb = value & 0x0F,
            6 if self.submapper == 3 => self.chr_base_msb = value & 0x0F,
            _ => {}
        }
    }
}

impl Mapper for Mapper176 {
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
        self.mapper_number
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for_addr(addr);
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
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
            // Submapper 5: PRG upper bits at $4800
            0x4800 if self.submapper == 5 => {
                self.prg_base = value & 0x3F;
            }
            // Outer bank registers in $5000–$5FFF
            0x5000..=0x5FFF => {
                self.write_outer_register(addr, value);
            }
            // Forward everything else to MMC3
            _ => self.mmc3.write_prg(addr, value),
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.chr_bank_for_addr(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.extend_from_slice(&[
            self.mode_reg,
            self.prg_base,
            self.chr_base,
            self.ext_mode,
            self.prg_base_msb,
            self.chr_base_msb,
        ]);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let mmc3_snap_len = self.mmc3.registers_snapshot().len();
        if data.len() >= mmc3_snap_len {
            self.mmc3.restore_registers(&data[..mmc3_snap_len]);
        }
        if data.len() >= mmc3_snap_len + 6 {
            let ext = &data[mmc3_snap_len..];
            self.mode_reg = ext[0];
            self.prg_base = ext[1];
            self.chr_base = ext[2];
            self.ext_mode = ext[3];
            self.prg_base_msb = ext[4];
            self.chr_base_msb = ext[5];
        }
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.mode_reg = 0;
        self.prg_base = 0;
        self.chr_base = 0;
        self.ext_mode = 0;
        self.prg_base_msb = 0;
        self.chr_base_msb = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K: usize = 8 * 1024;
    const CHR_1K: usize = 1024;
    const MAPPER_NUMBER: u16 = 176;
    const MAPPER_179_ALIAS: u16 = 179;

    fn make_mapper_sub(submapper: u8) -> Mapper176 {
        Mapper176::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_8K, 128), // 1 MiB PRG
                banked_data(CHR_1K, 256), // 256 KiB CHR
                NametableLayout::Vertical,
            )
            .with_submapper(submapper),
        )
    }

    fn make_mapper() -> Mapper176 {
        make_mapper_sub(0)
    }

    #[test]
    fn mapper_176_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_8K, 128),
            banked_data(CHR_1K, 256),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 176 must be creatable via factory");
    }

    #[test]
    fn mapper_179_alias_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_179_ALIAS,
            banked_data(PRG_8K, 128),
            banked_data(CHR_1K, 256),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 179 alias must be creatable via factory"
        );
    }

    #[test]
    fn power_on_mmc3_state_is_default() {
        let m = make_mapper();
        assert_eq!(m.mode_reg, 0);
        assert_eq!(m.prg_base, 0);
        assert_eq!(m.chr_base, 0);
    }

    #[test]
    fn write_5xx0_sets_mode_register() {
        let mut m = make_mapper();
        m.write_prg(0x5000, 0x15);
        assert_eq!(m.mode_reg, 0x15);
        // Different solder-pad address also hits mode register
        m.write_prg(0x5300, 0x04);
        assert_eq!(m.mode_reg, 0x04);
    }

    #[test]
    fn write_5xx1_sets_prg_base() {
        let mut m = make_mapper();
        m.write_prg(0x5001, 0x03);
        assert_eq!(m.prg_base, 0x03);
    }

    #[test]
    fn write_5xx2_sets_chr_base() {
        let mut m = make_mapper();
        m.write_prg(0x5002, 0x07);
        assert_eq!(m.chr_base, 0x07);
    }

    #[test]
    fn outer_prg_base_selects_512kb_window_in_mode0() {
        let mut m = make_mapper_sub(0);
        // Mode 0 (default): outer window = 512 KiB (64 banks per outer slot)
        // Set prg_base = 1 → outer bits [1:0] = 01 → bank offset = 1 << 6 = 64
        m.write_prg(0x5001, 0x01); // prg_base = 1

        // Inner page for $8000 with default MMC3: bank register 6 = 0 → inner = 0
        // actual_bank = (1 & 0x03) << 6 | (0 & 0x3F) = 64
        let bank = m.prg_bank_for_addr(0x8000);
        assert_eq!(bank, 64, "Outer PRG base=1 must select bank 64 in Mode 0");
    }

    #[test]
    fn outer_prg_base_mode1_selects_256kb_window() {
        let mut m = make_mapper_sub(0);
        m.write_prg(0x5000, 0x01); // mode = 1 (256 KiB outer window)
        m.write_prg(0x5001, 0x02); // prg_base = 2

        // Mode 1: (2 & 0x07) << 5 | (inner & 0x1F) = 64 | 0 = 64
        let bank = m.prg_bank_for_addr(0x8000);
        assert_eq!(bank, 64, "PRG mode 1 with base=2 must give bank 64");
    }

    #[test]
    fn chr_base_selects_outer_chr_window_256kb() {
        let mut m = make_mapper_sub(0);
        m.write_prg(0x5002, 0x01); // chr_base = 1 → outer = 1 << 8 = 256

        // Inner CHR bank for $0000 with default MMC3 = 0
        let bank = m.chr_bank_for_addr(0x0000);
        assert_eq!(bank, 256, "CHR base=1 must shift CHR bank by 256");
    }

    #[test]
    fn chr_base_selects_outer_chr_window_128kb() {
        let mut m = make_mapper_sub(0);
        m.write_prg(0x5000, 0x10); // mode_reg bit 4 = 1 → 128 KiB CHR mode
        m.write_prg(0x5002, 0x01); // chr_base = 1 → outer = 1 << 7 = 128

        let bank = m.chr_bank_for_addr(0x0000);
        assert_eq!(bank, 128, "CHR base=1 in 128 KiB mode must give bank 128");
    }

    #[test]
    fn mmc3_irq_control_works_through_wrapper() {
        let mut m = make_mapper();
        // Standard MMC3 IRQ control at $C000/$C001/$E000/$E001
        m.write_prg(0xC000, 10); // set IRQ counter to 10
        m.write_prg(0xC001, 0); // reload
        m.write_prg(0xE000, 0); // enable IRQ (clear disable)
        // No immediate IRQ; just verify no panic
        assert!(!m.irq_pending());
    }

    #[test]
    fn mmc3_bank_select_works_through_wrapper() {
        let mut m = make_mapper_sub(0);
        // Select PRG register 6
        m.write_prg(0x8000, 0x06);
        // Set inner bank 5
        m.write_prg(0x8001, 5);

        // With prg_base=0 and inner=5, actual bank = 5
        let bank = m.prg_bank_for_addr(0x8000);
        assert_eq!(bank, 5);
    }

    #[test]
    fn submapper3_uses_7bit_address_decode() {
        let mut m = make_mapper_sub(3);
        // $5xx5 = PRG base MSB (submapper 3 only)
        m.write_prg(0x5005, 0x0F);
        assert_eq!(m.prg_base_msb, 0x0F);
        // $5xx6 = CHR base MSB (submapper 3 only)
        m.write_prg(0x5006, 0x07);
        assert_eq!(m.chr_base_msb, 0x07);
    }

    #[test]
    fn submapper5_4800_sets_prg_base() {
        let mut m = make_mapper_sub(5);
        m.write_prg(0x4800, 0x3F);
        assert_eq!(m.prg_base, 0x3F);
    }

    #[test]
    fn ext_mode_written_via_5xx3() {
        let mut m = make_mapper_sub(1);
        m.write_prg(0x5003, 0x02); // extended MMC3 mode enable
        assert_eq!(m.ext_mode, 0x02);
        assert!(m.extended_mmc3_mode());
    }

    #[test]
    fn reset_clears_outer_registers() {
        let mut m = make_mapper();
        m.write_prg(0x5001, 0x3F);
        m.write_prg(0x5002, 0x07);
        m.reset();
        assert_eq!(m.prg_base, 0);
        assert_eq!(m.chr_base, 0);
        assert_eq!(m.mode_reg, 0);
    }

    #[test]
    fn snapshot_restore_round_trips_outer_registers() {
        let mut m = make_mapper();
        m.write_prg(0x5000, 0x05);
        m.write_prg(0x5001, 0x02);
        m.write_prg(0x5002, 0x06);
        let snap = m.registers_snapshot();

        let mut m2 = make_mapper();
        m2.restore_registers(&snap);

        assert_eq!(m2.mode_reg, m.mode_reg);
        assert_eq!(m2.prg_base, m.prg_base);
        assert_eq!(m2.chr_base, m.chr_base);
    }
}
