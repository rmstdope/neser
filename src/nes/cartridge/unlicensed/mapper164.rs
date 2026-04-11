//! Mapper 164 – Dongda PEC-9588 / cy2000-3
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_164.xhtml>
//!
//! ## Known Limitations
//! - 1 bpp video mode (bit C of $5000) is not implemented.
//!   This mode requires PPU-level modifications and is not yet supported.
//! - 93C66 serial EEPROM interface ($5200 write / $5500 read) is not
//!   emulated; EEPROM reads always return 0.
//!
//! ## Overview
//!
//! Mapper 164 is used by several games including Final Fantasy V and
//! Pokémon: Gold Edition on the Dongda PEC-9588 circuit board.
//!
//! ## PRG Banking
//!
//! Two registers control PRG banking:
//! - `$5000` (mask `$FF00`): bank low / mode switch
//! - `$5100` (mask `$FF00`): bank high (A20..A19)
//!
//! ### Register $5000 bit layout
//! ```text
//! D~7654 3210
//!   ---------
//!   CSQM PPPp
//!   ||+|-++++- PRG A18..A14 if M=0  (7-bit low bank with $5100 high bits)
//!   || | ++++- PRG A18..A15 if M=1  (32KB bank with $5100 high bits)
//!   || +------ PRG banking mode
//!   ||          0: UxROM - 16 KiB switchable lower bank at $8000-$BFFF
//!   ||             Fixed upper bank at $C000-$FFFF:
//!   ||               S=0 → A14..A18=11111 (bank 31 within $5100 group)
//!   ||               S=1 → A14..A18=111p0 (bank 28 or 30 within $5100 group)
//!   ||          1: BxROM - 32 KiB switchable bank at $8000-$FFFF
//!   ||         Also selects nametable mirroring:
//!   ||          0: Forced vertical mirroring
//!   ||          1: Mirroring selected by $5300
//!   |+-------- S: fixed upper bank selector (UxROM mode only)
//!   +--------- C: 1 bpp video mode (NOT IMPLEMENTED)
//! ```
//!
//! ### Register $5100 bit layout
//! ```text
//! D~7654 3210
//!   ---------
//!   .... ..PP
//!          ++- PRG A20..A19 (high bank bits)
//! ```
//!
//! ## Mirroring
//!
//! - $5000 bit 4 (M) = 0: Forced vertical mirroring
//! - $5000 bit 4 (M) = 1: Use $5300 bit 7
//!   - $5300 bit 7 = 0: Horizontal mirroring
//!   - $5300 bit 7 = 1: Vertical mirroring
//!
//! ## CHR Memory
//!
//! 8 KiB unbanked CHR-RAM.
//!
//! ## PRG-RAM
//!
//! Up to 8 KiB unbanked PRG-RAM at $6000-$7FFF (not battery-backed).
//! The actual size depends on what the iNES header specifies.
//!
//! ## Power-on / Reset State
//!
//! All registers initialize to $00 on reset:
//! - UxROM mode, bank 0 at $8000-$BFFF, bank 31 fixed at $C000-$FFFF
//! - Forced vertical mirroring

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 164;
const PRG_BANK_SIZE_16K: usize = 16 * 1024;

/// Mapper 164 – Dongda PEC-9588 / cy2000-3 circuit board.
///
/// See the module-level documentation for hardware details.
pub struct Mapper164 {
    base: BaseMapper,
    /// $5000 register value (write, mask $FF00).
    reg5000: u8,
    /// $5100 register value (write, mask $FF00) — bits [1:0] = A20..A19.
    reg5100: u8,
    /// $5300 register value (write, mask $FF00) — bit [7] = mirroring select.
    reg5300: u8,
}

impl Mapper164 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_16K);
        let mut mapper = Self {
            base,
            reg5000: 0,
            reg5100: 0,
            reg5300: 0,
        };
        mapper.update_banks();
        mapper
    }

    /// Recompute both PRG bank slots and mirroring from the current register state.
    fn update_banks(&mut self) {
        let high_bits = (self.reg5100 & 0x03) as i16; // A20..A19
        let mode_m = (self.reg5000 & 0x10) != 0; // bit 4: banking mode
        let bit_s = (self.reg5000 & 0x40) != 0; // bit 6: fixed bank S
        let bit_q = (self.reg5000 & 0x20) != 0; // bit 5: PRG A18 (UxROM mode)
        let bits_pppn = (self.reg5000 & 0x0F) as i16; // bits 3-0: PPPp
        let bit_p = (self.reg5000 & 0x01) != 0; // bit 0: p

        if mode_m {
            // BxROM: 32 KiB switchable — both 16 KB slots in consecutive banks.
            let bank32 = (high_bits << 4) | bits_pppn; // 6-bit 32KB bank index
            self.base.select_prg_page(0, bank32 * 2);
            self.base.select_prg_page(1, bank32 * 2 + 1);
        } else {
            // UxROM: 16 KiB switchable lower bank, semi-fixed upper bank.
            let q_bit: i16 = if bit_q { 0x10 } else { 0 };
            let low_bank = (high_bits << 5) | q_bit | bits_pppn;

            let high_bank = if bit_s {
                // A18..A14 = 111p0 → bank 28 or 30 within the $5100 group
                let p_bit: i16 = if bit_p { 2 } else { 0 };
                (high_bits << 5) | 0x1C | p_bit
            } else {
                // A18..A14 = 11111 → fixed last bank (31) in the $5100 group
                (high_bits << 5) | 0x1F
            };

            self.base.select_prg_page(0, low_bank);
            self.base.select_prg_page(1, high_bank);
        }

        // Mirroring.
        if mode_m {
            // Controlled by $5300 bit 7: 0=Horizontal, 1=Vertical
            self.base.set_mirroring_hv((self.reg5300 & 0x80) == 0);
        } else {
            // Forced vertical mirroring
            self.base.set_mirroring(NametableLayout::Vertical);
        }
    }
}

impl Mapper for Mapper164 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            // PRG-RAM at $6000-$7FFF.
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(open_bus),
            // $5500: EEPROM data input (inverted). Not implemented — return 0.
            a if a & 0xFF00 == 0x5500 => 0,
            a if a < 0x8000 => open_bus,
            _ => self.base.read_prg_banked(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Check PRG-RAM first ($6000-$7FFF).
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        // Registers are decoded by upper byte only (mask $FF00).
        match addr & 0xFF00 {
            0x5000 => {
                self.reg5000 = value;
                self.update_banks();
            }
            0x5100 => {
                self.reg5100 = value & 0x03;
                self.update_banks();
            }
            0x5200 => {
                // EEPROM write — not implemented, ignore.
            }
            0x5300 => {
                self.reg5300 = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg5000, self.reg5100, self.reg5300]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.reg5000 = data[0];
            self.reg5100 = data[1] & 0x03;
            self.reg5300 = data[2];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg5000 = 0;
        self.reg5100 = 0;
        self.reg5300 = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANK_SIZE: usize = PRG_BANK_SIZE_16K;

    /// Create a mapper with 32 PRG banks (512 KiB) and 8 KiB CHR-RAM.
    fn make_mapper_32banks() -> Mapper164 {
        Mapper164::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 32),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    /// Create a mapper with 128 PRG banks (2 MiB) for testing A20..A19 bits.
    fn make_mapper_128banks() -> Mapper164 {
        Mapper164::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 128),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Power-on / reset state ─────────────────────────────────────────────────

    #[test]
    fn power_on_low_bank_is_zero() {
        let mapper = make_mapper_32banks();
        // $8000-$BFFF must map to bank 0 on reset.
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            0,
            "$8000 must read bank 0 on power-on"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xBFFF),
            0,
            "$BFFF must read bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_high_bank_is_bank31() {
        let mapper = make_mapper_32banks();
        // $C000-$FFFF must be fixed at bank 31 (S=0 in UxROM mode, regs all zero).
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            31,
            "$C000 must read bank 31 on power-on (fixed high bank)"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xFFFF),
            31,
            "$FFFF must read bank 31 on power-on (fixed high bank)"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper_32banks();
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must be vertical (M=0)"
        );
    }

    // ── UxROM 16 KiB switchable lower bank ────────────────────────────────────

    #[test]
    fn uxrom_low_bank_switches_via_reg5000() {
        let mut mapper = make_mapper_32banks();
        // Select low bank 5 (M=0, Q=0, PPPp=0101 → bank 5).
        mapper.write_prg(0x5000, 0x05);
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            5,
            "$8000 must read bank 5 after writing $05 to $5000"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xBFFF),
            5,
            "$BFFF must read bank 5 after writing $05 to $5000"
        );
    }

    #[test]
    fn uxrom_high_bank_fixed_at_31_when_s0() {
        let mut mapper = make_mapper_32banks();
        // Write to $5000 with S=0 (bit 6 = 0). High bank stays at 31.
        mapper.write_prg(0x5000, 0x07); // bank 7, S=0
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            31,
            "$C000 must remain at bank 31 with S=0"
        );
    }

    #[test]
    fn uxrom_q_bit_selects_bank_16_through_31() {
        let mut mapper = make_mapper_32banks();
        // Q=1 (bit 5), PPPp=0000 → bank = 0b10000 = 16
        mapper.write_prg(0x5000, 0x20);
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            16,
            "$8000 must read bank 16 when Q=1, PPPp=0"
        );
    }

    #[test]
    fn uxrom_high_bank_with_s1_p0_is_bank28() {
        let mut mapper = make_mapper_32banks();
        // S=1 (bit 6), p=0 (bit 0 = 0) → high bank = 0b11100 = 28
        mapper.write_prg(0x5000, 0x40); // S=1, p=0
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            28,
            "$C000 must read bank 28 when S=1, p=0"
        );
    }

    #[test]
    fn uxrom_high_bank_with_s1_p1_is_bank30() {
        let mut mapper = make_mapper_32banks();
        // S=1 (bit 6), p=1 (bit 0 = 1) → high bank = 0b11110 = 30
        mapper.write_prg(0x5000, 0x41); // S=1, p=1
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            30,
            "$C000 must read bank 30 when S=1, p=1"
        );
    }

    // ── BxROM 32 KiB switchable bank ──────────────────────────────────────────

    #[test]
    fn bxrom_32kb_bank_switches_both_slots() {
        let mut mapper = make_mapper_32banks();
        // M=1 (bit 4), PPPp=0010 → 32KB bank 2 → low=bank 4, high=bank 5
        mapper.write_prg(0x5000, 0x12); // M=1, PPPp=2
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            4,
            "$8000 must read bank 4 (32KB bank 2 low) in BxROM mode"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            5,
            "$C000 must read bank 5 (32KB bank 2 high) in BxROM mode"
        );
    }

    #[test]
    fn bxrom_bank_zero_maps_to_banks_0_and_1() {
        let mut mapper = make_mapper_32banks();
        // M=1, PPPp=0 → 32KB bank 0 → low=bank 0, high=bank 1
        mapper.write_prg(0x5000, 0x10); // M=1, PPPp=0
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            0,
            "$8000 must read bank 0 in BxROM mode with PPPp=0"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            1,
            "$C000 must read bank 1 in BxROM mode with PPPp=0"
        );
    }

    // ── High address bits via $5100 ────────────────────────────────────────────

    #[test]
    fn reg5100_selects_high_bank_bits() {
        let mut mapper = make_mapper_128banks();
        // $5100 = 1 → high bits = 01 → low bank = bank 0 + 32 = 32
        mapper.write_prg(0x5100, 0x01);
        mapper.write_prg(0x5000, 0x00); // bank 0 within group, UxROM, S=0
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            32,
            "$8000 must read bank 32 with $5100=1 and $5000 bank=0"
        );
        // High bank ($C000): fixed 0b11111 within group → 32 + 31 = 63
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            63,
            "$C000 must read bank 63 with $5100=1 (fixed high bank in group)"
        );
    }

    #[test]
    fn reg5100_only_uses_low_2_bits() {
        let mut mapper = make_mapper_128banks();
        // Writing $FF to $5100 should only use bits [1:0] = 3 → group 3
        mapper.write_prg(0x5100, 0xFF);
        mapper.write_prg(0x5000, 0x00); // bank 0 within group
        // Group 3: low bank = 3*32 + 0 = 96
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            96,
            "$8000 must read bank 96 with $5100=FF (only [1:0] bits matter)"
        );
    }

    #[test]
    fn reg5100_bxrom_high_bits() {
        let mut mapper = make_mapper_128banks();
        // BxROM mode, $5100=2, PPPp=1 → 32KB bank = (2<<4)|1 = 33 → 16KB banks 66,67
        mapper.write_prg(0x5100, 0x02);
        mapper.write_prg(0x5000, 0x11); // M=1, PPPp=1
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            66,
            "$8000 must read bank 66 with $5100=2, BxROM PPPp=1"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            67,
            "$C000 must read bank 67 with $5100=2, BxROM PPPp=1"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_forced_vertical_when_m0() {
        let mut mapper = make_mapper_32banks();
        // M=0 (bit 4 = 0): forced vertical regardless of $5300
        mapper.write_prg(0x5300, 0x80); // set $5300 to horizontal bit
        mapper.write_prg(0x5000, 0x00); // M=0
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Must be vertical when M=0 regardless of $5300"
        );
    }

    #[test]
    fn mirroring_horizontal_when_m1_and_5300_bit7_clear() {
        let mut mapper = make_mapper_32banks();
        // M=1, $5300 bit7=0 → horizontal
        mapper.write_prg(0x5300, 0x00);
        mapper.write_prg(0x5000, 0x10); // M=1
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "Must be horizontal when M=1 and $5300 bit7=0"
        );
    }

    #[test]
    fn mirroring_vertical_when_m1_and_5300_bit7_set() {
        let mut mapper = make_mapper_32banks();
        // M=1, $5300 bit7=1 → vertical
        mapper.write_prg(0x5300, 0x80);
        mapper.write_prg(0x5000, 0x10); // M=1
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Must be vertical when M=1 and $5300 bit7=1"
        );
    }

    #[test]
    fn mirroring_updates_when_5300_written_with_m1() {
        let mut mapper = make_mapper_32banks();
        mapper.write_prg(0x5000, 0x10); // M=1
        mapper.write_prg(0x5300, 0x00); // horizontal
        assert_eq!(mapper.base().mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x5300, 0x80); // vertical
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    // ── Register address decoding (mask $FF00) ────────────────────────────────

    #[test]
    fn reg5000_mask_ff00_lower_byte_ignored() {
        let mut mapper = make_mapper_32banks();
        // Writing to $50FF should be treated the same as $5000.
        mapper.write_prg(0x50FF, 0x05);
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            5,
            "$50FF write must behave like $5000 write"
        );
    }

    #[test]
    fn reg5100_mask_ff00_lower_byte_ignored() {
        let mut mapper = make_mapper_128banks();
        // Writing to $51AB should be treated the same as $5100.
        mapper.write_prg(0x51AB, 0x01);
        mapper.write_prg(0x5000, 0x00);
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            32,
            "$51AB write must behave like $5100 write"
        );
    }

    // ── CHR-RAM ────────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_read_write() {
        let mut mapper = make_mapper_32banks();
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable and readable"
        );
    }

    #[test]
    fn chr_is_unbanked_8kb() {
        let mut mapper = make_mapper_32banks();
        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x1FFF, 0x22);
        assert_eq!(mapper.read_chr(0x0000), 0x11);
        assert_eq!(mapper.read_chr(0x1FFF), 0x22);
    }

    // ── PRG-RAM at $6000-$7FFF ────────────────────────────────────────────────

    #[test]
    fn prg_ram_read_write() {
        let mut mapper = Mapper164::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 32),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1),
        );
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0xFF),
            0x42,
            "PRG-RAM at $6000 must be readable after write"
        );
        mapper.write_prg(0x7FFF, 0xBB);
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, 0xFF),
            0xBB,
            "PRG-RAM at $7FFF must be readable after write"
        );
    }

    #[test]
    fn no_prg_ram_returns_open_bus() {
        let mut mapper = make_mapper_32banks(); // prg_ram_banks=0
        let result = mapper.read_prg_open_bus(0x6000, 0xDE);
        assert_eq!(
            result, 0xDE,
            "Without PRG-RAM, $6000 read must return open bus"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper_32banks();
        // Change state.
        mapper.write_prg(0x5000, 0x05); // bank 5, UxROM
        mapper.write_prg(0x5100, 0x01); // high bits
        mapper.write_prg(0x5300, 0x80); // mirroring
        // Reset.
        mapper.reset();
        // Verify power-on state.
        assert_eq!(
            mapper.base.read_prg_banked(0x8000),
            0,
            "$8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.base.read_prg_banked(0xC000),
            31,
            "$C000 must be bank 31 after reset"
        );
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be vertical after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper_128banks();
        mapper.write_prg(0x5000, 0x15); // M=1, PPPp=5
        mapper.write_prg(0x5100, 0x02); // high bits = 2
        mapper.write_prg(0x5300, 0x80); // vertical via $5300

        let snap = mapper.registers_snapshot();
        assert_eq!(snap.len(), 3);

        let mut restored = make_mapper_128banks();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg_open_bus(0x8000, 0),
            mapper.read_prg_open_bus(0x8000, 0),
            "Restored $8000 must match original"
        );
        assert_eq!(
            restored.read_prg_open_bus(0xC000, 0),
            mapper.read_prg_open_bus(0xC000, 0),
            "Restored $C000 must match original"
        );
        assert_eq!(
            restored.base().mirroring(),
            mapper.base().mirroring(),
            "Restored mirroring must match original"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper_32banks();
        let caps = mapper.capabilities();
        assert!(!caps.has_chr_banking, "Must not have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
        assert_eq!(caps.max_prg_ram_kb, 8, "Max PRG-RAM must be 8 KiB");
    }

    // ── create_mapper registration ────────────────────────────────────────────

    #[test]
    fn create_mapper_accepts_mapper_164() {
        let prg_rom = banked_data(PRG_BANK_SIZE, 32);
        let metadata = MapperContext::new_for_test(164, prg_rom, vec![], NametableLayout::Vertical);
        let result = create_mapper(metadata);
        assert!(result.is_ok(), "create_mapper must succeed for mapper 164");
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper_32banks();
        assert!(!mapper.irq_pending(), "Mapper 164 must never assert IRQ");
    }
}
