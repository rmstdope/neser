//! Mapper 299 – BMC-11160 (TXC)
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki unavailable (403).
//! - Fallback source: Mesen2 `Core/NES/Mappers/Txc/Bmc11160.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Txc/Bmc11160.h>
//!
//! ## Hardware overview
//!
//! The BMC-11160 is a simple TXC multicart board providing:
//!
//! - 1 × 32 KB switchable PRG-ROM slot ($8000–$FFFF)
//! - 1 × 8 KB switchable CHR-ROM/RAM slot ($0000–$1FFF)
//! - Switchable H/V mirroring
//! - No PRG-RAM, no IRQ, no expansion audio
//!
//! ## Register
//!
//! Any write to $8000–$FFFF:
//!
//! | Bits  | Function                                          |
//! |-------|---------------------------------------------------|
//! | 7     | Mirroring: 1 = Vertical, 0 = Horizontal           |
//! | 6:4   | PRG bank select (3 bits → 8 × 32 KB banks)        |
//! | 3:2   | Unused                                            |
//! | 1:0   | CHR bank low bits (combined with PRG bank)        |
//!
//! The CHR bank is computed as `(prg_bank << 2) | (value & 0x03)`.
//!
//! ## Power-on / reset state
//!
//! On reset, a write of 0 to $8000 is simulated:
//! PRG bank 0, CHR bank 0, horizontal mirroring.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 299;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 299 – BMC-11160
///
/// See the module-level documentation for hardware details.
pub struct Mapper299 {
    base: BaseMapper,
    /// Current register value (latched on every write to $8000–$FFFF).
    register: u8,
}

impl Mapper299 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self { base, register: 0 };
        mapper.apply_register();
        mapper
    }

    fn apply_register(&mut self) {
        let prg_bank = ((self.register >> 4) & 0x07) as i16;
        let chr_bank = ((prg_bank << 2) | ((self.register & 0x03) as i16)) & 0xFF;
        let vertical = (self.register & 0x80) != 0;
        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);
        self.base.set_mirroring_hv(!vertical);
    }
}

impl Mapper for Mapper299 {
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
        self.register = value;
        self.apply_register();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&reg) = data.first() {
            self.register = reg;
            self.apply_register();
        }
    }

    fn reset(&mut self) {
        self.register = 0;
        self.apply_register();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to catch modulo-wrapping bugs.
    const PRG_BANKS: usize = 9;
    const CHR_BANKS: usize = 37; // 8 PRG × 4 CHR sub-banks = 32; use 37 to be safe

    fn make_mapper() -> Mapper299 {
        Mapper299::new(
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
    fn mapper_299_is_registered() {
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
            "Mapper 299 must be registered in the factory"
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
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF must map to PRG bank 0 at power-on"
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

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Power-on mirroring must be Horizontal (bit 7 = 0)"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_selects_prg_bank_via_bits_6_to_4() {
        let mut mapper = make_mapper();
        // value = 0x30 → bits[6:4] = 0b011 = 3
        mapper.write_prg(0x8000, 0x30);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank should be (value >> 4) & 0x07"
        );
    }

    #[test]
    fn prg_bank_uses_only_3_bits() {
        let mut mapper = make_mapper();
        // value = 0x80 → bits[6:4] = 0b000 = 0 (bit 7 is mirroring, not bank)
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Bit 7 must not affect PRG bank selection"
        );
    }

    #[test]
    fn write_selects_each_prg_bank_0_to_7() {
        let mut mapper = make_mapper();
        for bank in 0u8..8 {
            mapper.write_prg(0x8000, bank << 4);
            assert_eq!(
                mapper.read_prg(0x8000),
                bank,
                "PRG bank {bank} must be selectable"
            );
        }
    }

    #[test]
    fn prg_bank_covers_full_32kb_window() {
        let mut mapper = make_mapper();
        // Select bank 2
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG window start");
        assert_eq!(mapper.read_prg(0xFFFF), 2, "PRG window end");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_is_prg_bank_shifted_with_low_bits() {
        let mut mapper = make_mapper();
        // value = 0x12: prg_bank = 1, chr_bank = (1<<2) | (2 & 0x03) = 4|2 = 6
        // But CHR_BANKS=37 so bank 6 exists.
        mapper.write_prg(0x8000, 0x12);
        assert_eq!(
            mapper.read_chr(0x0000),
            6,
            "CHR bank = (prg_bank << 2) | (value & 0x03)"
        );
    }

    #[test]
    fn chr_bank_low_bits_from_value_bits_1_0() {
        let mut mapper = make_mapper();
        // value = 0x03: prg_bank=0, chr_bank=(0<<2)|3 = 3
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR low bits from value[1:0]");
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // prg=1, chr=(1<<2)|0=4
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR window start");
        assert_eq!(mapper.read_chr(0x1FFF), 4, "CHR window end");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn bit_7_set_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x80);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Bit 7 = 1 must select Vertical mirroring"
        );
    }

    #[test]
    fn bit_7_clear_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Bit 7 = 0 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_and_banking_set_simultaneously() {
        let mut mapper = make_mapper();
        // value = 0xB1 = 1011_0001: bit7=1(V), prg=(0110>>4 & 7)=?
        // Actually: 0xB1 = 1011_0001 → bit7=1, bits6:4=011=3, bits1:0=01
        // prg=3, chr=(3<<2)|1=13, mirroring=Vertical
        mapper.write_prg(0x8000, 0xB1);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank = 3");
        assert_eq!(mapper.read_chr(0x0000), 13, "CHR bank = 13");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring = Vertical"
        );
    }

    // ── Write below $8000 has no effect ───────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_change_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x30); // Should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Write below $8000 must not change PRG bank"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Write below $8000 must not change CHR bank"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 299 must never assert IRQ");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xF3); // set some state
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must return to bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR must return to bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be Horizontal after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xB2); // prg=3, chr=14, vertical

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG bank survives snapshot round-trip"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "CHR bank survives snapshot round-trip"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring survives snapshot round-trip"
        );
    }

    #[test]
    fn restore_with_empty_snapshot_does_not_panic() {
        let mut mapper = make_mapper();
        mapper.restore_registers(&[]);
        // Should not panic; state is unchanged
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_writable_when_no_chr_rom() {
        let mut mapper = Mapper299::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 4),
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
