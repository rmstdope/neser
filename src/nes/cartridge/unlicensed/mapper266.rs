//! Mapper 266 – UNL-CITYFIGHT (City Fighter IV, VRC4-style clone)
//!
//! Specifications:
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_266>
//! - Reference implementation: Mesen2 `Core/NES/Mappers/Unlicensed/CityFighter.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/CityFighter.h>
//!
//! # Hardware overview
//!
//! Used by the unlicensed game "City Fighter IV". The board is a VRC4 variant
//! with non-standard address line wiring and a CPU-cycle-driven IRQ counter.
//!
//! The register mask for all addresses is `$F00C`.
//!
//! # PRG banking
//!
//! - PRG page size: 8 KiB.
//! - `$8000–$9FFF`, `$A000–$BFFF`: two switchable 8 KiB slots.
//! - `$C000–$DFFF`, `$E000–$FFFF`: the last two 8 KiB banks (fixed).
//!
//! **PRG register (`$9000`, mask `$F00C`):**
//! - bits 3:2 of value → select 32 KiB outer PRG block (inner bank bits cleared).
//!
//! **PRG mode (`$C000`, mask `$F00C`):**
//! - bit 0 of value selects which 8 KiB page is "active" for slot at `$8000–$9FFF`.
//!
//! # CHR banking
//!
//! - 8 banks of 1 KiB each, selected via low/high nibble registers.
//! - Registers use a nibble scheme: each CHR bank is set by two writes
//!   (low nibble first, then high nibble).
//! - Register addresses (mask `$F00C`):
//!
//! | Reg addr | Slot | Action |
//! |----------|------|--------|
//! | `$D000`  | 0    | low nibble  |
//! | `$D004`  | 0    | high nibble |
//! | `$D008`  | 1    | low nibble  |
//! | `$D00C`  | 1    | high nibble |
//! | `$A000`  | 2    | low nibble  |
//! | `$A004`  | 2    | high nibble |
//! | `$A008`  | 3    | low nibble  |
//! | `$A00C`  | 3    | high nibble |
//! | `$B000`  | 4    | low nibble  |
//! | `$B004`  | 4    | high nibble |
//! | `$B008`  | 5    | low nibble  |
//! | `$B00C`  | 5    | high nibble |
//! | `$E000`  | 6    | low nibble  |
//! | `$E004`  | 6    | high nibble |
//! | `$E008`  | 7    | low nibble  |
//! | `$E00C`  | 7    | high nibble |
//!
//! # Mirroring
//!
//! Controlled by bits 1:0 of the value written to `$9000` (mask `$F00C`):
//! - `0` = Vertical
//! - `1` = Horizontal
//! - `2` = Single-screen lower (A)
//! - `3` = Single-screen upper (B)
//!
//! # IRQ
//!
//! CPU-cycle-driven counter (counts every M2 cycle):
//! - **`$F000`**: Acknowledge + disable IRQ; write IRQ counter low nibble.
//! - **`$F004`**: Write IRQ counter high nibble.
//! - **`$F008`**: Control (bit 1: enable/disable counting; also acknowledges pending IRQ).
//!
//! The IRQ counter is loaded from low nibble (`$F000`) and high nibble
//! (`$F004`) fields, each taking 4 bits. When enabled via `$F008` bit 1,
//! the counter decrements on every CPU cycle; IRQ asserts when it reaches 0.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 266;
const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;

/// Mapper 266 – UNL-CITYFIGHT
pub struct Mapper266 {
    base: BaseMapper,
    /// 32 KiB outer PRG bank (from bits 3:2 of $9000 value, so 0/4/8/12)
    prg_reg: u8,
    /// PRG mode bit from $C000 (bit 0): selects which inner 8 KiB bank is at $8000.
    prg_mode: u8,
    /// 8 × 1 KiB CHR bank registers.
    chr_regs: [u8; 8],
    /// Nametable mirroring control: bits 1:0 from last write to $9000.
    mirroring_bits: u8,
    /// IRQ: enable flag.
    irq_enabled: bool,
    /// IRQ counter (16-bit, decremented each CPU cycle while enabled).
    irq_counter: u16,
    /// IRQ asserted flag.
    irq_asserted: bool,
}

impl Mapper266 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_reg: 0,
            prg_mode: 0,
            chr_regs: [0; 8],
            mirroring_bits: 0,
            irq_enabled: false,
            irq_counter: 0,
            irq_asserted: false,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        // The 8-KiB PRG slot layout:
        //  Slot 0 ($8000): prg_reg + prg_mode
        //  Slot 1 ($A000): prg_reg (bits 3:2 set bit 2 to select second bank of pair)
        //  Slot 2 ($C000): second-to-last bank
        //  Slot 3 ($E000): last bank (fixed)
        let outer = (self.prg_reg & 0x0C) as i16; // bits 3:2 → 0, 4, 8, 12
        self.base
            .select_prg_page(0, outer | (self.prg_mode & 0x01) as i16);
        self.base.select_prg_page(1, outer | 2);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        for (slot, &bank) in self.chr_regs.iter().enumerate() {
            self.base.select_chr_page(slot, bank as i16);
        }

        self.apply_mirroring(self.mirroring_bits);
    }

    fn apply_mirroring(&mut self, mirroring_bits: u8) {
        let layout = match mirroring_bits & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            _ => NametableLayout::SingleScreenUpper,
        };
        self.base.set_mirroring(layout);
    }
}

impl Mapper for Mapper266 {
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
        match addr & 0xF00C {
            // PRG register: bits 3:2 select 32 KiB outer block; bits 1:0 select mirroring
            0x9000 => {
                self.prg_reg = value & 0x0C;
                self.mirroring_bits = value & 0x03;
                self.apply_state();
            }
            // PRG mode variants (same register family with other bit patterns)
            0x9004 | 0x9008 | 0x900C => {
                self.prg_reg = value & 0x0C;
                self.apply_state();
            }
            // PRG mode register
            0xC000 | 0xC004 | 0xC008 | 0xC00C => {
                self.prg_mode = value & 0x01;
                self.apply_state();
            }
            // CHR registers (1 KiB banks, nibble-pair scheme)
            0xD000 => self.chr_regs[0] = (self.chr_regs[0] & 0xF0) | (value & 0x0F),
            0xD004 => self.chr_regs[0] = (self.chr_regs[0] & 0x0F) | (value << 4),
            0xD008 => self.chr_regs[1] = (self.chr_regs[1] & 0xF0) | (value & 0x0F),
            0xD00C => self.chr_regs[1] = (self.chr_regs[1] & 0x0F) | (value << 4),
            0xA000 => self.chr_regs[2] = (self.chr_regs[2] & 0xF0) | (value & 0x0F),
            0xA004 => self.chr_regs[2] = (self.chr_regs[2] & 0x0F) | (value << 4),
            0xA008 => self.chr_regs[3] = (self.chr_regs[3] & 0xF0) | (value & 0x0F),
            0xA00C => self.chr_regs[3] = (self.chr_regs[3] & 0x0F) | (value << 4),
            0xB000 => self.chr_regs[4] = (self.chr_regs[4] & 0xF0) | (value & 0x0F),
            0xB004 => self.chr_regs[4] = (self.chr_regs[4] & 0x0F) | (value << 4),
            0xB008 => self.chr_regs[5] = (self.chr_regs[5] & 0xF0) | (value & 0x0F),
            0xB00C => self.chr_regs[5] = (self.chr_regs[5] & 0x0F) | (value << 4),
            0xE000 => self.chr_regs[6] = (self.chr_regs[6] & 0xF0) | (value & 0x0F),
            0xE004 => self.chr_regs[6] = (self.chr_regs[6] & 0x0F) | (value << 4),
            0xE008 => self.chr_regs[7] = (self.chr_regs[7] & 0xF0) | (value & 0x0F),
            0xE00C => self.chr_regs[7] = (self.chr_regs[7] & 0x0F) | (value << 4),
            // IRQ registers
            0xF000 => {
                // Acknowledge: disable IRQ, deassert, reload low nibble of counter
                self.irq_asserted = false;
                self.irq_enabled = false;
                self.irq_counter = (self.irq_counter & 0x1E0) | ((value as u16 & 0x0F) << 1);
            }
            0xF004 => {
                // Reload high nibble of counter
                self.irq_counter = (self.irq_counter & 0x1E) | ((value as u16 & 0x0F) << 5);
            }
            0xF008 => {
                // Enable/disable IRQ; acknowledge pending IRQ
                self.irq_enabled = value & 0x02 != 0;
                self.irq_asserted = false;
            }
            _ => {}
        }
        // Re-apply CHR state after any CHR register change
        if matches!(
            addr & 0xF00C,
            0xD000
                | 0xD004
                | 0xD008
                | 0xD00C
                | 0xA000
                | 0xA004
                | 0xA008
                | 0xA00C
                | 0xB000
                | 0xB004
                | 0xB008
                | 0xB00C
                | 0xE000
                | 0xE004
                | 0xE008
                | 0xE00C
        ) {
            self.apply_state();
        }
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        self.irq_counter = self.irq_counter.wrapping_sub(1);
        if self.irq_counter == 0 {
            self.irq_asserted = true;
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
    }

    fn reset(&mut self) {
        self.prg_reg = 0;
        self.prg_mode = 0;
        self.chr_regs = [0; 8];
        self.mirroring_bits = 0;
        self.irq_enabled = false;
        self.irq_counter = 0;
        self.irq_asserted = false;
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let [irq_lo, irq_hi] = self.irq_counter.to_le_bytes();
        let flags = (self.irq_enabled as u8) | ((self.irq_asserted as u8) << 1);
        let mut snap = vec![
            self.prg_reg,
            self.prg_mode,
            irq_lo,
            irq_hi,
            flags,
            self.mirroring_bits,
        ];
        snap.extend_from_slice(&self.chr_regs);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 14 {
            return;
        }
        self.prg_reg = data[0] & 0x0C;
        self.prg_mode = data[1] & 0x01;
        self.irq_counter = u16::from_le_bytes([data[2], data[3]]);
        self.irq_enabled = data[4] & 0x01 != 0;
        self.irq_asserted = data[4] & 0x02 != 0;
        self.mirroring_bits = data[5] & 0x03;
        self.chr_regs.copy_from_slice(&data[6..14]);
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16; // 16 × 8 KiB = 128 KiB
    const CHR_BANKS: usize = 32; // 32 × 1 KiB = 32 KiB

    fn make_mapper() -> Mapper266 {
        Mapper266::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_266_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 266 must be registered in the factory"
        );
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_slot0_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "slot0 → bank 0 at power-on");
    }

    #[test]
    fn power_on_prg_slot1_is_bank_2() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "slot1 → bank 2 at power-on (outer|2)"
        );
    }

    #[test]
    fn power_on_prg_slots_2_and_3_are_fixed_last_two_banks() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "slot2 → second-to-last bank"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "slot3 → last bank"
        );
    }

    // ── PRG register ($9000) ──────────────────────────────────────────────────

    #[test]
    fn prg_reg_bits_3_2_select_32k_outer_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x04); // bits 3:2 = 01 → outer = 4
        assert_eq!(mapper.read_prg(0x8000), 4, "slot0 → bank 4");
        assert_eq!(mapper.read_prg(0xA000), 6, "slot1 → bank 4 | 2 = 6");
    }

    #[test]
    fn prg_reg_outer_8_maps_correct_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x08); // bits 3:2 = 10 → outer = 8
        assert_eq!(mapper.read_prg(0x8000), 8, "slot0 → bank 8");
        assert_eq!(mapper.read_prg(0xA000), 10, "slot1 → bank 10");
    }

    // ── PRG mode ($C000) ──────────────────────────────────────────────────────

    #[test]
    fn prg_mode_bit0_swaps_slot0_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x04); // outer = 4
        mapper.write_prg(0xC000, 0x01); // mode = 1 → slot0 = 4 | 1 = 5
        assert_eq!(mapper.read_prg(0x8000), 5, "mode=1 → slot0 = outer | 1 = 5");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_slot0_low_nibble_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x05); // slot 0 low nibble = 5
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR slot 0 → bank 5");
    }

    #[test]
    fn chr_slot0_high_nibble_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x05); // low nibble = 5
        mapper.write_prg(0xD004, 0x01); // high nibble = 1 → bank = 0x15 = 21
        assert_eq!(mapper.read_chr(0x0000), 21, "CHR slot 0 → bank 21");
    }

    #[test]
    fn chr_slot1_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD008, 0x03);
        assert_eq!(mapper.read_chr(0x0400), 3, "CHR slot 1 → bank 3");
    }

    #[test]
    fn chr_slot7_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE008, 0x07);
        assert_eq!(mapper.read_chr(0x1C00), 7, "CHR slot 7 → bank 7");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn prg_reg_bits_1_0_control_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "0 → Vertical"
        );
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "1 → Horizontal"
        );
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "2 → SingleScreenLower"
        );
        mapper.write_prg(0x9000, 0x03);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "3 → SingleScreenUpper"
        );
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ not asserted at power-on");
    }

    #[test]
    fn irq_asserts_when_counter_reaches_zero() {
        let mut mapper = make_mapper();
        // Load counter = 3 (low nibble=3, counter = 3 << 1 = 6)
        mapper.write_prg(0xF000, 0x03); // low nibble=3 → counter = 6, disabled
        mapper.write_prg(0xF008, 0x02); // enable IRQ
        // Tick 6 cycles
        for _ in 0..6 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ should be asserted after counter reaches 0"
        );
    }

    #[test]
    fn irq_acknowledge_clears_assertion() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x01);
        mapper.write_prg(0xF008, 0x02);
        for _ in 0..2 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xF000, 0x00); // acknowledge
        assert!(!mapper.irq_pending(), "IRQ cleared by acknowledge");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x08); // outer=8, H mirroring? No, bits 1:0=0 = Vertical
        mapper.write_prg(0x9000, 0x09); // outer=8, bits 1:0=1 = Horizontal
        mapper.write_prg(0xF008, 0x02);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 after reset");
        assert!(!mapper.irq_pending(), "no IRQ after reset");
        assert!(!mapper.irq_enabled, "IRQ disabled after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring reset to Vertical"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x05); // outer=4, mirroring=H (bits 1:0=1)
        mapper.write_prg(0xD000, 0x03);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "mirroring restored"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x08);
        mapper.restore_registers(&[0x00; 6]); // needs 14 bytes
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "state preserved on short restore"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "has IRQ");
        assert!(caps.has_chr_banking, "has CHR banking");
        assert!(caps.has_dynamic_mirroring, "has dynamic mirroring");
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
