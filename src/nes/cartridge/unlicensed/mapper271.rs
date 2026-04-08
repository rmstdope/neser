//! Mapper 271 – BMC-22026 (TXC MGC-026 4-in-1 multicart)
//!
//! Specifications:
//! - Primary reference: NesDev wiki (via Wayback Machine snapshot 2025-10-13)
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_271>
//!
//! # Hardware overview
//!
//! Used by the TXC 4-in-1 multicart (MGC-026, UNIF board BMC-22026).
//! It is described as "basically an oversize GNROM with one of the PRG bits
//! also serving as a mirroring control."
//!
//! - PRG-ROM: 32 KiB window at $8000–$FFFF. Bank selected by bits [5:4] of
//!   the data latch (2 bits → 4 banks × 32 KiB = 128 KiB).
//! - CHR-ROM: 8 KiB window at $0000–$1FFF. Bank selected by bits [3:0] of
//!   the data latch (4 bits → 16 banks × 8 KiB = 128 KiB).
//! - Mirroring: bit 6 of data latch; 0 = Horizontal, 1 = Vertical.
//! - Bit 7: unused.
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Register format (written to $8000–$FFFF)
//!
//! | Bits  | Effect                                                   |
//! |-------|----------------------------------------------------------|
//! | 3:0   | 8 KiB CHR-ROM bank at PPU $0000–$1FFF                   |
//! | 5:4   | 32 KiB PRG-ROM bank at CPU $8000–$FFFF                  |
//! | 6     | Nametable mirroring: 0 = Horizontal, 1 = Vertical       |
//! | 7     | Unused                                                   |
//!
//! # Power-on state
//!
//! Register = 0: PRG bank 0 at $8000–$FFFF; CHR bank 0; horizontal mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 271;
const PRG_BANK_SIZE_BYTES: usize = 32 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 271 – BMC-22026 TXC MGC-026 4-in-1 multicart.
///
/// Data latch format (write to $8000–$FFFF):
/// - Bits [3:0] = 8 KiB CHR bank
/// - Bits [5:4] = 32 KiB PRG bank
/// - Bit  [6]   = Mirroring: 0 = Horizontal, 1 = Vertical
pub struct Mapper271 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper271 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self { base, reg: 0 };
        mapper.apply_state(0);
        mapper
    }

    fn apply_state(&mut self, reg: u8) {
        self.reg = reg;
        let prg_bank = ((reg >> 4) & 0x03) as i16;
        let chr_bank = (reg & 0x0F) as i16;
        let vertical = (reg & 0x40) != 0;
        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);
        self.base.set_mirroring_hv(!vertical);
    }
}

impl Mapper for Mapper271 {
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
        if addr >= 0x8000 {
            self.apply_state(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.apply_state(data[0]);
    }

    fn reset(&mut self) {
        self.apply_state(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS_32K: usize = 5; // 5 × 32 KiB
    const CHR_BANKS_8K: usize = 11; // 11 × 8 KiB

    fn make_mapper() -> Mapper271 {
        Mapper271::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_271_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 271 must be registered in factory");
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_is_0() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte N; bank 0 = 0x00
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should map to PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF should also be in PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank should be 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        // Bit 6 = 0 at power-on → Horizontal mirroring
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should be horizontal at power-on (bit 6 = 0)"
        );
    }

    // ── PRG banking: bits [5:4] select 32 KiB bank ───────────────────────────

    #[test]
    fn prg_bank_selected_by_bits_5_4() {
        let mut mapper = make_mapper();
        // bank 1 → bits[5:4]=01 → value = 0x10
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG should be bank 1");

        // bank 2 → bits[5:4]=10 → value = 0x20
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG should be bank 2");

        // bank 3 → bits[5:4]=11 → value = 0x30
        mapper.write_prg(0x8000, 0x30);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG should be bank 3");
    }

    #[test]
    fn prg_bank_0_when_bits_5_4_are_clear() {
        let mut mapper = make_mapper();
        // Set a non-zero bank first
        mapper.write_prg(0x8000, 0x10); // bank 1
        // Now clear bits [5:4]
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 when bits [5:4] are clear"
        );
    }

    #[test]
    fn prg_32kb_window_covers_8000_to_ffff() {
        let mut mapper = make_mapper();
        // bank 2 → value = 0x20
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 should read bank 2");
        assert_eq!(mapper.read_prg(0xFFFF), 2, "$FFFF should read same bank 2");
    }

    #[test]
    fn prg_bank_ignores_bits_outside_5_4_and_6() {
        let mut mapper = make_mapper();
        // bank 1 with CHR bank 0xF and bit 7 set: value = 0x9F (bit 7 set, prg=01, chr=0xF)
        mapper.write_prg(0x8000, 0x9F); // bits[5:4]=01 → bank 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank 1 regardless of other bits"
        );
    }

    #[test]
    fn write_at_any_address_in_8000_ffff_updates_register() {
        let mut mapper = make_mapper();
        // bank 2 via write to $FFFF
        mapper.write_prg(0xFFFF, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "write to $FFFF should set PRG bank 2"
        );
    }

    // ── CHR banking: bits [3:0] select 8 KiB bank ────────────────────────────

    #[test]
    fn chr_bank_selected_by_bits_3_0() {
        let mut mapper = make_mapper();
        // CHR bank 5 → bits[3:0]=0101 → value = 0x05
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR should be bank 5");

        // CHR bank 10 → bits[3:0]=1010 → value = 0x0A
        mapper.write_prg(0x8000, 0x0A);
        assert_eq!(mapper.read_chr(0x0000), 10, "CHR should be bank 10");
    }

    #[test]
    fn chr_bank_0_when_bits_3_0_are_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0F); // CHR bank 15
        mapper.write_prg(0x8000, 0x00); // CHR bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 when bits [3:0] are clear"
        );
    }

    #[test]
    fn chr_8kb_window_covers_0000_to_1fff() {
        let mut mapper = make_mapper();
        // CHR bank 3 → value = 0x03
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "$0000 should read CHR bank 3");
        assert_eq!(mapper.read_chr(0x1FFF), 3, "$1FFF should read CHR bank 3");
    }

    #[test]
    fn prg_and_chr_bank_are_independent() {
        let mut mapper = make_mapper();
        // PRG bank 2, CHR bank 7 → value = bits[5:4]=10, bits[3:0]=0111 → 0x27
        mapper.write_prg(0x8000, 0x27);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG should be bank 2");
        assert_eq!(mapper.read_chr(0x0000), 7, "CHR should be bank 7");
    }

    // ── Mirroring: bit 6 ─────────────────────────────────────────────────────

    #[test]
    fn bit6_set_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x40); // bit 6 = 1 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit6=1 should select vertical mirroring"
        );
    }

    #[test]
    fn bit6_clear_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x40); // set vertical first
        mapper.write_prg(0x8000, 0x00); // clear bit 6
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit6=0 should select horizontal mirroring"
        );
    }

    #[test]
    fn mirror_bit_does_not_affect_prg_or_chr_bank() {
        let mut mapper = make_mapper();
        // PRG bank 1, CHR bank 3, mirroring=vertical: value = 0x10 | 0x03 | 0x40 = 0x53
        mapper.write_prg(0x8000, 0x53);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank should be 1 (not affected by bit 6)"
        );
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank should be 3");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical"
        );
    }

    #[test]
    fn bit7_is_unused_and_does_not_affect_behavior() {
        let mut mapper = make_mapper();
        // PRG bank 1 with bit7 set: value = 0x10 | 0x80 = 0x90
        mapper.write_prg(0x8000, 0x90);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank should be 1 regardless of bit 7"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be horizontal (bit6=0) despite bit7 set"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();
        // PRG bank 2, CHR bank 9, vertical: 0x20 | 0x09 | 0x40 = 0x69
        mapper.write_prg(0x8000, 0x69);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            2,
            "restored PRG bank should be 2"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            9,
            "restored CHR bank should be 9"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "restored mirroring should be vertical"
        );
    }

    #[test]
    fn restore_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // PRG bank 1
        mapper.restore_registers(&[]); // empty → must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "state must be unchanged after empty restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x77); // PRG bank 3, CHR bank 7, vertical
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be horizontal after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(caps.has_chr_banking, "CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 32);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 271 must never assert IRQ");
    }
}
