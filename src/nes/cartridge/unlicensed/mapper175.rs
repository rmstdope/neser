//! Mapper 175 – Kaiser KS-7022
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_175>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Kaiser/Kaiser7022.h`
//!
//! Known Limitations:
//! - In hardware, bank activation occurs when the CPU reads address `$FFFC`
//!   (the reset vector).  Because `read_prg` takes `&self` in this codebase,
//!   banks are instead committed immediately when the bank register is written
//!   (`$8001–$FFFF`).  This is functionally equivalent for all known software.
//!
//! ## Overview
//!
//! Mapper 175 is used by the Kaiser KS-7022 board, a single-game board with a
//! copy-protection mechanism: the PRG/CHR bank selected by the register does
//! not take effect until the CPU reads the 6502 reset vector at `$FFFC`.
//!
//! ## Register Map
//!
//! | Address       | Access | Effect                                           |
//! |---------------|--------|--------------------------------------------------|
//! | `$8000`       | Write  | Mirroring: bit 2 → 1 = Horizontal, 0 = Vertical |
//! | `$8001–$FFFF` | Write  | PRG/CHR bank register (bits 3:0)                 |
//!
//! ## PRG Banking
//!
//! Two 16 KiB slots:
//! - Power-on: slot 0 = bank 0, slot 1 = last bank.
//! - After bank register write: both slots map to the same bank (register value).
//!
//! ## CHR Banking
//!
//! One 8 KiB slot mapped to the same bank register value.
//!
//! ## Mirroring
//!
//! Software-controlled via bit 2 of the value written to `$8000`.
//!
//! ## Reset
//!
//! Bank register resets to 0; both PRG slots and CHR become bank 0.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 175;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 175 – Kaiser KS-7022.
///
/// See the module-level documentation for hardware details.
pub struct Mapper175 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper175 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        // Power-on: slot 0 = bank 0, slot 1 = last bank (copy-protection bootstrap).
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -1);
        base.select_chr_page(0, 0);
        Self { base, reg: 0 }
    }

    /// Apply the current bank register to both PRG slots and CHR.
    fn activate_banks(&mut self) {
        let bank = self.reg as i16;
        self.base.select_prg_page(0, bank);
        self.base.select_prg_page(1, bank);
        self.base.select_chr_page(0, bank);
    }
}

impl Mapper for Mapper175 {
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
        match addr {
            0x8000 => self.base.set_mirroring_hv((value >> 2) & 1 != 0),
            0x8001..=0xFFFF => {
                self.reg = value & 0x0F;
                self.activate_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&reg) = data.first() {
            self.reg = reg & 0x0F;
            self.activate_banks();
        }
    }

    fn reset(&mut self) {
        self.reg = 0;
        self.activate_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 5;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper175 {
        Mapper175::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_175_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 175 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_slot0_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 (slot 0) must map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_slot1_is_last_bank() {
        let mapper = make_mapper();
        let last_bank = (PRG_BANKS - 1) as u8;
        assert_eq!(
            mapper.read_prg(0xC000),
            last_bank,
            "$C000 (slot 1) must map to the last bank at power-on"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 at power-on");
    }

    #[test]
    fn power_on_mirroring_matches_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must match the iNES header at power-on"
        );
    }

    // ── Mirroring ($8000 write) ───────────────────────────────────────────────

    #[test]
    fn write_8000_bit2_1_sets_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0100); // bit 2 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit 2 = 1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn write_8000_bit2_0_sets_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0100); // set H first
        mapper.write_prg(0x8000, 0b0000_0000); // clear bit 2 → V
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit 2 = 0 must select Vertical mirroring"
        );
    }

    #[test]
    fn write_8000_does_not_change_prg_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Write to $8000 must not change PRG banks"
        );
    }

    // ── Bank register ($8001–$FFFF writes) ───────────────────────────────────

    #[test]
    fn bank_register_applies_both_prg_slots() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3, "Slot 0 must be bank 3");
        assert_eq!(mapper.read_prg(0xC000), 3, "Slot 1 must be bank 3");
    }

    #[test]
    fn bank_register_uses_only_lower_nibble() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0xF2); // upper nibble ignored → bank 2
        assert_eq!(mapper.read_prg(0x8000), 2, "Upper nibble must be ignored");
        assert_eq!(mapper.read_prg(0xC000), 2);
    }

    #[test]
    fn bank_register_applies_chr_slot() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x04);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR must follow the same bank register"
        );
    }

    #[test]
    fn any_non_8000_write_sets_bank_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x02); // $FFFF also sets reg
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 2);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_applies_bank_0_to_both_slots() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x03);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Slot 0 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "Slot 1 must be bank 0 after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x03);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG $8000 must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored PRG $C000 must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR must match"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x03);
        assert!(!mapper.irq_pending(), "Mapper 175 must never assert IRQ");
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper175::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB);
    }
}
