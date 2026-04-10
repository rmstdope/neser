//! Mapper 162 – Waixing FS304
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_162>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Waixing/Waixing162.h`
//!
//! ## Overview
//!
//! Mapper 162 is used by Waixing educational cartridges. It provides a 32 KiB
//! switchable PRG-ROM window, 8 KiB unbanked CHR-RAM, battery-backed PRG-RAM,
//! and hardware-wired nametable mirroring.
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: 8 KiB unbanked PRG-RAM (battery-backed)
//! * `CPU $8000–$FFFF`: 32 KiB switchable PRG-ROM bank
//! * `PPU $0000–$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! ## Registers (CPU `$5000–$5FFF`, mask `$FF00`)
//!
//! Register index = `(addr >> 8) & 0x03`:
//!
//! ### Register 0 (`$5000`)
//!
//! ```text
//! 7654 3210
//! ---------
//! C... PPpq
//!        +- q: PRG A15 if mode bits (A=1, B=1)
//!       +-- p: PRG A16 if mode bit A=1
//!     ++--- P: PRG A18..A17
//! +-------- C: CHR auto-switch enable (not implemented; CHR is always page 0)
//! ```
//!
//! ### Register 1 (`$5100`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... ..p.
//!        +- p: PRG A15 source when mode B=0
//! ```
//!
//! ### Register 2 (`$5200`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... PPPP
//!      ++++- P: PRG A20..A17 (outer bank, replaces A18..A17 from reg0 in Mesen)
//! ```
//!
//! Wait — Mesen uses bits [3:0] of reg2 for A20..A17. NESdev says "PRG A20..A19"
//! for bits [1:0] of reg2.  Implementation follows Mesen's formula exactly.
//!
//! ### Register 3 (`$5300`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... .A.B
//!        +- B: PRG A15 mode
//!      +--- A: PRG A16 mode
//! ```
//!
//! ## PRG Bank Calculation (Mesen reference)
//!
//! Based on `reg3 & 0x05` (bits A=2, B=0):
//!
//! | `reg3 & 0x5` | Bank formula |
//! |---|---|
//! | 0 | `(reg0 & 0x0C) \| (reg1 & 0x02) \| ((reg2 & 0x0F) << 4)` |
//! | 1 | `(reg0 & 0x0C) \| ((reg2 & 0x0F) << 4)` |
//! | 4 | `(reg0 & 0x0E) \| ((reg1 >> 1) & 0x01) \| ((reg2 & 0x0F) << 4)` |
//! | 5 | `(reg0 & 0x0F) \| ((reg2 & 0x0F) << 4)` |
//!
//! ## Power-on / Reset State
//!
//! Mesen initializes with `reg0=3, reg3=7` (other regs = 0), giving boot bank 3.
//! NESdev states all registers are cleared to 0 on reset, which would give
//! boot bank 2.  This implementation follows Mesen for compatibility.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 162;
const PRG_BANK_SIZE: usize = 32 * 1024;

/// Mapper 162 – Waixing FS304 cartridge.
///
/// See the module-level documentation for hardware details.
pub struct Mapper162 {
    base: BaseMapper,
    regs: [u8; 4],
}

impl Mapper162 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut ctx = ctx;
        // Force CHR-RAM; mapper always uses internal CHR-RAM.
        ctx.chr_rom = vec![];
        if ctx.prg_ram_banks_8k == 0 {
            ctx.prg_ram_banks_8k = 1;
            ctx.prg_ram_size_specified = true;
        }

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);

        // Mesen power-on state: reg0=3, reg3=7.
        let regs = [3, 0, 0, 7];
        let mut mapper = Self { base, regs };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let r0 = self.regs[0] as usize;
        let r1 = self.regs[1] as usize;
        let r2 = self.regs[2] as usize;
        let r3 = self.regs[3] as usize;

        let bank = match r3 & 0x05 {
            0 => (r0 & 0x0C) | (r1 & 0x02) | ((r2 & 0x0F) << 4),
            1 => (r0 & 0x0C) | ((r2 & 0x0F) << 4),
            4 => (r0 & 0x0E) | ((r1 >> 1) & 0x01) | ((r2 & 0x0F) << 4),
            5 => (r0 & 0x0F) | ((r2 & 0x0F) << 4),
            _ => unreachable!(),
        };

        self.base.select_prg_page(0, bank as i16);
    }
}

impl Mapper for Mapper162 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x5000..=0x5FFF).contains(&addr) {
            let idx = ((addr >> 8) & 0x03) as usize;
            self.regs[idx] = value;
            self.update_banks();
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.regs.copy_from_slice(&data[..4]);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.regs = [3, 0, 0, 7];
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use 11 banks (non-power-of-two) to expose wrapping false positives.
    const PRG_BANKS: usize = 11;

    fn make_mapper() -> Mapper162 {
        Mapper162::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(8 * 1024, 1),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_162_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1),
        );
        assert!(
            result.is_ok(),
            "Mapper 162 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_boots_to_bank3() {
        // Mesen initial state: reg0=3, reg3=7 → case 5 → bank=(3 & 0x0F)|0 = 3
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), (3 % PRG_BANKS) as u8);
    }

    // ── Mode 5 (reg3 & 0x5 = 5): reg0 & 0x0F | outer ─────────────────────────

    #[test]
    fn mode5_bank_from_reg0_low_nibble() {
        let mut mapper = make_mapper();
        // reg3=7 → mode5; set reg0=5 → bank=(5 & 0x0F)|0 = 5
        mapper.write_prg(0x5000, 5);
        assert_eq!(mapper.read_prg(0x8000), (5 % PRG_BANKS) as u8);
    }

    #[test]
    fn mode5_outer_bits_from_reg2() {
        let mut mapper = make_mapper();
        // reg3=7 → mode5; reg0=0, reg2=1 → bank=0|(1<<4)=16
        mapper.write_prg(0x5000, 0);
        mapper.write_prg(0x5200, 1); // reg2=1 → outer = 1<<4 = 16
        assert_eq!(mapper.read_prg(0x8000), (16 % PRG_BANKS) as u8);
    }

    // ── Mode 0 (reg3 & 0x5 = 0): reg0[3:2] | reg1[1] | outer ─────────────────

    #[test]
    fn mode0_bank_from_reg0_bits2_3_and_reg1_bit1() {
        let mut mapper = make_mapper();
        // Set reg3=0 → mode0
        mapper.write_prg(0x5300, 0x00);
        // reg0=0x04 (bit2=1), reg1=0 → bank=(4 & 0x0C)|(0 & 0x02)|0 = 4
        mapper.write_prg(0x5000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), (4 % PRG_BANKS) as u8);
    }

    #[test]
    fn mode0_reg1_bit1_contributes_a15() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5300, 0x00);
        // reg0=0, reg1=0x02 (bit1=1) → bank=0|2|0 = 2
        mapper.write_prg(0x5100, 0x02);
        assert_eq!(mapper.read_prg(0x8000), (2 % PRG_BANKS) as u8);
    }

    // ── Mode 1 (reg3 & 0x5 = 1): reg0[3:2] | outer ───────────────────────────

    #[test]
    fn mode1_ignores_reg1() {
        let mut mapper = make_mapper();
        // reg3=1 → mode1
        mapper.write_prg(0x5300, 0x01);
        mapper.write_prg(0x5000, 0x04); // reg0 bit2=1 → bank = (4 & 0x0C)|0 = 4
        mapper.write_prg(0x5100, 0x02); // reg1 bit1 should be ignored
        assert_eq!(mapper.read_prg(0x8000), (4 % PRG_BANKS) as u8);
    }

    // ── Mode 4 (reg3 & 0x5 = 4): reg0[3:1] | reg1[1]>>1 | outer ─────────────

    #[test]
    fn mode4_reg1_shifted() {
        let mut mapper = make_mapper();
        // reg3=4 → mode4; also clear reg0 to isolate reg1 contribution
        mapper.write_prg(0x5300, 0x04);
        mapper.write_prg(0x5000, 0x00); // reg0=0 so no contribution from reg0
        // reg1=0x02 → (reg1>>1)&1 = 1 → bank=0|1|0 = 1
        mapper.write_prg(0x5100, 0x02);
        assert_eq!(mapper.read_prg(0x8000), (1 % PRG_BANKS) as u8);
    }

    // ── Register echo: $5400-$5FFF mirrors back to reg 0-3 ───────────────────

    #[test]
    fn register_echoes_in_5x00_range() {
        let mut mapper = make_mapper();
        // $5400: (0x5400 >> 8) & 0x03 = 0x54 & 0x03 = 0 → writes reg 0
        // reg3=7 (mode5); setting reg0=6 → bank=(6 & 0x0F)|0 = 6
        mapper.write_prg(0x5400, 6);
        assert_eq!(mapper.read_prg(0x8000), (6 % PRG_BANKS) as u8);
    }

    // ── PRG-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_readable_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    // ── CHR-RAM ────────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0200, 0xBE);
        assert_eq!(mapper.read_chr(0x0200), 0xBE);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_boot_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5300, 0x00); // change mode
        mapper.write_prg(0x5000, 0x07); // change bank
        mapper.reset();
        // Back to power-on: mode5, bank 3
        assert_eq!(mapper.read_prg(0x8000), (3 % PRG_BANKS) as u8);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5300, 0x04);
        mapper.write_prg(0x5000, 0x06);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }
}
