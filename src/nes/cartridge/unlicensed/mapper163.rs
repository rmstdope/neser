//! Mapper 163 – Nanjing FC-001
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_163>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Nanjing.h`
//!
//! Known Limitations:
//! - CHR auto-switch (PPU scanline-based 4 KiB switching) is not implemented;
//!   static CHR-RAM page 0 is used for both 4 KiB halves.
//! - Submapper 1 (ADPCM NJ-YUYIN0106) is not implemented.
//!
//! ## Overview
//!
//! Mapper 163 is used by most Nanjing FC-001 educational cartridges (Harvest Moon,
//! Diablo, etc.). It provides a 32 KiB switchable PRG-ROM window, 8 KiB unbanked
//! CHR-RAM with optional auto-switch, battery-backed PRG-RAM, and hardware-wired
//! nametable mirroring.
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: 8 KiB unbanked PRG-RAM (battery-backed)
//! * `CPU $8000–$FFFF`: 32 KiB switchable PRG-ROM bank
//! * `PPU $0000–$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! ## Registers (CPU `$5000–$5FFF`)
//!
//! Register decode uses `addr & 0x7300` (except `$5101` which is matched exactly):
//!
//! | Address mask | Reg | Bits used |
//! |---|---|---|
//! | `$5000` | reg0 | `C... PPPP` |
//! | `$5100` | reg1 | `.... .F.E` (latch; special protection) |
//! | `$5101` | prev5101 | toggle latch (nonzero→zero transition toggles flag) |
//! | `$5200` | reg2 | `.... PPPP` |
//! | `$5300` | reg3 | mode (ignored by PRG bank calc; stored only) |
//!
//! ## PRG Banking (Mesen formula)
//!
//! ```text
//! bank = (reg0 & 0x0F) | ((reg2 & 0x0F) << 4)
//! ```
//!
//! Special case: writing value 6 to `$5100` forces PRG to bank 3.
//!
//! ## Protection / Feedback Mechanism
//!
//! * Read at `addr & 0x7700 == $5100`: returns `reg3 | reg1 | reg0 | (reg2 ^ 0xFF)`
//! * Read at `addr & 0x7700 == $5500`: returns `reg3 | reg0` if toggle else 0
//! * Read at anything else in `$5000-$5FFF`: returns 4
//!
//! `toggle` is initialized to `true` and flips whenever `$5101` is written
//! with the previous value non-zero and the new value zero.
//!
//! ## Power-on / Reset State
//!
//! Following Mesen: all registers cleared, toggle = true, bank 0 selected.
//! NESdev notes that with mode-register A=0, PRG A15/A16 are fixed to 11b,
//! giving boot bank 3; this mode-register behavior is not implemented here.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 163;
const PRG_BANK_SIZE: usize = 32 * 1024;

/// Mapper 163 – Nanjing FC-001.
///
/// See the module-level documentation for hardware details.
pub struct Mapper163 {
    base: BaseMapper,
    /// Register 0 (`$5000`): PRG bank low + CHR auto-switch control.
    reg0: u8,
    /// Register 1 (`$5100`): feedback latch.
    reg1: u8,
    /// Register 2 (`$5200`): PRG bank high.
    reg2: u8,
    /// Register 3 (`$5300`): mode (stored but not used in bank calc).
    reg3: u8,
    /// Last byte written to `$5101`; used to detect nonzero→zero transitions.
    prev5101: u8,
    /// Protection toggle flag; flips on nonzero→zero `$5101` write.
    toggle: bool,
}

impl Mapper163 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut ctx = ctx;
        // Force CHR-RAM (chip-internal).
        ctx.chr_rom = vec![];
        // Mapper 163 always provides 8 KiB of PRG-RAM at $6000-$7FFF.
        if ctx.prg_ram_banks_8k == 0 {
            ctx.prg_ram_banks_8k = 1;
        }
        ctx.prg_ram_size_specified = true;

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);

        let mut mapper = Self {
            base,
            reg0: 0,
            reg1: 0,
            reg2: 0,
            reg3: 0,
            prev5101: 0,
            // Mesen initializes toggle to true.
            toggle: true,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let bank = ((self.reg0 & 0x0F) as usize) | (((self.reg2 & 0x0F) as usize) << 4);
        self.base.select_prg_page(0, bank as i16);
    }
}

impl Mapper for Mapper163 {
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
        if !(0x5000..=0x5FFF).contains(&addr) {
            return;
        }
        if addr == 0x5101 {
            // Toggle when previous value was nonzero and new value is zero.
            if self.prev5101 != 0 && value == 0 {
                self.toggle = !self.toggle;
            }
            self.prev5101 = value;
            return;
        }
        // Special protection bypass: value 6 written to any $51xx address
        // (excluding $5101 already handled above) forces PRG to bank 3.
        if addr & 0x7300 == 0x5100 && value == 6 {
            self.reg1 = value;
            self.base.select_prg_page(0, 3);
            return;
        }
        match addr & 0x7300 {
            0x5000 => {
                self.reg0 = value;
                self.update_banks();
            }
            0x5100 => {
                self.reg1 = value;
            }
            0x5200 => {
                self.reg2 = value;
                self.update_banks();
            }
            0x5300 => {
                self.reg3 = value;
            }
            _ => {}
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x5000..=0x5FFF).contains(&addr) {
            return match addr & 0x7700 {
                0x5100 => self.reg3 | self.reg1 | self.reg0 | (self.reg2 ^ 0xFF),
                0x5500 => {
                    if self.toggle {
                        self.reg3 | self.reg0
                    } else {
                        0
                    }
                }
                _ => 4,
            };
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.reg0,
            self.reg1,
            self.reg2,
            self.reg3,
            self.prev5101,
            self.toggle as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.reg0 = data[0];
            self.reg1 = data[1];
            self.reg2 = data[2];
            self.reg3 = data[3];
            self.prev5101 = data[4];
            self.toggle = data[5] != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg0 = 0;
        self.reg1 = 0;
        self.reg2 = 0;
        self.reg3 = 0;
        self.prev5101 = 0;
        self.toggle = true;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 11;

    fn make_mapper() -> Mapper163 {
        Mapper163::new(
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
    fn mapper_163_is_registered() {
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
            "Mapper 163 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_bank_is_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn power_on_toggle_is_true() {
        let mapper = make_mapper();
        assert!(mapper.toggle);
    }

    // ── PRG banking ────────────────────────────────────────────────────────────

    #[test]
    fn reg0_low_nibble_selects_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 5);
        assert_eq!(mapper.read_prg(0x8000), (5 % PRG_BANKS) as u8);
    }

    #[test]
    fn reg2_low_nibble_provides_outer_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0);
        mapper.write_prg(0x5200, 1); // outer = 1 << 4 = 16
        assert_eq!(mapper.read_prg(0x8000), (16 % PRG_BANKS) as u8);
    }

    #[test]
    fn bank_combines_reg0_and_reg2() {
        let mut mapper = make_mapper();
        // bank = (3 & 0x0F) | ((1 & 0x0F) << 4) = 3 | 16 = 19
        mapper.write_prg(0x5000, 3);
        mapper.write_prg(0x5200, 1);
        assert_eq!(mapper.read_prg(0x8000), (19 % PRG_BANKS) as u8);
    }

    // ── Protection: $5100 write value=6 forces bank 3 ────────────────────────

    #[test]
    fn write_6_to_5100_forces_bank3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 7); // select bank 7 first
        mapper.write_prg(0x5100, 6); // special: force bank 3
        assert_eq!(mapper.read_prg(0x8000), (3 % PRG_BANKS) as u8);
    }

    #[test]
    fn write_6_to_51xx_mirror_also_forces_bank3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 7);
        // $5180 & 0x7300 = 0x5100 → same effect as $5100
        mapper.write_prg(0x5180, 6);
        assert_eq!(mapper.read_prg(0x8000), (3 % PRG_BANKS) as u8);
    }

    #[test]
    fn write_6_to_5100_also_updates_reg1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5100, 6);
        // Protection read at $5100 must reflect reg1=6
        let result = mapper.read_prg_open_bus(0x5100, 0x00);
        assert_eq!(
            result & 0x06,
            0x06,
            "reg1=6 should be reflected in protection read"
        );
    }

    // ── Toggle mechanism ($5101) ───────────────────────────────────────────────

    #[test]
    fn toggle_flips_on_nonzero_to_zero_write() {
        let mut mapper = make_mapper();
        assert!(mapper.toggle); // starts true
        mapper.write_prg(0x5101, 0x01); // prev=0→1 (no flip, was 0)
        assert!(mapper.toggle);
        mapper.write_prg(0x5101, 0x00); // prev=1→0 (flip!)
        assert!(!mapper.toggle);
    }

    #[test]
    fn toggle_does_not_flip_on_zero_to_zero_write() {
        let mut mapper = make_mapper();
        let initial = mapper.toggle;
        mapper.write_prg(0x5101, 0x00); // prev=0→0 (no flip)
        assert_eq!(mapper.toggle, initial);
    }

    #[test]
    fn toggle_double_flip_restores_initial_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5101, 0x01);
        mapper.write_prg(0x5101, 0x00); // flip 1
        mapper.write_prg(0x5101, 0x01);
        mapper.write_prg(0x5101, 0x00); // flip 2
        assert!(mapper.toggle); // back to initial
    }

    // ── Protection reads ───────────────────────────────────────────────────────

    #[test]
    fn read_5100_returns_protection_data() {
        let mut mapper = make_mapper();
        // reg3=0, reg1=0, reg0=5, reg2=3 → 0|0|5|(3^0xFF) = 5|252 = 0xFD
        mapper.write_prg(0x5000, 5);
        mapper.write_prg(0x5200, 3);
        let result = mapper.read_prg_open_bus(0x5100, 0x00);
        assert_eq!(result, 5 | (3 ^ 0xFF));
    }

    #[test]
    fn read_5500_returns_reg_data_when_toggle_true() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x02); // reg0=2
        mapper.write_prg(0x5300, 0x04); // reg3=4
        // toggle=true (power-on state)
        let result = mapper.read_prg_open_bus(0x5500, 0x00);
        assert_eq!(result, 0x04 | 0x02); // reg3|reg0
    }

    #[test]
    fn read_5500_returns_0_when_toggle_false() {
        let mut mapper = make_mapper();
        // toggle starts true; flip it to false
        mapper.write_prg(0x5101, 0x01);
        mapper.write_prg(0x5101, 0x00);
        assert!(!mapper.toggle);
        let result = mapper.read_prg_open_bus(0x5500, 0x55);
        assert_eq!(result, 0);
    }

    #[test]
    fn read_other_5xxx_returns_4() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg_open_bus(0x5200, 0x00), 4);
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x00), 4);
    }

    // ── PRG-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_readable_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x99);
        assert_eq!(mapper.read_prg(0x6000), 0x99);
    }

    #[test]
    fn prg_ram_present_even_when_header_omits_size() {
        // Verifies that PRG-RAM is always allocated even when the iNES header
        // doesn't explicitly specify PRG-RAM size (prg_ram_size_specified=false).
        let mut mapper = Mapper163::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1)
            .with_unspecified_prg_ram_size(),
        );
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "$6000 PRG-RAM must work regardless of header"
        );
    }

    // ── CHR-RAM ────────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0400, 0x77);
        assert_eq!(mapper.read_chr(0x0400), 0x77);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 7);
        mapper.write_prg(0x5200, 2);
        mapper.write_prg(0x5101, 1);
        mapper.write_prg(0x5101, 0); // toggle flipped
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert!(mapper.toggle);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 3);
        mapper.write_prg(0x5200, 1);
        mapper.write_prg(0x5101, 1);
        mapper.write_prg(0x5101, 0); // toggle flipped

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.toggle, mapper.toggle);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }
}
