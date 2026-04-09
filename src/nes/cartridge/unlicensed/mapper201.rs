//! Mapper 201 – 8-in-1 / 21-in-1 multicart (NROM-256 multicart)
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_201.xhtml>
//! - Fallback: Mesen2 reference
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` latches address bits into the bank
//! register (the data byte is ignored, address lines only):
//!
//! ```text
//! A~[.... ....  RRRR RRRR]
//!               +--------- PRG A17..A15, CHR A15..A13 (bank select, bits [7:0])
//! ```
//!
//! **PRG banking (32 KB pages):**
//! - `$8000–$FFFF` is one 32 KB bank window, mapped to bank R.
//!
//! **CHR banking:** always 8 KB bank R at `$0000–$1FFF`.
//!
//! **Mirroring:** fixed; no dynamic control.
//!
//! All known games use only the two least significant bits (128 KiB PRG /
//! 32 KiB CHR = 4 banks).
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: bank 0.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 201;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 201 – 8-in-1 / 21-in-1 NROM-256 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper201 {
    base: BaseMapper,
    /// Bank register R = addr & 0xFF (bits [7:0] of write address).
    bank: u8,
}

impl Mapper201 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, bank: 0 };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.bank as i16);
        self.base.select_chr_page(0, self.bank as i16);
    }
}

impl Mapper for Mapper201 {
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
        if addr < 0x8000 {
            return;
        }
        // Data byte is ignored; banking is determined by address lines A[7:0].
        let _ = value;
        self.bank = (addr & 0x00FF) as u8;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&byte) = data.first() {
            self.bank = byte;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
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

    fn make_mapper() -> Mapper201 {
        Mapper201::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_201_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 201 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_ffff_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    // ── PRG banking (32 KB window) ────────────────────────────────────────────

    #[test]
    fn write_8001_selects_bank_1() {
        let mut mapper = make_mapper();
        // addr=0x8001: bits[7:0]=1 → bank 1
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must reflect PRG bank 1");
        assert_eq!(
            mapper.read_prg(0xFFFF),
            1,
            "$FFFF must also reflect PRG bank 1 (32 KB window)"
        );
    }

    #[test]
    fn write_8002_selects_bank_2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 window must be bank 2");
        assert_eq!(
            mapper.read_prg(0xFFFF),
            2,
            "$FFFF window must also be bank 2"
        );
    }

    #[test]
    fn prg_covers_full_32kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG start of window");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "PRG end of window");
    }

    #[test]
    fn write_8004_selects_bank_4_wraps_mod5() {
        let mut mapper = make_mapper();
        // bank=4, mod 5 = 4
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_prg(0x8000), 4);
    }

    #[test]
    fn write_8005_wraps_bank_to_0_mod5() {
        let mut mapper = make_mapper();
        // bank=5, mod 5 = 0
        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.read_prg(0x8000), 0, "bank 5 mod 5 = 0");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_matches_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must match the PRG bank index"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 4, "CHR end of window");
    }

    // ── Data byte ignored ─────────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 3);
        mapper.write_prg(0x8003, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "Data byte must be ignored");
    }

    // ── Writes below $8000 ignored ────────────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_change_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes below $8000 must not affect PRG bank"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes below $8000 must not affect CHR bank"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 201 must never assert IRQ");
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
        mapper.write_prg(0x8003, 0); // bank 3
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "PRG $FFFF must be bank 0 after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0); // bank 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.bank, mapper.bank, "Snapshot must preserve bank");
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data at $8000"
        );
        assert_eq!(
            restored.read_prg(0xFFFF),
            mapper.read_prg(0xFFFF),
            "Restored mapper must read same PRG data at $FFFF"
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
        let mut mapper = Mapper201::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
