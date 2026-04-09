//! Mapper 240 – Jing Ke Xin Zhuan / Sheng Huo Lie Zhuan
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_240.xhtml>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper240.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper240.h>
//!
//! ## Hardware behavior
//!
//! A single write-only register decoded at `$4020–$5FFF`:
//!
//! ```text
//! $4020–$5FFF  [PPPP CCCC]
//!   P = PRG bank (bits 7–4) → selects 32 KB PRG bank at $8000–$FFFF
//!   C = CHR bank (bits 3–0) → selects 8 KB CHR bank at $0000–$1FFF
//! ```
//!
//! **PRG banking:** one 32 KB switchable window at `$8000–$FFFF`.
//! Writes outside `$4020–$5FFF` do not update the bank registers.
//!
//! **CHR banking:** one 8 KB switchable window at `$0000–$1FFF`.
//!
//! **PRG-RAM:** 8 KB at `$6000–$7FFF` (battery-backed on known carts).
//!
//! **Mirroring:** fixed from the cartridge header; no dynamic control.
//!
//! **IRQ:** none.
//!
//! **Power-on/reset state:** PRG bank 0, CHR bank 0.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 240;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 240 – GNROM-like 32 KB PRG + 8 KB CHR bank switcher.
///
/// See the module-level documentation for hardware details.
pub struct Mapper240 {
    base: BaseMapper,
    /// PRG bank (bits 7–4 of the written byte); selects a 32 KB bank at $8000.
    prg_bank: u8,
    /// CHR bank (bits 3–0 of the written byte); selects an 8 KB bank at $0000.
    chr_bank: u8,
}

impl Mapper240 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper240 {
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
            0x4020..=0x5FFF => {
                self.prg_bank = (value >> 4) & 0x0F;
                self.chr_bank = value & 0x0F;
                self.update_banks();
            }
            _ => {
                self.base.try_write_prg_ram(addr, value);
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0] & 0x0F;
            self.chr_bank = data[1] & 0x0F;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 5;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper240 {
        Mapper240::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_240_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );
        assert!(
            result.is_ok(),
            "Mapper 240 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    // ── PRG banking ──────────────────────────────────────────────────────────

    #[test]
    fn write_selects_prg_bank_from_upper_nibble() {
        let mut mapper = make_mapper();
        // value = 0x10: P=1, C=0 → PRG bank 1, CHR bank 0
        mapper.write_prg(0x4020, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank must be 1");
    }

    #[test]
    fn write_selects_prg_bank_2() {
        let mut mapper = make_mapper();
        // value = 0x20: P=2, C=0 → PRG bank 2
        mapper.write_prg(0x5000, 0x20);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank must be 2");
    }

    #[test]
    fn prg_bank_covers_full_32kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x10); // PRG bank 1
        assert_eq!(mapper.read_prg(0x8000), 1, "Start of 32 KB window");
        assert_eq!(mapper.read_prg(0xFFFF), 1, "End of 32 KB window");
    }

    #[test]
    fn writes_to_8000_do_not_update_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x10); // PRG bank 1
        mapper.write_prg(0x8000, 0x20); // Should not update
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Writes to $8000 must not change PRG bank"
        );
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn write_selects_chr_bank_from_lower_nibble() {
        let mut mapper = make_mapper();
        // value = 0x01: P=0, C=1 → PRG bank 0, CHR bank 1
        mapper.write_prg(0x4020, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank must be 1");
    }

    #[test]
    fn write_selects_chr_bank_3() {
        let mut mapper = make_mapper();
        // value = 0x03: P=0, C=3 → CHR bank 3
        mapper.write_prg(0x5FFF, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank must be 3");
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x02); // CHR bank 2
        assert_eq!(mapper.read_chr(0x0000), 2, "Start of CHR window");
        assert_eq!(mapper.read_chr(0x1FFF), 2, "End of CHR window");
    }

    #[test]
    fn prg_and_chr_banks_are_independent() {
        let mut mapper = make_mapper();
        // value = 0x32: P=3, C=2 → PRG bank 3, CHR bank 2
        mapper.write_prg(0x4020, 0x32);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank must be 3");
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank must be 2");
    }

    // ── PRG-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_is_accessible_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG-RAM at $6000 must be writable and readable"
        );
    }

    #[test]
    fn prg_ram_at_7fff_is_accessible() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCD, "PRG-RAM at $7FFF must work");
    }

    // ── Register address range ────────────────────────────────────────────────

    #[test]
    fn write_at_4020_updates_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x12); // P=1, C=2
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn write_at_5fff_updates_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5FFF, 0x21); // P=2, C=1
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn write_below_4020_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x10); // P=1, C=0
        // Write below $4020 should be ignored by register logic
        // (address $4000 is below the mapper's register range)
        mapper.write_prg(0x4000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank must remain 1 after non-register write"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank must remain 0 after non-register write"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 240 must never assert IRQ");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(
            !caps.has_dynamic_mirroring,
            "Must not have dynamic mirroring"
        );
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x34); // P=3, C=4
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG must be bank 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x23); // P=2, C=3
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.prg_bank, 2, "Snapshot must preserve PRG bank");
        assert_eq!(restored.chr_bank, 3, "Snapshot must preserve CHR bank");
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must read same CHR data"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper240::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
