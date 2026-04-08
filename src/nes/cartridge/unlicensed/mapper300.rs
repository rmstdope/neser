//! Mapper 300 – BMC-190in1
//!
//! Specifications:
//! - Primary reference: Mesen2 `Bmc190in1.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Bmc190in1.h>
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//!
//! # Hardware overview
//!
//! Used for 190-in-1 multicart boards.
//!
//! - PRG-ROM: two 16 KiB windows at $8000–$BFFF and $C000–$FFFF, always mapped
//!   to the same bank (NROM-128 style). Bank selected by bits [4:2] of the
//!   control register.
//! - CHR-ROM: 8 KiB window at $0000–$1FFF. Bank selected by the same bits [4:2].
//! - Mirroring: bit 0 controls nametable layout:
//!   - 1 → Horizontal
//!   - 0 → Vertical
//! - Control register: any write to $8000–$FFFF latches the written byte.
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Register format (written to $8000–$FFFF)
//!
//! | Bits | Effect                                                  |
//! |------|---------------------------------------------------------|
//! | 4:2  | 16 KiB PRG-ROM bank + 8 KiB CHR-ROM bank (same value) |
//! | 0    | Nametable mirroring: 1 = Horizontal, 0 = Vertical      |
//!
//! # Power-on state
//!
//! Register = 0: PRG bank 0 at both $8000 and $C000; CHR bank 0; vertical
//! mirroring (bit 0 = 0 → vertical).
//!
//! Mesen2 calls `WriteRegister(0x8000, 0)` on init, so:
//! - bank = (0 >> 2) & 7 = 0
//! - mirroring = bit 0 of 0 = 0 → vertical

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 300;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const BANK_MASK: u8 = 0x07; // bits [4:2] >> 2 → 3-bit bank index
const MIRROR_BIT: u8 = 0x01; // bit 0: 1 = Horizontal, 0 = Vertical

/// Mapper 300 – BMC-190in1
///
/// Control register layout (write to $8000–$FFFF):
/// - Bits [4:2] → 16 KiB PRG-ROM bank (same index for CHR)
/// - Bit  [0]   → Nametable mirroring: 1 = Horizontal, 0 = Vertical
pub struct Mapper300 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper300 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 16,
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
        let bank = ((reg >> 2) & BANK_MASK) as i16;
        self.base.select_prg_page(0, bank);
        self.base.select_prg_page(1, bank);
        self.base.select_chr_page(0, bank);
        // bit 0: 1 = Horizontal, 0 = Vertical
        self.base.set_mirroring_hv((reg & MIRROR_BIT) != 0);
    }
}

impl Mapper for Mapper300 {
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
    const PRG_BANKS_16K: usize = 11; // 11 × 16 KB
    const CHR_BANKS_8K: usize = 9; //  9 × 8 KB

    fn make_mapper() -> Mapper300 {
        Mapper300::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_300_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 300 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_0_at_both_windows() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte N, so bank 0 = 0x00
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be PRG bank 0 at power-on"
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
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        // Mesen2 init: WriteRegister(0x8000, 0) → bit 0 = 0 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be vertical at power-on (bit 0 = 0)"
        );
    }

    // ── PRG banking: bits [4:2] select 16 KB bank, both windows same ─────────

    #[test]
    fn both_prg_windows_mirror_same_bank() {
        let mut mapper = make_mapper();
        // bank = (value >> 2) & 7; write 0x0C → bank 3
        mapper.write_prg(0x8000, 0x0C);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 should be bank 3");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should also be bank 3");
    }

    #[test]
    fn prg_bank_selected_by_bits_4_2() {
        let mut mapper = make_mapper();
        // bank 5 → value = 5 << 2 = 0x14
        mapper.write_prg(0x8000, 0x14);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5");
    }

    #[test]
    fn prg_bank_bits_outside_4_2_are_ignored() {
        let mut mapper = make_mapper();
        // bit 0 = mirroring, bits 7:5 unused; only bits [4:2] select bank
        // bank 4 → 0x10; add bit 0 (mirror) and bit 7 (unused) → 0x91
        mapper.write_prg(0x8000, 0x91); // bits[4:2] = 0b100 = 4
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "only bits [4:2] select PRG bank"
        );
        assert_eq!(mapper.read_prg(0xC000), 4, "$C000 mirrors same bank");
    }

    #[test]
    fn register_written_at_any_address_in_8000_ffff() {
        let mut mapper = make_mapper();
        // bank 2 → 0x08; write to $FFFF
        mapper.write_prg(0xFFFF, 0x08);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "write to $FFFF should set bank 2"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_follows_same_bits_as_prg() {
        let mut mapper = make_mapper();
        // bank 6 → 0x18
        mapper.write_prg(0x8000, 0x18);
        assert_eq!(mapper.read_chr(0x0000), 6, "CHR should be bank 6");
    }

    #[test]
    fn chr_and_prg_always_use_same_bank_index() {
        let mut mapper = make_mapper();
        for b in 0u8..=7 {
            let value = b << 2; // bank = b, mirroring = 0
            mapper.write_prg(0x8000, value);
            assert_eq!(mapper.read_prg(0x8000), b, "PRG bank should be {b}");
            assert_eq!(
                mapper.read_chr(0x0000),
                b,
                "CHR bank should match PRG bank {b}"
            );
        }
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn bit0_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // bit 0 = 1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit0=1 should select horizontal mirroring"
        );
    }

    #[test]
    fn bit0_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // set horizontal first
        mapper.write_prg(0x8000, 0x00); // clear bit 0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit0=0 should select vertical mirroring"
        );
    }

    #[test]
    fn mirror_bit_does_not_affect_prg_or_chr_bank() {
        let mut mapper = make_mapper();
        // bank 3 + mirroring bit: bits[4:2]=3 → 0x0C, bit 0 = 1 → 0x0D
        mapper.write_prg(0x8000, 0x0D);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank should be 3 (not affected by bit 0)"
        );
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank should be 3");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_bank_and_mirroring() {
        let mut mapper = make_mapper();
        // bank 5, horizontal: (5 << 2) | 1 = 0x15
        mapper.write_prg(0x8000, 0x15);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            5,
            "restored PRG bank should be 5"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            5,
            "restored CHR bank should be 5"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "restored mirroring should be horizontal"
        );
    }

    #[test]
    fn restore_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        // bank 4, horizontal
        mapper.write_prg(0x8000, 0x10);
        mapper.restore_registers(&[]); // too short — must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "state must be unchanged after empty restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x15); // bank 5, horizontal
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical after reset (bit 0 = 0)"
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
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 300 must never assert IRQ");
    }
}
