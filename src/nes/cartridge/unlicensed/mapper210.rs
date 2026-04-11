//! Mapper 210 – Namco 175 (submapper 1) and Namco 340 (submapper 2)
//!
//! # Specifications
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_210>
//!
//! ## Overview
//!
//! Mapper 210 houses two related Namco ASICs that are cost-reduced variants of
//! the Namco 163 (mapper 19), retaining the basic PRG/CHR banking but removing
//! the IRQ counter, expansion audio, ROM nametables, and (for N340) PRG-RAM.
//!
//! | Feature     | N163 | N175 (sub 1) | N340 (sub 2) |
//! |-------------|------|--------------|--------------|
//! | IRQ         | Yes  | No           | No           |
//! | ROM nametables | Yes | No          | No           |
//! | PRG-RAM     | Yes  | Optional     | No           |
//! | Exp. audio  | Yes  | No           | No           |
//! | Mirroring   | Ext. | Hardwired H/V | H/V/0/1 sw  |
//!
//! ## Memory Map
//!
//! - `CPU $6000–$7FFF`: 8 KiB PRG-RAM (Namco 175 only; enabled by `$C000` bit 0)
//! - `CPU $8000–$9FFF`: 8 KiB switchable PRG-ROM bank (PRG Select 1)
//! - `CPU $A000–$BFFF`: 8 KiB switchable PRG-ROM bank (PRG Select 2)
//! - `CPU $C000–$DFFF`: 8 KiB switchable PRG-ROM bank (PRG Select 3)
//! - `CPU $E000–$FFFF`: 8 KiB PRG-ROM bank, fixed to the last bank
//! - `PPU $0000–$03FF`: 1 KiB switchable CHR bank (CHR reg 0)
//! - `PPU $0400–$07FF`: 1 KiB switchable CHR bank (CHR reg 1)
//! - `PPU $0800–$0BFF`: 1 KiB switchable CHR bank (CHR reg 2)
//! - `PPU $0C00–$0FFF`: 1 KiB switchable CHR bank (CHR reg 3)
//! - `PPU $1000–$13FF`: 1 KiB switchable CHR bank (CHR reg 4)
//! - `PPU $1400–$17FF`: 1 KiB switchable CHR bank (CHR reg 5)
//! - `PPU $1800–$1BFF`: 1 KiB switchable CHR bank (CHR reg 6)
//! - `PPU $1C00–$1FFF`: 1 KiB switchable CHR bank (CHR reg 7)
//!
//! ## Registers
//!
//! Each register spans a 2 KiB window (`$800` bytes).
//!
//! ### CHR Select (`$8000–$BFFF`) — 8 registers
//!
//! ```text
//! D~7654 3210
//!   CCCC CCCC   Select 1 KiB CHR-ROM page
//! ```
//! Address range to CHR slot mapping:
//! - `$8000–$87FF` → slot 0 (`PPU $0000–$03FF`)
//! - `$8800–$8FFF` → slot 1 (`PPU $0400–$07FF`)
//! - `$9000–$97FF` → slot 2 (`PPU $0800–$0BFF`)
//! - `$9800–$9FFF` → slot 3 (`PPU $0C00–$0FFF`)
//! - `$A000–$A7FF` → slot 4 (`PPU $1000–$13FF`)
//! - `$A800–$AFFF` → slot 5 (`PPU $1400–$17FF`)
//! - `$B000–$B7FF` → slot 6 (`PPU $1800–$1BFF`)
//! - `$B800–$BFFF` → slot 7 (`PPU $1C00–$1FFF`)
//!
//! ### PRG-RAM Enable (`$C000–$C7FF`) — Namco 175 only
//!
//! ```text
//! D~7654 3210
//!   .... ...E   1: Enable PRG-RAM at $6000–$7FFF
//! ```
//!
//! ### PRG Select 1 (`$E000–$E7FF`)
//!
//! ```text
//! D~7654 3210
//!   MMPP PPPP   (Namco 340 only: MM = mirroring bits [7:6])
//!   ..PP PPPP   (Namco 175)
//! ```
//! Selects 8 KiB PRG-ROM bank for `$8000–$9FFF`.  
//! For **Namco 340**: bits [7:6] select nametable mirroring.
//!
//! ### PRG Select 2 (`$E800–$EFFF`)
//!
//! ```text
//! D~7654 3210
//!   ..PP PPPP   Select 8 KiB PRG-ROM bank for $A000–$BFFF
//! ```
//!
//! ### PRG Select 3 (`$F000–$F7FF`)
//!
//! ```text
//! D~7654 3210
//!   ..PP PPPP   Select 8 KiB PRG-ROM bank for $C000–$DFFF
//! ```
//!
//! ## Mirroring
//!
//! - **Namco 175** (submapper 1): hardwired from cartridge header (H or V only)
//! - **Namco 340** (submapper 2): bits [7:6] of PRG Select 1 (`$E000`) register:
//!   - `0b00` → SingleScreen A (lower)
//!   - `0b01` → Vertical
//!   - `0b10` → SingleScreen B (upper)
//!   - `0b11` → Horizontal
//!
//! ## Power-on State
//!
//! PRG banks 0/0/0, CHR banks 0, PRG-RAM disabled (Namco 175).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::PrgRam;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 210;
const PRG_8K_BANK_SIZE: usize = 8 * 1024;
const CHR_1K_BANK_SIZE: usize = 1024;
const PRG_RAM_SIZE: usize = 8 * 1024;

/// Mapper 210 – Namco 175 (submapper 1) / Namco 340 (submapper 2).
///
/// See the module-level documentation for hardware details.
pub struct Mapper210 {
    base: BaseMapper,
    /// CHR bank registers, one per 1 KiB PPU slot (slots 0–7).
    chr_regs: [u8; 8],
    /// PRG bank registers for $8000/$A000/$C000 windows (indices 0/1/2).
    prg_regs: [u8; 3],
    /// `true` for Namco 175 (submapper 1): optional PRG-RAM, hardwired mirroring.
    /// `false` for Namco 340 (submapper 2): no PRG-RAM, software mirroring.
    is_namco175: bool,
    /// PRG-RAM (Namco 175 only), 8 KiB.
    prg_ram: PrgRam,
    /// PRG-RAM enabled flag (Namco 175 only).
    prg_ram_enabled: bool,
    /// Initial mirroring from cartridge header (Namco 175 uses this; Namco 340 ignores it).
    initial_mirroring: NametableLayout,
    /// Current Namco 340 mirroring bits ($E000[7:6]).
    /// Stored so `registers_snapshot` can round-trip the mirroring state.
    mirroring_bits: u8,
}

impl Mapper210 {
    pub fn new(ctx: MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let submapper = ctx.submapper;
        let is_namco175 = submapper == 1;

        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: !is_namco175, // only 340 has software mirroring
            prg_bank_size_kb: PRG_8K_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_1K_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_8K_BANK_SIZE);
        base.configure_chr_banking(CHR_1K_BANK_SIZE);

        // Namco 175: use header mirroring (hardwired solder pad).
        // Namco 340: power-on state = $E000=0 → bits[7:6]=00 → SingleScreenLower.
        if is_namco175 {
            base.set_mirroring(mirroring);
        } else {
            base.set_mirroring(NametableLayout::SingleScreenLower);
        }

        let prg_ram = if is_namco175 {
            PrgRam::new(PRG_RAM_SIZE)
        } else {
            PrgRam::new(0)
        };

        let mut mapper = Self {
            base,
            chr_regs: [0; 8],
            prg_regs: [0; 3],
            is_namco175,
            prg_ram,
            prg_ram_enabled: false,
            initial_mirroring: mirroring,
            mirroring_bits: 0,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        // PRG: three switchable 8KB + one fixed last
        self.base.select_prg_page(0, self.prg_regs[0] as i16);
        self.base.select_prg_page(1, self.prg_regs[1] as i16);
        self.base.select_prg_page(2, self.prg_regs[2] as i16);
        self.base.select_prg_page(3, -1); // fixed last bank at $E000

        // CHR: eight independent 1KB slots
        for (slot, &bank) in self.chr_regs.iter().enumerate() {
            self.base.select_chr_page(slot, bank as i16);
        }
    }

    /// Returns the 0-based CHR register index for a CPU address in `$8000–$BFFF`.
    fn chr_reg_index(addr: u16) -> usize {
        ((addr - 0x8000) / 0x800) as usize
    }

    fn set_mirroring_namco340(&mut self, bits: u8) {
        let layout = match bits & 3 {
            0 => NametableLayout::SingleScreenLower,
            1 => NametableLayout::Vertical,
            2 => NametableLayout::SingleScreenUpper,
            _ => NametableLayout::Horizontal,
        };
        self.base.set_mirroring(layout);
    }
}

impl Mapper for Mapper210 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000–$7FFF (Namco 175 only, when enabled)
        if self.is_namco175 && self.prg_ram_enabled && (0x6000..=0x7FFF).contains(&addr) {
            let offset = (addr - 0x6000) as usize;
            if offset < self.prg_ram.size() {
                return self.prg_ram.read_at_offset(offset);
            }
        }
        if addr >= 0x8000 {
            return self.base.read_prg_banked(addr);
        }
        0
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM writes (Namco 175 only, when enabled)
        if self.is_namco175 && self.prg_ram_enabled && (0x6000..=0x7FFF).contains(&addr) {
            let offset = (addr - 0x6000) as usize;
            if offset < self.prg_ram.size() {
                self.prg_ram.write_at_offset(offset, value);
            }
            return;
        }

        match addr {
            0x8000..=0xBFFF => {
                // CHR select: each $800-byte block maps to one 1KB CHR slot
                let idx = Self::chr_reg_index(addr);
                self.chr_regs[idx] = value;
                self.base.select_chr_page(idx, value as i16);
            }
            0xC000..=0xC7FF if self.is_namco175 => {
                // PRG-RAM enable (Namco 175 only)
                self.prg_ram_enabled = (value & 0x01) != 0;
            }
            0xE000..=0xE7FF => {
                // PRG Select 1 → $8000–$9FFF
                self.prg_regs[0] = value & 0x3F;
                if !self.is_namco175 {
                    // Namco 340: bits [7:6] = mirroring
                    self.mirroring_bits = value >> 6;
                    self.set_mirroring_namco340(self.mirroring_bits);
                }
                self.base.select_prg_page(0, self.prg_regs[0] as i16);
            }
            0xE800..=0xEFFF => {
                // PRG Select 2 → $A000–$BFFF
                self.prg_regs[1] = value & 0x3F;
                self.base.select_prg_page(1, self.prg_regs[1] as i16);
            }
            0xF000..=0xF7FF => {
                // PRG Select 3 → $C000–$DFFF
                self.prg_regs[2] = value & 0x3F;
                self.base.select_prg_page(2, self.prg_regs[2] as i16);
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + 3 + 2);
        data.extend_from_slice(&self.chr_regs);
        data.extend_from_slice(&self.prg_regs);
        data.push(self.prg_ram_enabled as u8);
        data.push(self.mirroring_bits);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 12 {
            self.chr_regs.copy_from_slice(&data[0..8]);
            self.prg_regs.copy_from_slice(&data[8..11]);
            self.prg_ram_enabled = data[11] != 0;
        }
        if data.len() >= 13 {
            self.mirroring_bits = data[12];
            if !self.is_namco175 {
                self.set_mirroring_namco340(self.mirroring_bits);
            }
        }
        self.apply_banks();
    }

    fn reset(&mut self) {
        self.chr_regs = [0; 8];
        self.prg_regs = [0; 3];
        self.prg_ram_enabled = false;
        self.mirroring_bits = 0;
        if self.is_namco175 {
            self.base.set_mirroring(self.initial_mirroring);
        } else {
            // Namco 340 power-on: $E000=0 → bits[7:6]=00 → SingleScreenLower
            self.base.set_mirroring(NametableLayout::SingleScreenLower);
        }
        self.apply_banks();
    }

    fn wram_size(&self) -> usize {
        if self.is_namco175 {
            self.prg_ram.size()
        } else {
            0
        }
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        if self.is_namco175 {
            self.prg_ram.snapshot()
        } else {
            Vec::new()
        }
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if self.is_namco175 {
            self.prg_ram.load_snapshot(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_namco340() -> Mapper210 {
        Mapper210::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_8K_BANK_SIZE, 8),
                banked_data(CHR_1K_BANK_SIZE, 32),
                NametableLayout::Vertical,
            )
            .with_submapper(2),
        )
    }

    fn make_namco175() -> Mapper210 {
        Mapper210::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_8K_BANK_SIZE, 8),
                banked_data(CHR_1K_BANK_SIZE, 32),
                NametableLayout::Horizontal,
            )
            .with_submapper(1),
        )
    }

    #[test]
    fn mapper_210_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_8K_BANK_SIZE, 8),
                banked_data(CHR_1K_BANK_SIZE, 32),
                NametableLayout::Vertical,
            )
            .with_submapper(2),
        );
        assert!(result.is_ok(), "Mapper 210 must be creatable via factory");
    }

    #[test]
    fn namco340_prg_select1_sets_bank_at_8000() {
        let mut m = make_namco340();
        m.write_prg(0xE000, 3); // bank 3 at $8000
        assert_eq!(m.prg_regs[0], 3);
    }

    #[test]
    fn namco340_prg_select2_sets_bank_at_a000() {
        let mut m = make_namco340();
        m.write_prg(0xE800, 5);
        assert_eq!(m.prg_regs[1], 5);
    }

    #[test]
    fn namco340_prg_select3_sets_bank_at_c000() {
        let mut m = make_namco340();
        m.write_prg(0xF000, 7);
        assert_eq!(m.prg_regs[2], 7);
    }

    #[test]
    fn namco340_power_on_mirroring_is_single_screen_lower() {
        let m = make_namco340();
        // Namco 340 defaults to $E000=0 (bits[7:6]=00 → SingleScreenLower).
        assert_eq!(m.base.mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn namco340_snapshot_preserves_mirroring_bits() {
        let mut m = make_namco340();
        m.write_prg(0xE000, 0b0100_0000); // Vertical mirroring
        let snap = m.registers_snapshot();

        let mut m2 = make_namco340();
        m2.restore_registers(&snap);
        assert_eq!(m2.base.mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn namco175_wram_size_returns_prg_ram_size() {
        let m = make_namco175();
        assert_eq!(m.wram_size(), PRG_RAM_SIZE);
    }

    #[test]
    fn namco340_e000_bits7_6_set_mirroring() {
        let mut m = make_namco340();

        m.write_prg(0xE000, 0b0000_0000); // bits[7:6] = 0 → SingleScreenLower
        assert_eq!(m.base.mirroring(), NametableLayout::SingleScreenLower);

        m.write_prg(0xE000, 0b0100_0000); // bits[7:6] = 1 → Vertical
        assert_eq!(m.base.mirroring(), NametableLayout::Vertical);

        m.write_prg(0xE000, 0b1000_0000); // bits[7:6] = 2 → SingleScreenUpper
        assert_eq!(m.base.mirroring(), NametableLayout::SingleScreenUpper);

        m.write_prg(0xE000, 0b1100_0000); // bits[7:6] = 3 → Horizontal
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn namco175_mirroring_is_hardwired_from_header() {
        let m = make_namco175();
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn namco175_e000_does_not_change_mirroring() {
        let mut m = make_namco175();
        m.write_prg(0xE000, 0b1100_0000); // bits[7:6] would change mirroring for N340
        // For N175 mirroring is hardwired; should not change
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn chr_select_8000_maps_slot0() {
        let mut m = make_namco340();
        m.write_prg(0x8000, 7); // slot 0 → bank 7
        assert_eq!(m.chr_regs[0], 7);
    }

    #[test]
    fn chr_select_b800_maps_slot7() {
        let mut m = make_namco340();
        m.write_prg(0xB800, 15); // slot 7 → bank 15
        assert_eq!(m.chr_regs[7], 15);
    }

    #[test]
    fn chr_register_boundaries_cover_all_8_slots() {
        let mut m = make_namco340();
        let bases = [
            0x8000u16, 0x8800, 0x9000, 0x9800, 0xA000, 0xA800, 0xB000, 0xB800,
        ];
        for (i, &base) in bases.iter().enumerate() {
            m.write_prg(base, i as u8);
            assert_eq!(
                m.chr_regs[i], i as u8,
                "CHR reg {i} must be set by ${base:04X}"
            );
        }
    }

    #[test]
    fn namco175_prg_ram_disabled_by_default() {
        let m = make_namco175();
        assert!(!m.prg_ram_enabled);
    }

    #[test]
    fn namco175_c000_enables_prg_ram() {
        let mut m = make_namco175();
        m.write_prg(0xC000, 0x01);
        assert!(m.prg_ram_enabled);
        m.write_prg(0xC000, 0x00);
        assert!(!m.prg_ram_enabled);
    }

    #[test]
    fn namco175_prg_ram_readable_when_enabled() {
        let mut m = make_namco175();
        m.write_prg(0xC000, 0x01); // enable
        m.write_prg(0x6000, 0xAB);
        assert_eq!(m.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn namco175_prg_ram_not_writable_when_disabled() {
        let mut m = make_namco175();
        // disabled by default; write should be ignored
        m.write_prg(0x6000, 0xAB);
        m.write_prg(0xC000, 0x01); // enable
        // Since the earlier write was ignored, value should be 0 (RAM initialized to 0)
        assert_eq!(m.read_prg(0x6000), 0x00);
    }

    #[test]
    fn namco340_has_no_prg_ram() {
        let mut m = make_namco340();
        m.write_prg(0x6000, 0xAB);
        // Namco 340 has no PRG-RAM, reads return 0 (open bus)
        let val = m.read_prg(0x6000);
        assert_eq!(val, 0, "Namco 340 must not expose PRG-RAM");
    }

    #[test]
    fn last_prg_bank_is_fixed() {
        let m = make_namco340();
        // banked_data fills each bank with its bank index as the first byte.
        // With 8 banks (0..=7), last bank = 7.
        let val = m.read_prg(0xE000); // $E000–$FFFF is fixed to last bank
        assert_eq!(val, 7, "Last PRG bank must be fixed");
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut m = make_namco340();
        m.write_prg(0xE000, 5);
        m.write_prg(0xB800, 10);
        m.reset();
        assert_eq!(m.prg_regs[0], 0);
        assert_eq!(m.chr_regs[7], 0);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut m = make_namco340();
        m.write_prg(0xE000, 3);
        m.write_prg(0x8000, 7);
        let snap = m.registers_snapshot();

        let mut m2 = make_namco340();
        m2.restore_registers(&snap);

        assert_eq!(m2.prg_regs[0], m.prg_regs[0]);
        assert_eq!(m2.chr_regs[0], m.chr_regs[0]);
    }
}
